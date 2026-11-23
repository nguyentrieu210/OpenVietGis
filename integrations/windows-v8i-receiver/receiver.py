"""Windows-side OpenVietGIS inbound receiver, R026 HTTPS read-only export contract.

Downloads an immutable DXF/GPKG pair from exactly the same source revision.
No DGN writes, remote mutations, background polling or credential persistence.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import sqlite3
import shutil
import ssl
import tempfile
from contextlib import closing
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import quote, urlsplit
from urllib.request import HTTPRedirectHandler, HTTPSHandler, Request, build_opener
from uuid import UUID

MAX_DOWNLOAD = 32 * 1024 * 1024
_SHA = re.compile(r"^[a-fA-F0-9]{64}$")
_REVISION = re.compile(r"^[a-zA-Z0-9_.:-]{1,128}$")
_CRS = frozenset({"EPSG:5899", "EPSG:3857"})


class ReceiverError(ValueError):
    """A bounded or permission-safe inbound download could not be verified."""


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[override]
        return None  # Never forward the bearer token to a different origin.


@dataclass(frozen=True)
class Connection:
    server: str
    organization_id: str
    project_id: str
    bearer_token: str

    def validated(self) -> "Connection":
        if self.server != self.server.strip() or any(c.isspace() for c in self.server):
            raise ReceiverError("Invalid server origin.")
        try:
            parsed = urlsplit(self.server)
            port = parsed.port
        except ValueError as exc:
            raise ReceiverError("Invalid server origin.") from exc
        if (
            parsed.scheme != "https" or not parsed.hostname or parsed.username
            or parsed.password or parsed.query or parsed.fragment
            or parsed.path not in {"", "/"} or port not in {None, 443}
        ):
            raise ReceiverError("Server must be a plain HTTPS origin (no URL path, userinfo or redirect).")
        if not self.bearer_token or len(self.bearer_token) > 4096 or any(
            ord(c) < 33 or ord(c) > 126 for c in self.bearer_token
        ):
            raise ReceiverError("A valid temporary bearer credential is required.")
        try:
            UUID(self.organization_id)
            UUID(self.project_id)
        except (ValueError, AttributeError) as exc:
            raise ReceiverError("Organization and project must be UUIDs.") from exc
        return self


@dataclass(frozen=True)
class _Downloaded:
    data: bytes
    sha256: str
    content_revision: str
    schema_revision: str
    crs: str
    feature_count: int
    attribute_loss: bool


def _header(headers: Any, name: str) -> str:
    value = headers.get(name, "")
    return str(value).strip()


def _download(connection: Connection, layer_id: str, kind: str, opener: Any) -> _Downloaded:
    if kind not in {"dxf", "geopackage"}:
        raise ReceiverError("Unsupported receiver payload.")
    adapter = "autocad" if kind == "dxf" else "qgis"
    org = quote(str(UUID(connection.organization_id)), safe="")
    project = quote(str(UUID(connection.project_id)), safe="")
    lid = quote(str(UUID(layer_id)), safe="")
    url = (connection.server.rstrip("/") + "/api/v1/organizations/" + org
           + "/projects/" + project + "/integration-hub/adapters/" + adapter
           + "/layers/" + lid + "/export?format=" + kind)
    req = Request(url, headers={
        "Authorization": "Bearer " + connection.bearer_token,
        "Accept": "application/dxf" if kind == "dxf" else "application/geopackage+sqlite3",
        "Cache-Control": "no-store",
    }, method="GET")
    try:
        with opener.open(req, timeout=25) as response:
            if response.getcode() != 200 or response.geturl() != url:
                raise ReceiverError("Untrusted HTTP response or redirect.")
            headers = response.headers
            if _header(headers, "Content-Type").split(";", 1)[0].strip().lower() != (
                "application/dxf" if kind == "dxf" else "application/geopackage+sqlite3"
            ):
                raise ReceiverError("Unexpected export content type.")
            if _header(headers, "Content-Length").isdigit() and int(_header(headers, "Content-Length")) > MAX_DOWNLOAD:
                raise ReceiverError("Export is too large.")
            data = response.read(MAX_DOWNLOAD + 1)
    except ReceiverError:
        raise
    except Exception as exc:
        # Do not leak HTTP diagnostic bodies/URLs/tokens to GUI or audit records.
        raise ReceiverError("Cannot fetch authorized export (check connection, permission and CRS).") from exc
    if not 0 < len(data) <= MAX_DOWNLOAD:
        raise ReceiverError("Empty or oversized export.")
    expected_sha = _header(headers, "X-Content-SHA256").lower()
    if not _SHA.fullmatch(expected_sha) or hashlib.sha256(data).hexdigest() != expected_sha:
        raise ReceiverError("Export SHA-256 verification failed.")
    content_rev = _header(headers, "X-OpenVietGIS-Content-Revision")
    schema_rev = _header(headers, "X-OpenVietGIS-Schema-Revision")
    crs = _header(headers, "X-OpenVietGIS-CRS")
    count_raw = _header(headers, "X-OpenVietGIS-Feature-Count")
    if not _REVISION.fullmatch(content_rev) or not _REVISION.fullmatch(schema_rev):
        raise ReceiverError("Missing or unsafe version pins.")
    if crs not in _CRS:
        raise ReceiverError("Only verified projected metre EPSG:5899/3857 is accepted for V8i DXF.")
    if not count_raw.isascii() or not count_raw.isdecimal() or not 1 <= int(count_raw) <= 10_000:
        raise ReceiverError("Invalid feature count for CAD export.")
    loss = _header(headers, "X-OpenVietGIS-Attribute-Loss").lower() == "true"
    if kind == "dxf":
        if not loss or b"SECTION" not in data[:512] or b"EOF" not in data[-512:]:
            raise ReceiverError("Invalid or unlabelled DXF geometry export.")
    elif not data.startswith(b"SQLite format 3\x00"):
        raise ReceiverError("Invalid GeoPackage file signature.")
    return _Downloaded(data, expected_sha, content_rev, schema_rev, crs, int(count_raw), loss)


def _verify_geopackage(path: Path, crs: str, feature_count: int) -> None:
    """Inspect the actual SQLite GeoPackage instead of trusting the magic bytes."""
    try:
        uri = path.resolve().as_uri() + "?mode=ro"
        with closing(sqlite3.connect(uri, uri=True)) as db:
            db.execute("PRAGMA query_only = ON")
            if db.execute("PRAGMA quick_check").fetchone() != ("ok",):
                raise ReceiverError("GeoPackage integrity check failed.")
            rows = db.execute(
                "SELECT table_name,srs_id FROM gpkg_contents WHERE data_type='features'"
            ).fetchall()
            if len(rows) != 1 or rows[0][1] != int(crs.removeprefix("EPSG:")):
                raise ReceiverError("GeoPackage layer count/CRS does not match DXF.")
            name = rows[0][0]
            if not isinstance(name, str) or not name or len(name) > 128:
                raise ReceiverError("GeoPackage feature table is invalid.")
            safe_name = '"' + name.replace('"', '""') + '"'
            if db.execute(f"SELECT COUNT(*) FROM {safe_name}").fetchone()[0] != feature_count:
                raise ReceiverError("GeoPackage feature count differs from export metadata.")
            if db.execute(
                "SELECT COUNT(*) FROM gpkg_geometry_columns WHERE table_name=? AND srs_id=?",
                (name, int(crs.removeprefix("EPSG:"))),
            ).fetchone()[0] != 1:
                raise ReceiverError("GeoPackage geometry registry is inconsistent.")
    except ReceiverError:
        raise
    except (sqlite3.DatabaseError, OSError, ValueError) as exc:
        raise ReceiverError("GeoPackage structure cannot be verified.") from exc


def receive_layer(
    connection: Connection, layer_id: str, inbox: str | Path, *, opener: Any = None
) -> Path:
    """Download both formats, verify a common revision, then publish one immutable folder.

    The DXF is geometry-only; the GPKG contains attribute data. Neither is
    imported to MicroStation automatically: attaching/reference is user-controlled.
    """
    connection.validated()
    try:
        layer_uuid = str(UUID(layer_id))
    except (ValueError, AttributeError) as exc:
        raise ReceiverError("Layer must be a valid UUID.") from exc
    if opener is None:
        opener = build_opener(_NoRedirect(), HTTPSHandler(context=ssl.create_default_context()))
    dxf = _download(connection, layer_uuid, "dxf", opener)
    gpkg = _download(connection, layer_uuid, "geopackage", opener)
    if (dxf.content_revision, dxf.schema_revision, dxf.crs, dxf.feature_count) != (
        gpkg.content_revision, gpkg.schema_revision, gpkg.crs, gpkg.feature_count
    ):
        raise ReceiverError("Source revision/CRS/count changed between exports. Retry without merging files.")

    destination = Path(inbox).expanduser().resolve()
    destination.mkdir(parents=True, exist_ok=True)
    folder_name = f"{layer_uuid[:8]}-{dxf.content_revision[:12]}-{dxf.sha256[:12]}"
    final = destination / folder_name
    if final.exists():
        if final.is_dir() and (final / "geometry.dxf").is_file() and (final / "attributes.gpkg").is_file():
            if (hashlib.sha256((final / "geometry.dxf").read_bytes()).hexdigest() == dxf.sha256
                    and hashlib.sha256((final / "attributes.gpkg").read_bytes()).hexdigest() == gpkg.sha256):
                return final
        raise ReceiverError("Existing local bundle differs; never overwrite received files.")
    staged = Path(tempfile.mkdtemp(prefix=".ovg-pending-", dir=destination))
    manifest = {
        "schema_version": "ovg.windows.inbound.v1",
        "server_origin": connection.server.rstrip("/"),
        "organization_id": str(UUID(connection.organization_id)),
        "project_id": str(UUID(connection.project_id)),
        "layer_id": layer_uuid,
        "content_revision": dxf.content_revision,
        "schema_revision": dxf.schema_revision,
        "crs": dxf.crs,
        "feature_count": dxf.feature_count,
        "geometry_dxf_sha256": dxf.sha256,
        "attributes_geopackage_sha256": gpkg.sha256,
        "dxf_attribute_loss": True,
        "requires_cad_unit_verification": True,
        "cad_write_back_enabled": False,
        "received_at_utc": datetime.now(timezone.utc).isoformat(),
    }
    try:
        (staged / "geometry.dxf").write_bytes(dxf.data)
        (staged / "attributes.gpkg").write_bytes(gpkg.data)
        _verify_geopackage(staged / "attributes.gpkg", dxf.crs, dxf.feature_count)
        (staged / "receipt.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        # Atomic publication within one volume: a failed download never exposes a partial bundle.
        if final.exists():
            raise ReceiverError("Received folder already exists.")
        staged.rename(final)
    finally:
        if staged.exists():
            shutil.rmtree(staged)
    return final


def default_inbox() -> Path:
    appdata = os.environ.get("LOCALAPPDATA", str(Path.home()))
    return Path(appdata) / "OpenVietGIS" / "Incoming"