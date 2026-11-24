"""Pure-stdlib HTTP client for the OpenVietGIS R026 QGIS Connector.

This module intentionally has ZERO PyQt or QGIS imports so it can be unit tested
in any standard Python environment. It enforces:
- HTTPS by default (optional http://localhost or http://127.0.0.1 for local dev only)
- Strict UUID, SHA-256 revision, operation_id, and origin validation with zero
  credential/query/fragment leakage
- Fail-closed SHA-256 verification on GeoPackage downloads
- Path-traversal-safe filename sanitization and atomic local file writes
- Read-only layer export by default plus explicit two-step GeoPackage sync
  (`sync-preview` -> confirmed `sync-push` via R006 changesets; deletions blocked)
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import secrets
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from uuid import UUID

USER_AGENT = "OpenVietGIS-QGIS-Connector/1.0.0"
DEFAULT_PROFILE_TIMEOUT_SECONDS = 30.0
DEFAULT_EXPORT_TIMEOUT_SECONDS = 60.0
DEFAULT_SYNC_TIMEOUT_SECONDS = 60.0
MAX_SYNC_UPLOAD_BYTES = 16 * 1024 * 1024

_UUID_RE = re.compile(
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)
_SHA256_RE = re.compile(r"^[a-fA-F0-9]{64}$")
_OPERATION_ID_RE = re.compile(r"^[A-Za-z0-9_.:-]{1,128}$")
_RESERVED_OPERATION_ID_PREFIX = "field-sync-"
_SAFE_GPKG_FILENAME_RE = re.compile(r"^[A-Za-z0-9._-]+\.gpkg$")
_UNSAFE_FILENAME_CHARS_RE = re.compile(r"[^A-Za-z0-9._-]+")
_CD_FILENAME_QUOTED_RE = re.compile(r'(?i)(?:^|;\s*)filename="([^"\r\n]+)"')
_CD_FILENAME_UNQUOTED_RE = re.compile(r"(?i)(?:^|;\s*)filename=([^;\r\n]+)")
_CD_FILENAME_STAR_RE = re.compile(r"(?i)(?:^|;\s*)filename\*=(?:UTF-8|utf-8)''([^;\r\n]+)")
_LOCALHOST_HOSTS = frozenset({"localhost", "127.0.0.1"})


class OpenVietGisError(Exception):
    """Base exception for OpenVietGIS read-only connector errors."""


def decode_pairing_code(raw: str) -> dict[str, str]:
    """Decode an ovg-pair-v1:<base64url> pairing bundle issued by OpenVietGIS R027."""
    token = raw.strip()
    if not token.startswith("ovg-pair-v1:"):
        raise OpenVietGisError("Mã ghép nối phải bắt đầu bằng 'ovg-pair-v1:'.")
    encoded = token.split(":", 1)[1].strip()
    if not encoded:
        raise OpenVietGisError("Mã ghép nối rỗng.")
    padding = "=" * (-len(encoded) % 4)
    try:
        data = json.loads(base64.urlsafe_b64decode(encoded + padding).decode("utf-8"))
    except Exception as exc:
        raise OpenVietGisError("Không thể giải mã chuỗi ghép nối R027.") from exc
    if not isinstance(data, dict) or data.get("v") != 1:
        raise OpenVietGisError("Phiên bản mã ghép nối không được hỗ trợ.")
    required = ("server_url", "organization_id", "project_id", "bearer_token")
    out: dict[str, str] = {}
    for key in required:
        val = str(data.get(key) or "").strip()
        if not val:
            raise OpenVietGisError(f"Mã ghép nối thiếu trường bắt buộc: {key}")
        out[key] = val
    for optional in ("binding_id", "layer_id", "external_dataset_ref"):
        val = str(data.get(optional) or "").strip()
        if val:
            out[optional] = val
    return out


class ConfigurationError(OpenVietGisError):
    """Raised when connection configuration or identifiers are invalid."""


class ApiError(OpenVietGisError):
    """Raised when the OpenVietGIS API returns an error or malformed response."""

    def __init__(self, message: str, *, status_code: int | None = None) -> None:
        super().__init__(message)
        self.status_code = status_code


class IntegrityError(OpenVietGisError):
    """Raised when SHA-256 verification fails or the integrity header is missing."""


def _validate_uuid(value: str, field_name: str) -> str:
    if not isinstance(value, str):
        raise ConfigurationError(f"{field_name} must be a UUID string.")
    cleaned = value.strip()
    if not _UUID_RE.fullmatch(cleaned):
        raise ConfigurationError(f"{field_name} must be a valid UUID.")
    return str(UUID(cleaned))


def _validate_sha256_hex(value: str, field_name: str) -> str:
    if not isinstance(value, str):
        raise ConfigurationError(f"{field_name} must be a 64-character hexadecimal string.")
    cleaned = value.strip()
    if not _SHA256_RE.fullmatch(cleaned):
        raise ConfigurationError(f"{field_name} must be a valid 64-character SHA-256 hex string.")
    return cleaned.lower()


def _validate_operation_id(operation_id: str) -> str:
    if not isinstance(operation_id, str):
        raise ConfigurationError("operation_id must be a non-empty string.")
    cleaned = operation_id.strip()
    if not cleaned:
        raise ConfigurationError("operation_id is required.")
    if cleaned.lower().startswith(_RESERVED_OPERATION_ID_PREFIX):
        raise ConfigurationError(
            "operation_id must not start with the reserved 'field-sync-' prefix."
        )
    if not _OPERATION_ID_RE.fullmatch(cleaned):
        raise ConfigurationError("operation_id must be 1..128 URL-safe characters.")
    return cleaned


def _validate_summary(summary: str) -> str:
    if not isinstance(summary, str):
        raise ConfigurationError("summary must be a string.")
    cleaned = summary.strip()
    if not cleaned or len(cleaned) > 500:
        raise ConfigurationError("summary must be between 1 and 500 characters.")
    if any(ch in cleaned for ch in ("\r", "\n", "\x00")):
        raise ConfigurationError("summary must not contain control characters.")
    return cleaned


def _read_validated_gpkg(gpkg_path: str | Path) -> tuple[str, bytes]:
    if not isinstance(gpkg_path, str | Path):
        raise ConfigurationError("gpkg_path must be a file path.")
    path = Path(gpkg_path).expanduser()
    if not path.exists() or not path.is_file():
        raise ConfigurationError("GeoPackage file does not exist.")
    try:
        file_size = path.stat().st_size
    except OSError as exc:
        raise ConfigurationError("Unable to inspect GeoPackage file.") from exc
    if file_size <= 0:
        raise ConfigurationError("GeoPackage file is empty.")
    if file_size > MAX_SYNC_UPLOAD_BYTES:
        raise ConfigurationError("GeoPackage file exceeds the 16 MiB sync upload limit.")
    try:
        payload = path.read_bytes()
    except OSError as exc:
        raise ConfigurationError("Unable to read GeoPackage file.") from exc
    if not payload:
        raise ConfigurationError("GeoPackage file is empty.")
    if len(payload) > MAX_SYNC_UPLOAD_BYTES:
        raise ConfigurationError("GeoPackage file exceeds the 16 MiB sync upload limit.")
    filename = sanitize_gpkg_filename(f'filename="{path.name}"', fallback_layer_id="sync")
    return filename, payload


def _validate_base_url(base_url: str, *, allow_insecure_localhost: bool) -> str:
    if not isinstance(base_url, str):
        raise ConfigurationError("base_url must be a string.")
    cleaned = base_url.strip()
    if not cleaned:
        raise ConfigurationError("base_url is required.")
    if any(ch.isspace() or ord(ch) < 32 for ch in cleaned):
        raise ConfigurationError("base_url must not contain whitespace or control characters.")

    parsed = urllib.parse.urlsplit(cleaned)
    scheme = parsed.scheme.lower()
    if not parsed.netloc or not parsed.hostname:
        raise ConfigurationError("base_url must include a valid host.")
    if parsed.username is not None or parsed.password is not None:
        raise ConfigurationError("base_url must not contain embedded credentials.")
    if parsed.query or parsed.fragment:
        raise ConfigurationError("base_url must not contain query parameters or fragments.")

    try:
        _ = parsed.port
    except ValueError as exc:
        raise ConfigurationError("base_url contains an invalid port.") from exc

    hostname = parsed.hostname.lower()
    if scheme == "https":
        pass
    elif scheme == "http":
        if not allow_insecure_localhost or hostname not in _LOCALHOST_HOSTS:
            raise ConfigurationError(
                "HTTPS is required. Insecure http:// is permitted only for "
                "localhost or 127.0.0.1 when allow_insecure_localhost=True."
            )
    else:
        raise ConfigurationError("base_url scheme must be https (or http for allowed localhost).")

    path_segments = [seg for seg in parsed.path.split("/") if seg]
    if any(seg in {".", ".."} for seg in path_segments):
        raise ConfigurationError("base_url path must not contain relative dot segments.")

    normalized_path = f"/{'/'.join(path_segments)}" if path_segments else ""
    return urllib.parse.urlunsplit((scheme, parsed.netloc, normalized_path, "", ""))


def _validate_bearer_token(bearer_token: str) -> str:
    if not isinstance(bearer_token, str):
        raise ConfigurationError("bearer_token must be a non-empty string.")
    cleaned = bearer_token.strip()
    if not cleaned:
        raise ConfigurationError("bearer_token is required.")
    if any(ch in cleaned for ch in ("\r", "\n", "\x00")):
        raise ConfigurationError("bearer_token contains forbidden control characters.")
    return cleaned


def _redact_secret(text: str, secret: str) -> str:
    if not secret:
        return text
    return text.replace(secret, "[REDACTED]")


@dataclass(frozen=True)
class OpenVietGisConfig:
    """Validated connection parameters for read-only OpenVietGIS API calls."""

    base_url: str
    organization_id: str
    project_id: str
    bearer_token: str = field(repr=False)
    allow_insecure_localhost: bool = False

    def __post_init__(self) -> None:
        if not isinstance(self.allow_insecure_localhost, bool):
            raise ConfigurationError("allow_insecure_localhost must be a boolean.")
        normalized_url = _validate_base_url(
            self.base_url,
            allow_insecure_localhost=self.allow_insecure_localhost,
        )
        normalized_org = _validate_uuid(self.organization_id, "organization_id")
        normalized_proj = _validate_uuid(self.project_id, "project_id")
        normalized_token = _validate_bearer_token(self.bearer_token)
        object.__setattr__(self, "base_url", normalized_url)
        object.__setattr__(self, "organization_id", normalized_org)
        object.__setattr__(self, "project_id", normalized_proj)
        object.__setattr__(self, "bearer_token", normalized_token)


@dataclass(frozen=True)
class QgisLayerInfo:
    """ACL-filtered project layer metadata returned by the QGIS adapter profile."""

    layer_id: str
    source_id: str
    name: str
    geometry_family: str
    dimensions: str
    validation_state: str
    source_crs_code: int | None
    stored_crs_code: int | None
    crs_explicit: bool
    layer_revision: int
    content_revision: str
    schema_revision: str
    export_permitted: bool

    @property
    def crs_display(self) -> str:
        """Display explicit EPSG code without ever inferring an undeclared CRS."""
        if self.stored_crs_code is not None:
            return f"EPSG:{self.stored_crs_code}"
        if self.source_crs_code is not None:
            return f"EPSG:{self.source_crs_code} (unverified)"
        return "Unverified / Missing"


@dataclass(frozen=True)
class QgisProfile:
    """QGIS adapter profile returned by R026 Integration Hub."""

    organization_id: str
    project_id: str
    default_write_policy: str
    scoped_access: bool
    execution_status: str
    export_formats: tuple[str, ...]
    supports_direct_db_credentials: bool
    layers: tuple[QgisLayerInfo, ...]
    sync_push_enabled: bool = False
    deletions_blocked_by_default: bool = True


@dataclass(frozen=True)
class DownloadedLayer:
    """Verified local GeoPackage artifact downloaded from OpenVietGIS."""

    path: Path
    sha256: str
    crs: str
    content_revision: str
    schema_revision: str
    feature_count: int


@dataclass(frozen=True)
class SyncPreviewResult:
    """Non-mutating diff preview returned by R026 QGIS sync-preview."""

    layer_id: str
    source_id: str
    stored_crs_code: int
    stale_revision: bool
    deletions_blocked: bool
    create_count: int
    update_count: int
    unchanged_count: int
    omitted_server_feature_count: int
    diff_sha256: str
    can_commit: bool
    operations: tuple[dict[str, Any], ...]
    delete_count: int = 0


@dataclass(frozen=True)
class SyncPushResult:
    """Audit-backed R006 changeset receipt returned by R026 QGIS sync-push."""

    changeset_id: str
    resulting_content_revision: str
    operation_count: int
    create_count: int
    update_count: int
    omitted_server_feature_count: int
    deletions_blocked: bool
    diff_sha256: str
    audit_event_id: str
    delete_count: int = 0


def build_integration_events_url(config: OpenVietGisConfig) -> str:
    """Build the authenticated SSE endpoint URL for project spatial change events."""
    if not isinstance(config, OpenVietGisConfig):
        raise ConfigurationError("config must be an OpenVietGisConfig instance.")
    return (
        f"{config.base_url}/api/v1/organizations/{config.organization_id}"
        f"/projects/{config.project_id}/integration-hub/events"
    )


def _execute_request(
    request: urllib.request.Request,
    *,
    opener: Callable[..., Any] | Any | None,
    timeout: float,
) -> Any:
    if opener is None:
        return urllib.request.urlopen(request, timeout=timeout)
    open_method = getattr(opener, "open", None)
    if callable(open_method):
        return open_method(request, timeout=timeout)
    return opener(request, timeout=timeout)


def _get_header(headers: Any, name: str) -> str | None:
    if headers is None:
        return None
    if hasattr(headers, "get"):
        value = headers.get(name)
        if value is not None:
            return str(value)
    if hasattr(headers, "items"):
        target = name.lower()
        for key, value in headers.items():
            if str(key).lower() == target and value is not None:
                return str(value)
    return None


def _extract_http_error_detail(exc: urllib.error.HTTPError, secret: str) -> str:
    detail = exc.reason if isinstance(exc.reason, str) else "Request failed"
    try:
        raw_body = exc.read(4096)
    except OSError:
        raw_body = b""
    if raw_body:
        try:
            decoded = json.loads(raw_body.decode("utf-8", errors="replace"))
            if isinstance(decoded, dict) and isinstance(decoded.get("detail"), str):
                detail = decoded["detail"]
        except (ValueError, TypeError):
            pass
    sanitized = _redact_secret(str(detail).strip(), secret)
    return sanitized[:300] or "Request failed"


def _parse_optional_int(value: Any, field_name: str) -> int | None:
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        raise ApiError(f"Malformed adapter profile: '{field_name}' must be an integer or null.")
    return value


def _parse_layer(raw: Any) -> QgisLayerInfo:
    if not isinstance(raw, dict):
        raise ApiError("Malformed adapter profile: layer entry must be an object.")
    try:
        return QgisLayerInfo(
            layer_id=str(raw["layer_id"]),
            source_id=str(raw["source_id"]),
            name=str(raw["name"]),
            geometry_family=str(raw["geometry_family"]),
            dimensions=str(raw["dimensions"]),
            validation_state=str(raw["validation_state"]),
            source_crs_code=_parse_optional_int(raw.get("source_crs_code"), "source_crs_code"),
            stored_crs_code=_parse_optional_int(raw.get("stored_crs_code"), "stored_crs_code"),
            crs_explicit=bool(raw["crs_explicit"]),
            layer_revision=int(raw["layer_revision"]),
            content_revision=str(raw["content_revision"]),
            schema_revision=str(raw["schema_revision"]),
            export_permitted=bool(raw["export_permitted"]),
        )
    except (KeyError, TypeError, ValueError) as exc:
        raise ApiError("Malformed adapter profile: missing or invalid layer fields.") from exc


def sanitize_gpkg_filename(content_disposition: str | None, *, fallback_layer_id: str) -> str:
    """Extract and strictly sanitize a .gpkg basename from Content-Disposition."""
    fallback = f"layer_{fallback_layer_id}.gpkg"
    raw_name: str | None = None
    if content_disposition:
        quoted_match = _CD_FILENAME_QUOTED_RE.search(content_disposition)
        if quoted_match:
            raw_name = quoted_match.group(1)
        else:
            star_match = _CD_FILENAME_STAR_RE.search(content_disposition)
            if star_match:
                raw_name = urllib.parse.unquote(star_match.group(1))
            else:
                unquoted_match = _CD_FILENAME_UNQUOTED_RE.search(content_disposition)
                if unquoted_match:
                    raw_name = unquoted_match.group(1).strip().strip('"')

    if not raw_name:
        return fallback

    # Strip any Unix or Windows directory components to prevent path traversal.
    basename = raw_name.replace("\\", "/").rsplit("/", 1)[-1].strip()
    if not basename:
        return fallback

    sanitized = _UNSAFE_FILENAME_CHARS_RE.sub("_", basename)
    while ".." in sanitized:
        sanitized = sanitized.replace("..", "_")
    sanitized = sanitized.lstrip("._-")

    if sanitized.lower().endswith(".gpkg"):
        stem = sanitized[:-5]
    else:
        stem = sanitized
    stem = stem.strip("._-")[:180]
    if not stem:
        return fallback

    candidate = f"{stem}.gpkg"
    if not _SAFE_GPKG_FILENAME_RE.fullmatch(candidate) or ".." in candidate:
        return fallback
    return candidate


def fetch_qgis_profile(
    config: OpenVietGisConfig,
    *,
    opener: Callable[..., Any] | Any | None = None,
    timeout: float = DEFAULT_PROFILE_TIMEOUT_SECONDS,
) -> QgisProfile:
    """Fetch the read-only R026 QGIS adapter profile and authorized layer list."""
    if not isinstance(config, OpenVietGisConfig):
        raise ConfigurationError("config must be an OpenVietGisConfig instance.")

    url = (
        f"{config.base_url}/api/v1/organizations/{config.organization_id}"
        f"/projects/{config.project_id}/integration-hub/adapters/qgis"
    )
    request = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {config.bearer_token}",
            "Accept": "application/json",
            "User-Agent": USER_AGENT,
        },
        method="GET",
    )

    try:
        with _execute_request(request, opener=opener, timeout=timeout) as response:
            raw_bytes = response.read()
    except urllib.error.HTTPError as exc:
        detail = _extract_http_error_detail(exc, config.bearer_token)
        raise ApiError(f"HTTP {exc.code}: {detail}", status_code=exc.code) from None
    except urllib.error.URLError as exc:
        reason = _redact_secret(str(exc.reason), config.bearer_token)
        raise ApiError(f"Network error contacting OpenVietGIS: {reason}") from None
    except OSError as exc:
        reason = _redact_secret(str(exc), config.bearer_token)
        raise ApiError(f"Connection error contacting OpenVietGIS: {reason}") from None

    try:
        payload = json.loads(raw_bytes.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise ApiError("Invalid JSON payload received from OpenVietGIS.") from exc

    if not isinstance(payload, dict):
        raise ApiError("Unexpected response structure from OpenVietGIS.")

    try:
        default_write_policy = str(payload["default_write_policy"])
        scoped_access = bool(payload["scoped_access"])
        execution_status = str(payload["execution_status"])
        raw_layers = payload["layers"]
    except KeyError as exc:
        raise ApiError("Malformed adapter profile: missing required top-level field.") from exc

    if not isinstance(raw_layers, list):
        raise ApiError("Malformed adapter profile: 'layers' must be a list.")

    conn_profile = payload.get("connection_profile")
    export_formats: tuple[str, ...] = ()
    supports_direct_db_credentials = False
    sync_push_enabled = False
    deletions_blocked_by_default = True
    if isinstance(conn_profile, dict):
        raw_formats = conn_profile.get("export_formats")
        if isinstance(raw_formats, list):
            export_formats = tuple(str(item) for item in raw_formats)
        supports_direct_db_credentials = bool(
            conn_profile.get("supports_direct_db_credentials", False)
        )
        sync_push_enabled = bool(conn_profile.get("sync_push_enabled", False))
        deletions_blocked_by_default = bool(
            conn_profile.get("deletions_blocked_by_default", True)
        )

    layers = tuple(_parse_layer(item) for item in raw_layers)
    return QgisProfile(
        organization_id=str(payload.get("organization_id", config.organization_id)),
        project_id=str(payload.get("project_id", config.project_id)),
        default_write_policy=default_write_policy,
        scoped_access=scoped_access,
        execution_status=execution_status,
        export_formats=export_formats,
        supports_direct_db_credentials=supports_direct_db_credentials,
        layers=layers,
        sync_push_enabled=sync_push_enabled,
        deletions_blocked_by_default=deletions_blocked_by_default,
    )


def download_layer_geopackage(
    config: OpenVietGisConfig,
    layer_id: str,
    destination_dir: str | Path,
    *,
    opener: Callable[..., Any] | Any | None = None,
    timeout: float = DEFAULT_EXPORT_TIMEOUT_SECONDS,
) -> DownloadedLayer:
    """Download a read-only GeoPackage layer snapshot with fail-closed SHA-256 verification."""
    if not isinstance(config, OpenVietGisConfig):
        raise ConfigurationError("config must be an OpenVietGisConfig instance.")
    validated_layer_id = _validate_uuid(layer_id, "layer_id")

    dest_path = Path(destination_dir).expanduser().resolve()
    dest_path.mkdir(parents=True, exist_ok=True)
    if not dest_path.is_dir():
        raise ConfigurationError("destination_dir must be a directory.")

    url = (
        f"{config.base_url}/api/v1/organizations/{config.organization_id}"
        f"/projects/{config.project_id}/integration-hub/adapters/qgis"
        f"/layers/{validated_layer_id}/export?format=geopackage"
    )
    request = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {config.bearer_token}",
            "Accept": "application/geopackage+sqlite3",
            "User-Agent": USER_AGENT,
        },
        method="GET",
    )

    try:
        with _execute_request(request, opener=opener, timeout=timeout) as response:
            headers = getattr(response, "headers", None)
            expected_sha256_raw = _get_header(headers, "X-Content-SHA256")
            if not expected_sha256_raw or not _SHA256_RE.fullmatch(expected_sha256_raw.strip()):
                raise IntegrityError(
                    "Missing or malformed X-Content-SHA256 header; refusing unverified payload."
                )
            expected_sha256 = expected_sha256_raw.strip().lower()

            content_disposition = _get_header(headers, "Content-Disposition")
            crs = (_get_header(headers, "X-OpenVietGIS-CRS") or "").strip()
            content_revision = (
                _get_header(headers, "X-OpenVietGIS-Content-Revision") or ""
            ).strip()
            schema_revision = (_get_header(headers, "X-OpenVietGIS-Schema-Revision") or "").strip()
            feature_count_raw = (_get_header(headers, "X-OpenVietGIS-Feature-Count") or "0").strip()
            try:
                feature_count = int(feature_count_raw)
            except ValueError as exc:
                raise ApiError("Invalid X-OpenVietGIS-Feature-Count response header.") from exc
            if feature_count < 0:
                raise ApiError("Negative X-OpenVietGIS-Feature-Count response header.")

            payload = response.read()
    except (IntegrityError, ApiError, ConfigurationError):
        raise
    except urllib.error.HTTPError as exc:
        detail = _extract_http_error_detail(exc, config.bearer_token)
        raise ApiError(f"HTTP {exc.code}: {detail}", status_code=exc.code) from None
    except urllib.error.URLError as exc:
        reason = _redact_secret(str(exc.reason), config.bearer_token)
        raise ApiError(f"Network error downloading layer: {reason}") from None
    except OSError as exc:
        reason = _redact_secret(str(exc), config.bearer_token)
        raise ApiError(f"Connection error downloading layer: {reason}") from None

    actual_sha256 = hashlib.sha256(payload).hexdigest()
    if actual_sha256 != expected_sha256:
        raise IntegrityError(
            "SHA-256 verification failed: downloaded GeoPackage digest does not match "
            "X-Content-SHA256."
        )

    filename = sanitize_gpkg_filename(content_disposition, fallback_layer_id=validated_layer_id)
    target_path = (dest_path / filename).resolve()
    if target_path.parent != dest_path:
        raise IntegrityError("Refused target path outside destination directory.")

    tmp_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb",
            dir=dest_path,
            prefix=".ovg_",
            suffix=".tmp",
            delete=False,
        ) as tmp_file:
            tmp_path = Path(tmp_file.name)
            tmp_file.write(payload)
            tmp_file.flush()
            os.fsync(tmp_file.fileno())
        os.replace(tmp_path, target_path)
        tmp_path = None
    finally:
        if tmp_path is not None and tmp_path.exists():
            try:
                tmp_path.unlink()
            except OSError:
                pass

    return DownloadedLayer(
        path=target_path,
        sha256=actual_sha256,
        crs=crs,
        content_revision=content_revision,
        schema_revision=schema_revision,
        feature_count=feature_count,
    )


def _sanitize_header_param(value: str) -> str:
    return value.replace("\r", "").replace("\n", "").replace('"', "_")


def _build_multipart_body(
    fields: Mapping[str, str] | Sequence[tuple[str, str]],
    file_field: str,
    filename: str,
    payload: bytes,
    content_type: str,
) -> tuple[bytes, str]:
    """Build an RFC 7578 multipart/form-data body and Content-Type header."""
    boundary = f"----OpenVietGISBoundary{secrets.token_hex(16)}"
    items: Sequence[tuple[str, str]] = (
        list(fields.items()) if isinstance(fields, Mapping) else list(fields)
    )
    parts: list[bytes] = []
    for name, value in items:
        safe_name = _sanitize_header_param(str(name))
        parts.append(
            f'--{boundary}\r\nContent-Disposition: form-data; name="{safe_name}"\r\n\r\n'.encode()
        )
        parts.append(str(value).encode())
        parts.append(b"\r\n")

    safe_file_field = _sanitize_header_param(file_field)
    safe_filename = _sanitize_header_param(filename)
    safe_content_type = _sanitize_header_param(content_type) or "application/octet-stream"
    parts.append(
        (
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="{safe_file_field}"; '
            f'filename="{safe_filename}"\r\n'
            f"Content-Type: {safe_content_type}\r\n\r\n"
        ).encode()
    )
    parts.append(payload)
    parts.append(f"\r\n--{boundary}--\r\n".encode())
    return b"".join(parts), f"multipart/form-data; boundary={boundary}"


def _require_bool_field(raw: dict[str, Any], key: str, *, context: str) -> bool:
    if key not in raw or not isinstance(raw[key], bool):
        raise ApiError(f"Malformed {context}: '{key}' must be a boolean.")
    return raw[key]


def _require_int_field(
    raw: dict[str, Any],
    key: str,
    *,
    context: str,
    minimum: int = 0,
) -> int:
    value = raw.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ApiError(f"Malformed {context}: '{key}' must be an integer >= {minimum}.")
    return value


def _require_str_field(raw: dict[str, Any], key: str, *, context: str) -> str:
    value = raw.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ApiError(f"Malformed {context}: '{key}' must be a non-empty string.")
    return value.strip()


def _require_sha256_field(raw: dict[str, Any], key: str, *, context: str) -> str:
    value = _require_str_field(raw, key, context=context)
    if not _SHA256_RE.fullmatch(value):
        raise ApiError(f"Malformed {context}: '{key}' must be a 64-character SHA-256 hex string.")
    return value.lower()


def _post_multipart_json(
    config: OpenVietGisConfig,
    *,
    url: str,
    body: bytes,
    content_type_header: str,
    opener: Callable[..., Any] | Any | None,
    timeout: float,
    action_label: str,
) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        data=body,
        headers={
            "Authorization": f"Bearer {config.bearer_token}",
            "Accept": "application/json",
            "Content-Type": content_type_header,
            "User-Agent": USER_AGENT,
        },
        method="POST",
    )
    try:
        with _execute_request(request, opener=opener, timeout=timeout) as response:
            raw_bytes = response.read()
    except urllib.error.HTTPError as exc:
        detail = _extract_http_error_detail(exc, config.bearer_token)
        raise ApiError(f"HTTP {exc.code}: {detail}", status_code=exc.code) from None
    except urllib.error.URLError as exc:
        reason = _redact_secret(str(exc.reason), config.bearer_token)
        raise ApiError(f"Network error during {action_label}: {reason}") from None
    except OSError as exc:
        reason = _redact_secret(str(exc), config.bearer_token)
        raise ApiError(f"Connection error during {action_label}: {reason}") from None

    try:
        parsed = json.loads(raw_bytes.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise ApiError(f"Invalid JSON payload received from {action_label}.") from exc
    if not isinstance(parsed, dict):
        raise ApiError(f"Unexpected response structure from {action_label}.")
    return parsed


def preview_layer_geopackage_sync(
    config: OpenVietGisConfig,
    *,
    layer_id: str,
    gpkg_path: str | Path,
    base_content_revision: str,
    base_schema_revision: str,
    allow_deletes: bool = False,
    opener: Callable[..., Any] | Any | None = None,
    timeout: float = DEFAULT_SYNC_TIMEOUT_SECONDS,
) -> SyncPreviewResult:
    """Preview GeoPackage edits against a pinned server layer revision."""
    if not isinstance(config, OpenVietGisConfig):
        raise ConfigurationError("config must be an OpenVietGisConfig instance.")
    validated_layer_id = _validate_uuid(layer_id, "layer_id")
    validated_content_rev = _validate_sha256_hex(base_content_revision, "base_content_revision")
    validated_schema_rev = _validate_sha256_hex(base_schema_revision, "base_schema_revision")
    filename, gpkg_bytes = _read_validated_gpkg(gpkg_path)

    fields = {
        "base_content_revision": validated_content_rev,
        "base_schema_revision": validated_schema_rev,
    }
    if allow_deletes:
        fields["allow_deletes"] = "true"

    body, content_type_header = _build_multipart_body(
        fields=fields,
        file_field="file",
        filename=filename,
        payload=gpkg_bytes,
        content_type="application/geopackage+sqlite3",
    )
    url = (
        f"{config.base_url}/api/v1/organizations/{config.organization_id}"
        f"/projects/{config.project_id}/integration-hub/adapters/qgis"
        f"/layers/{validated_layer_id}/sync-preview"
    )
    payload = _post_multipart_json(
        config,
        url=url,
        body=body,
        content_type_header=content_type_header,
        opener=opener,
        timeout=timeout,
        action_label="sync preview",
    )

    ctx = "sync preview response"
    raw_ops = payload.get("operations")
    if not isinstance(raw_ops, list) or not all(isinstance(item, dict) for item in raw_ops):
        raise ApiError(f"Malformed {ctx}: 'operations' must be a list of objects.")

    return SyncPreviewResult(
        layer_id=_require_str_field(payload, "layer_id", context=ctx),
        source_id=_require_str_field(payload, "source_id", context=ctx),
        stored_crs_code=_require_int_field(payload, "stored_crs_code", context=ctx, minimum=1),
        stale_revision=_require_bool_field(payload, "stale_revision", context=ctx),
        deletions_blocked=_require_bool_field(payload, "deletions_blocked", context=ctx),
        create_count=_require_int_field(payload, "create_count", context=ctx, minimum=0),
        update_count=_require_int_field(payload, "update_count", context=ctx, minimum=0),
        delete_count=int(payload.get("delete_count", 0)),
        unchanged_count=_require_int_field(payload, "unchanged_count", context=ctx, minimum=0),
        omitted_server_feature_count=_require_int_field(
            payload, "omitted_server_feature_count", context=ctx, minimum=0
        ),
        diff_sha256=_require_sha256_field(payload, "diff_sha256", context=ctx),
        can_commit=_require_bool_field(payload, "can_commit", context=ctx),
        operations=tuple(dict(item) for item in raw_ops),
    )


def push_layer_geopackage_sync(
    config: OpenVietGisConfig,
    *,
    layer_id: str,
    gpkg_path: str | Path,
    base_content_revision: str,
    base_schema_revision: str,
    expected_diff_sha256: str,
    operation_id: str,
    summary: str = "QGIS GeoPackage sync push",
    allow_deletes: bool = False,
    opener: Callable[..., Any] | Any | None = None,
    timeout: float = DEFAULT_SYNC_TIMEOUT_SECONDS,
) -> SyncPushResult:
    """Commit a previewed GeoPackage diff through R006 versioned changesets."""
    if not isinstance(config, OpenVietGisConfig):
        raise ConfigurationError("config must be an OpenVietGisConfig instance.")
    validated_layer_id = _validate_uuid(layer_id, "layer_id")
    validated_content_rev = _validate_sha256_hex(base_content_revision, "base_content_revision")
    validated_schema_rev = _validate_sha256_hex(base_schema_revision, "base_schema_revision")
    validated_diff_sha = _validate_sha256_hex(expected_diff_sha256, "expected_diff_sha256")
    validated_operation_id = _validate_operation_id(operation_id)
    validated_summary = _validate_summary(summary)
    filename, gpkg_bytes = _read_validated_gpkg(gpkg_path)

    fields = {
        "base_content_revision": validated_content_rev,
        "base_schema_revision": validated_schema_rev,
        "operation_id": validated_operation_id,
        "expected_diff_sha256": validated_diff_sha,
        "summary": validated_summary,
    }
    if allow_deletes:
        fields["allow_deletes"] = "true"

    body, content_type_header = _build_multipart_body(
        fields=fields,
        file_field="file",
        filename=filename,
        payload=gpkg_bytes,
        content_type="application/geopackage+sqlite3",
    )
    url = (
        f"{config.base_url}/api/v1/organizations/{config.organization_id}"
        f"/projects/{config.project_id}/integration-hub/adapters/qgis"
        f"/layers/{validated_layer_id}/sync-push"
    )
    payload = _post_multipart_json(
        config,
        url=url,
        body=body,
        content_type_header=content_type_header,
        opener=opener,
        timeout=timeout,
        action_label="sync push",
    )

    ctx = "sync push response"
    return SyncPushResult(
        changeset_id=_require_str_field(payload, "changeset_id", context=ctx),
        resulting_content_revision=_require_sha256_field(
            payload, "resulting_content_revision", context=ctx
        ),
        operation_count=_require_int_field(payload, "operation_count", context=ctx, minimum=0),
        create_count=_require_int_field(payload, "create_count", context=ctx, minimum=0),
        update_count=_require_int_field(payload, "update_count", context=ctx, minimum=0),
        delete_count=int(payload.get("delete_count", 0)),
        omitted_server_feature_count=_require_int_field(
            payload, "omitted_server_feature_count", context=ctx, minimum=0
        ),
        deletions_blocked=_require_bool_field(payload, "deletions_blocked", context=ctx),
        diff_sha256=_require_sha256_field(payload, "diff_sha256", context=ctx),
        audit_event_id=_require_str_field(payload, "audit_event_id", context=ctx),
    )
