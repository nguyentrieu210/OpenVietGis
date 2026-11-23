"""Unit tests for the pure-stdlib QGIS Connector read-only HTTP client."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import sys
import urllib.error
import urllib.request
from email.message import Message
from pathlib import Path
from typing import Any
from uuid import uuid4

import pytest


def _load_qgis_client_module() -> Any:
    repo_root = Path(__file__).resolve().parents[5]
    client_path = (
        repo_root / "integrations" / "qgis_plugin" / "openvietgis_connector" / "client.py"
    )
    assert client_path.is_file(), f"QGIS plugin client not found at {client_path}"
    module_name = "_ovg_qgis_connector_client_under_test"
    spec = importlib.util.spec_from_file_location(module_name, client_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


qgis_client = _load_qgis_client_module()


class _FakeHttpResponse:
    def __init__(self, body: bytes, headers: dict[str, str] | None = None) -> None:
        self._body = body
        self.headers = headers or {}

    def read(self) -> bytes:
        return self._body

    def __enter__(self) -> _FakeHttpResponse:
        return self

    def __exit__(self, *_exc: object) -> None:
        return None


def _valid_config(**overrides: Any) -> Any:
    params: dict[str, Any] = {
        "base_url": "https://gis.example.vn",
        "organization_id": str(uuid4()),
        "project_id": str(uuid4()),
        "bearer_token": "ovg_secret_token_987654321",
        "allow_insecure_localhost": False,
    }
    params.update(overrides)
    return qgis_client.OpenVietGisConfig(**params)


def test_config_rejects_plain_http_for_non_localhost() -> None:
    with pytest.raises(qgis_client.ConfigurationError, match="HTTPS is required"):
        _valid_config(base_url="http://gis.example.vn", allow_insecure_localhost=False)

    with pytest.raises(qgis_client.ConfigurationError, match="HTTPS is required"):
        _valid_config(base_url="http://gis.example.vn", allow_insecure_localhost=True)


@pytest.mark.parametrize("local_url", ["http://127.0.0.1:8000", "http://localhost:8000"])
def test_config_allows_insecure_localhost_only_when_explicitly_enabled(
    local_url: str,
) -> None:
    with pytest.raises(qgis_client.ConfigurationError, match="HTTPS is required"):
        _valid_config(base_url=local_url, allow_insecure_localhost=False)

    cfg = _valid_config(base_url=local_url, allow_insecure_localhost=True)
    assert cfg.base_url == local_url


@pytest.mark.parametrize(
    "bad_url",
    [
        "https://user:pass@gis.example.vn",
        "https://user@gis.example.vn",
        "https://gis.example.vn?foo=bar",
        "https://gis.example.vn#frag",
        "ftp://gis.example.vn",
        "https://gis.example.vn/../etc",
    ],
)
def test_config_rejects_embedded_credentials_and_unsafe_urls(bad_url: str) -> None:
    with pytest.raises(qgis_client.ConfigurationError):
        _valid_config(base_url=bad_url)


@pytest.mark.parametrize("field_name", ["organization_id", "project_id"])
def test_config_rejects_invalid_uuid(field_name: str) -> None:
    with pytest.raises(qgis_client.ConfigurationError, match=field_name):
        _valid_config(**{field_name: "not-a-valid-uuid"})


def test_config_redacts_bearer_token_in_repr() -> None:
    secret = "ovg_super_secret_token_xyz"
    cfg = _valid_config(bearer_token=secret)
    rendered = repr(cfg)
    assert secret not in rendered
    assert "bearer_token" not in rendered


def test_fetch_qgis_profile_parses_layers_and_sends_bearer_get() -> None:
    cfg = _valid_config(base_url="https://gis.example.vn/")
    layer_id = str(uuid4())
    source_id = str(uuid4())
    captured: list[tuple[urllib.request.Request, float]] = []

    profile_payload = {
        "schema_version": 1,
        "organization_id": cfg.organization_id,
        "project_id": cfg.project_id,
        "scoped_access": True,
        "default_write_policy": "deny",
        "execution_status": "read_only_profile_ready",
        "connection_profile": {
            "supports_direct_db_credentials": False,
            "sync_push_enabled": True,
            "deletions_blocked_by_default": True,
            "export_formats": ["geopackage", "geojson", "postgis_sql"],
        },
        "layers": [
            {
                "layer_id": layer_id,
                "source_id": source_id,
                "name": "Quang Ninh VN-2000",
                "geometry_family": "Point",
                "dimensions": "XY",
                "validation_state": "ready",
                "source_crs_code": 5899,
                "stored_crs_code": 5899,
                "crs_explicit": True,
                "layer_revision": 2,
                "content_revision": "c-rev-1",
                "schema_revision": "s-rev-1",
                "export_permitted": True,
            }
        ],
    }

    def fake_opener(request: urllib.request.Request, *, timeout: float) -> _FakeHttpResponse:
        captured.append((request, timeout))
        return _FakeHttpResponse(json.dumps(profile_payload).encode("utf-8"))

    profile = qgis_client.fetch_qgis_profile(cfg, opener=fake_opener, timeout=12.5)

    assert len(captured) == 1
    req, timeout = captured[0]
    assert timeout == 12.5
    assert req.get_method() == "GET"
    assert req.full_url == (
        f"https://gis.example.vn/api/v1/organizations/{cfg.organization_id}"
        f"/projects/{cfg.project_id}/integration-hub/adapters/qgis"
    )
    assert req.get_header("Authorization") == f"Bearer {cfg.bearer_token}"
    assert req.get_header("Accept") == "application/json"

    assert profile.default_write_policy == "deny"
    assert profile.scoped_access is True
    assert profile.execution_status == "read_only_profile_ready"
    assert profile.export_formats == ("geopackage", "geojson", "postgis_sql")
    assert profile.supports_direct_db_credentials is False
    assert profile.sync_push_enabled is True
    assert profile.deletions_blocked_by_default is True
    assert len(profile.layers) == 1
    layer = profile.layers[0]
    assert layer.layer_id == layer_id
    assert layer.stored_crs_code == 5899
    assert layer.crs_explicit is True
    assert layer.crs_display == "EPSG:5899"
    assert layer.export_permitted is True


def test_fetch_qgis_profile_redacts_token_on_http_error() -> None:
    secret = "ovg_leaked_in_error_body_token"
    cfg = _valid_config(bearer_token=secret)

    def error_opener(request: urllib.request.Request, *, timeout: float) -> Any:
        del timeout
        err_body = json.dumps({"detail": f"denied token {secret}"}).encode("utf-8")
        raise urllib.error.HTTPError(
            request.full_url,
            401,
            "Unauthorized",
            Message(),
            io.BytesIO(err_body),
        )

    with pytest.raises(qgis_client.ApiError) as exc_info:
        qgis_client.fetch_qgis_profile(cfg, opener=error_opener)
    assert exc_info.value.status_code == 401
    assert secret not in str(exc_info.value)
    assert "[REDACTED]" in str(exc_info.value)


def test_download_layer_geopackage_verifies_sha256_and_writes_atomically(
    tmp_path: Path,
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    payload = b"SQLite format 3\x00fake-geopackage-bytes"
    digest = hashlib.sha256(payload).hexdigest()
    captured: list[urllib.request.Request] = []

    def fake_opener(request: urllib.request.Request, *, timeout: float) -> _FakeHttpResponse:
        del timeout
        captured.append(request)
        return _FakeHttpResponse(
            payload,
            headers={
                "Content-Disposition": 'attachment; filename="vn2000-points-qgis.gpkg"',
                "X-Content-SHA256": digest,
                "X-OpenVietGIS-CRS": "EPSG:5899",
                "X-OpenVietGIS-Content-Revision": "crev-9",
                "X-OpenVietGIS-Schema-Revision": "srev-4",
                "X-OpenVietGIS-Feature-Count": "4",
            },
        )

    result = qgis_client.download_layer_geopackage(cfg, layer_id, tmp_path, opener=fake_opener)

    assert len(captured) == 1
    assert captured[0].get_method() == "GET"
    assert captured[0].full_url == (
        f"https://gis.example.vn/api/v1/organizations/{cfg.organization_id}"
        f"/projects/{cfg.project_id}/integration-hub/adapters/qgis"
        f"/layers/{layer_id}/export?format=geopackage"
    )
    assert result.path == (tmp_path / "vn2000-points-qgis.gpkg").resolve()
    assert result.path.parent == tmp_path.resolve()
    assert result.path.read_bytes() == payload
    assert result.sha256 == digest
    assert result.crs == "EPSG:5899"
    assert result.content_revision == "crev-9"
    assert result.schema_revision == "srev-4"
    assert result.feature_count == 4
    assert list(tmp_path.iterdir()) == [result.path]


@pytest.mark.parametrize(
    "bad_headers",
    [
        {"Content-Disposition": 'attachment; filename="layer.gpkg"'},
        {
            "Content-Disposition": 'attachment; filename="layer.gpkg"',
            "X-Content-SHA256": "not-a-valid-sha256",
        },
        {
            "Content-Disposition": 'attachment; filename="layer.gpkg"',
            "X-Content-SHA256": "0" * 64,
        },
    ],
)
def test_download_layer_geopackage_rejects_missing_or_mismatched_sha256_without_writing(
    tmp_path: Path,
    bad_headers: dict[str, str],
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    payload = b"corrupted-or-unverified-payload"

    def fake_opener(request: urllib.request.Request, *, timeout: float) -> _FakeHttpResponse:
        del request, timeout
        return _FakeHttpResponse(payload, headers=bad_headers)

    with pytest.raises(qgis_client.IntegrityError):
        qgis_client.download_layer_geopackage(cfg, layer_id, tmp_path, opener=fake_opener)

    assert list(tmp_path.iterdir()) == []


@pytest.mark.parametrize(
    ("content_disposition", "expected_filename"),
    [
        ('attachment; filename="../../evil.gpkg"', "evil.gpkg"),
        ('attachment; filename="..\\..\\evil.gpkg"', "evil.gpkg"),
        ('attachment; filename="/etc/passwd/../../escape.gpkg"', "escape.gpkg"),
    ],
)
def test_download_layer_geopackage_sanitizes_path_traversal_content_disposition(
    tmp_path: Path,
    content_disposition: str,
    expected_filename: str,
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    payload = b"SQLite format 3\x00safe-payload"
    digest = hashlib.sha256(payload).hexdigest()
    target_dir = tmp_path / "nested" / "downloads"

    def fake_opener(request: urllib.request.Request, *, timeout: float) -> _FakeHttpResponse:
        del request, timeout
        return _FakeHttpResponse(
            payload,
            headers={
                "Content-Disposition": content_disposition,
                "X-Content-SHA256": digest,
                "X-OpenVietGIS-CRS": "EPSG:5899",
                "X-OpenVietGIS-Feature-Count": "1",
            },
        )

    result = qgis_client.download_layer_geopackage(
        cfg, layer_id, target_dir, opener=fake_opener
    )

    assert result.path.parent == target_dir.resolve()
    assert result.path.name == expected_filename
    assert result.path.read_bytes() == payload
    assert not (tmp_path / expected_filename).exists()


def _write_temp_gpkg(
    tmp_path: Path,
    name: str = "edited-layer.gpkg",
    size: int | None = None,
) -> Path:
    path = tmp_path / name
    if size is not None:
        path.write_bytes(b"\x00" * size)
    else:
        path.write_bytes(b"SQLite format 3\x00mock-gpkg-sync-payload")
    return path


def test_preview_layer_geopackage_sync_formats_multipart_and_parses_response(
    tmp_path: Path,
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    source_id = str(uuid4())
    gpkg_path = _write_temp_gpkg(tmp_path, "vn2000-edited.gpkg")
    content_rev = "A" * 64
    schema_rev = "B" * 64
    diff_sha = "c" * 64
    captured: list[tuple[urllib.request.Request, float]] = []

    preview_payload = {
        "schema_version": 1,
        "adapter_key": "qgis",
        "layer_id": layer_id,
        "source_id": source_id,
        "stored_crs_code": 5899,
        "stale_revision": False,
        "deletions_blocked": True,
        "create_count": 1,
        "update_count": 2,
        "unchanged_count": 3,
        "omitted_server_feature_count": 1,
        "diff_sha256": diff_sha,
        "can_commit": True,
        "operations": [
            {
                "op_type": "update",
                "feature_id": "HVS-QN1",
                "changed_fields": ["fld_name"],
                "geometry_changed": False,
            }
        ],
    }

    def fake_opener(request: urllib.request.Request, *, timeout: float) -> _FakeHttpResponse:
        captured.append((request, timeout))
        return _FakeHttpResponse(json.dumps(preview_payload).encode("utf-8"))

    result = qgis_client.preview_layer_geopackage_sync(
        cfg,
        layer_id=layer_id,
        gpkg_path=gpkg_path,
        base_content_revision=content_rev,
        base_schema_revision=schema_rev,
        opener=fake_opener,
        timeout=25.0,
    )

    assert len(captured) == 1
    req, timeout = captured[0]
    assert timeout == 25.0
    assert req.get_method() == "POST"
    assert req.full_url == (
        f"https://gis.example.vn/api/v1/organizations/{cfg.organization_id}"
        f"/projects/{cfg.project_id}/integration-hub/adapters/qgis"
        f"/layers/{layer_id}/sync-preview"
    )
    assert req.get_header("Authorization") == f"Bearer {cfg.bearer_token}"
    content_type = req.get_header("Content-type") or req.get_header("Content-Type")
    assert content_type is not None
    assert content_type.startswith("multipart/form-data; boundary=----OpenVietGISBoundary")
    boundary = content_type.split("boundary=", 1)[1].encode("ascii")

    body = req.data
    assert isinstance(body, bytes)
    assert body.startswith(b"--" + boundary + b"\r\n")
    assert body.endswith(b"\r\n--" + boundary + b"--\r\n")
    assert (
        b'Content-Disposition: form-data; name="base_content_revision"\r\n\r\n'
        + content_rev.lower().encode("ascii")
        in body
    )
    assert (
        b'Content-Disposition: form-data; name="base_schema_revision"\r\n\r\n'
        + schema_rev.lower().encode("ascii")
        in body
    )
    assert (
        b'Content-Disposition: form-data; name="file"; filename="vn2000-edited.gpkg"\r\n'
        b"Content-Type: application/geopackage+sqlite3\r\n\r\n"
        + gpkg_path.read_bytes()
        in body
    )

    assert result.layer_id == layer_id
    assert result.source_id == source_id
    assert result.stored_crs_code == 5899
    assert result.stale_revision is False
    assert result.deletions_blocked is True
    assert result.create_count == 1
    assert result.update_count == 2
    assert result.unchanged_count == 3
    assert result.omitted_server_feature_count == 1
    assert result.diff_sha256 == diff_sha
    assert result.can_commit is True
    assert len(result.operations) == 1
    assert result.operations[0]["feature_id"] == "HVS-QN1"


def test_push_layer_geopackage_sync_formats_multipart_and_parses_response(
    tmp_path: Path,
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    changeset_id = str(uuid4())
    audit_event_id = str(uuid4())
    gpkg_path = _write_temp_gpkg(tmp_path, "push-edits.gpkg")
    content_rev = "a" * 64
    schema_rev = "b" * 64
    diff_sha = "c" * 64
    resulting_rev = "d" * 64
    captured: list[urllib.request.Request] = []

    push_payload = {
        "changeset_id": changeset_id,
        "resulting_content_revision": resulting_rev,
        "operation_count": 2,
        "create_count": 1,
        "update_count": 1,
        "omitted_server_feature_count": 2,
        "deletions_blocked": True,
        "diff_sha256": diff_sha,
        "audit_event_id": audit_event_id,
    }

    def fake_opener(request: urllib.request.Request, *, timeout: float) -> _FakeHttpResponse:
        del timeout
        captured.append(request)
        return _FakeHttpResponse(json.dumps(push_payload).encode("utf-8"))

    result = qgis_client.push_layer_geopackage_sync(
        cfg,
        layer_id=layer_id,
        gpkg_path=gpkg_path,
        base_content_revision=content_rev,
        base_schema_revision=schema_rev,
        expected_diff_sha256=diff_sha,
        operation_id="qgis-sync-2026-01",
        summary="Update survey points",
        opener=fake_opener,
    )

    assert len(captured) == 1
    req = captured[0]
    assert req.get_method() == "POST"
    assert req.full_url == (
        f"https://gis.example.vn/api/v1/organizations/{cfg.organization_id}"
        f"/projects/{cfg.project_id}/integration-hub/adapters/qgis"
        f"/layers/{layer_id}/sync-push"
    )
    body = req.data
    assert isinstance(body, bytes)
    assert b'name="operation_id"\r\n\r\nqgis-sync-2026-01\r\n' in body
    assert b'name="expected_diff_sha256"\r\n\r\n' + diff_sha.encode("ascii") in body
    assert b'name="summary"\r\n\r\nUpdate survey points\r\n' in body
    assert gpkg_path.read_bytes() in body

    assert result.changeset_id == changeset_id
    assert result.resulting_content_revision == resulting_rev
    assert result.operation_count == 2
    assert result.create_count == 1
    assert result.update_count == 1
    assert result.omitted_server_feature_count == 2
    assert result.deletions_blocked is True
    assert result.diff_sha256 == diff_sha
    assert result.audit_event_id == audit_event_id


def test_sync_rejects_oversized_geopackage_before_network(tmp_path: Path) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    oversized = _write_temp_gpkg(
        tmp_path,
        "too-large.gpkg",
        size=qgis_client.MAX_SYNC_UPLOAD_BYTES + 1,
    )

    def fail_opener(*_args: Any, **_kwargs: Any) -> Any:
        pytest.fail("must not send HTTP request for >16 MiB GeoPackage")

    with pytest.raises(qgis_client.ConfigurationError, match="16 MiB"):
        qgis_client.preview_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=oversized,
            base_content_revision="a" * 64,
            base_schema_revision="b" * 64,
            opener=fail_opener,
        )

    with pytest.raises(qgis_client.ConfigurationError, match="16 MiB"):
        qgis_client.push_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=oversized,
            base_content_revision="a" * 64,
            base_schema_revision="b" * 64,
            expected_diff_sha256="c" * 64,
            operation_id="qgis-op-1",
            opener=fail_opener,
        )


@pytest.mark.parametrize("bad_hex", ["", "short", "z" * 64, "a" * 63, "a" * 65])
def test_sync_rejects_invalid_64_hex_revisions_and_diff_sha256(
    tmp_path: Path,
    bad_hex: str,
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    gpkg_path = _write_temp_gpkg(tmp_path)

    def fail_opener(*_args: Any, **_kwargs: Any) -> Any:
        pytest.fail("must validate 64-hex hashes before network call")

    with pytest.raises(qgis_client.ConfigurationError, match="base_content_revision"):
        qgis_client.preview_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=gpkg_path,
            base_content_revision=bad_hex,
            base_schema_revision="b" * 64,
            opener=fail_opener,
        )

    with pytest.raises(qgis_client.ConfigurationError, match="base_schema_revision"):
        qgis_client.preview_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=gpkg_path,
            base_content_revision="a" * 64,
            base_schema_revision=bad_hex,
            opener=fail_opener,
        )

    with pytest.raises(qgis_client.ConfigurationError, match="expected_diff_sha256"):
        qgis_client.push_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=gpkg_path,
            base_content_revision="a" * 64,
            base_schema_revision="b" * 64,
            expected_diff_sha256=bad_hex,
            operation_id="qgis-op-1",
            opener=fail_opener,
        )


@pytest.mark.parametrize("reserved_op_id", ["field-sync-123", "FIELD-SYNC-abc", "  field-sync-x  "])
def test_push_sync_rejects_reserved_field_sync_operation_id(
    tmp_path: Path,
    reserved_op_id: str,
) -> None:
    cfg = _valid_config()
    layer_id = str(uuid4())
    gpkg_path = _write_temp_gpkg(tmp_path)

    def fail_opener(*_args: Any, **_kwargs: Any) -> Any:
        pytest.fail("must reject reserved field-sync-* operation_id before network call")

    with pytest.raises(qgis_client.ConfigurationError, match="field-sync-"):
        qgis_client.push_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=gpkg_path,
            base_content_revision="a" * 64,
            base_schema_revision="b" * 64,
            expected_diff_sha256="c" * 64,
            operation_id=reserved_op_id,
            opener=fail_opener,
        )


def test_sync_preview_and_push_redact_bearer_token_on_http_error(tmp_path: Path) -> None:
    secret = "ovg_sync_secret_token_leaked_in_409"
    cfg = _valid_config(bearer_token=secret)
    layer_id = str(uuid4())
    gpkg_path = _write_temp_gpkg(tmp_path)

    def error_opener(request: urllib.request.Request, *, timeout: float) -> Any:
        del timeout
        err_body = json.dumps({"detail": f"sync conflict for bearer {secret}"}).encode("utf-8")
        raise urllib.error.HTTPError(
            request.full_url,
            409,
            "Conflict",
            Message(),
            io.BytesIO(err_body),
        )

    with pytest.raises(qgis_client.ApiError) as preview_exc:
        qgis_client.preview_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=gpkg_path,
            base_content_revision="a" * 64,
            base_schema_revision="b" * 64,
            opener=error_opener,
        )
    assert preview_exc.value.status_code == 409
    assert secret not in str(preview_exc.value)
    assert "[REDACTED]" in str(preview_exc.value)

    with pytest.raises(qgis_client.ApiError) as push_exc:
        qgis_client.push_layer_geopackage_sync(
            cfg,
            layer_id=layer_id,
            gpkg_path=gpkg_path,
            base_content_revision="a" * 64,
            base_schema_revision="b" * 64,
            expected_diff_sha256="c" * 64,
            operation_id="qgis-op-409",
            opener=error_opener,
        )
    assert push_exc.value.status_code == 409
    assert secret not in str(push_exc.value)
    assert "[REDACTED]" in str(push_exc.value)
