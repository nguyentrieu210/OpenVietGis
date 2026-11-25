"""HTTP handler security invariants without claiming real DB/RLS coverage."""

from __future__ import annotations

import hashlib
import io
import json
from datetime import UTC, datetime
from types import SimpleNamespace
from typing import Any
from uuid import UUID, uuid4

import pytest
from ezdxf.filemanagement import read as read_dxf
from fastapi import HTTPException, UploadFile
from fastapi.routing import APIRoute

from openvietgis_api.adapters.formats.geopackage import (
    inspect_geopackage,
    parse_geopackage_member,
    write_geopackage,
)
from openvietgis_api.adapters.http import integration_hub
from openvietgis_api.domains.authorization.context import Actor
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.editing.domain import (
    ChangesetRecord,
    EditOperationInput,
    EditOpType,
)
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping

_CONTENT_REV_A = "a" * 64
_SCHEMA_REV_B = "b" * 64
_RESULTING_REV_C = "c" * 64


def _auth() -> Any:
    return SimpleNamespace(
        actor=Actor("human", uuid4(), "test-user"),
        credential_id=None,
    )


def _as_session(obj: object) -> Any:
    return obj


def _upload(payload: bytes, filename: str = "sync-edit.gpkg") -> UploadFile:
    return UploadFile(filename=filename, file=io.BytesIO(payload))


class _ReaderProvider:
    def __init__(self, _session: object) -> None:
        pass

    def resolve(self, _context: object) -> frozenset[Permission]:
        return frozenset({Permission.PROJECT_READ})


class _FakeMappingsResult:
    def __init__(self, rows: list[dict[str, object]]) -> None:
        self._rows = rows

    def mappings(self) -> _FakeMappingsResult:
        return self

    def all(self) -> list[dict[str, object]]:
        return list(self._rows)

    def first(self) -> dict[str, object] | None:
        return self._rows[0] if self._rows else None


class _FakeLayerSession:
    def __init__(self, rows: list[dict[str, object]]) -> None:
        self._rows = rows
        self.execute_calls = 0

    def execute(self, _sql: object, _params: dict[str, object]) -> _FakeMappingsResult:
        self.execute_calls += 1
        return _FakeMappingsResult(self._rows)


class _FakeExportSession:
    def __init__(
        self,
        *,
        layer_row: dict[str, object] | None,
        field_rows: list[dict[str, object]] | None = None,
        feature_rows: list[dict[str, object]] | None = None,
    ) -> None:
        self._layer_row = layer_row
        self._field_rows = field_rows or []
        self._feature_rows = feature_rows or []
        self.executed_sql: list[str] = []

    def execute(self, sql: object, _params: dict[str, object]) -> _FakeMappingsResult:
        sql_text = str(sql)
        self.executed_sql.append(sql_text)
        if "FROM spatial.layers" in sql_text:
            return _FakeMappingsResult([self._layer_row] if self._layer_row is not None else [])
        if "FROM spatial.source_fields" in sql_text:
            return _FakeMappingsResult(self._field_rows)
        if "FROM spatial.reference_features" in sql_text:
            return _FakeMappingsResult(self._feature_rows)
        raise AssertionError(f"Unexpected SQL query in fake export session: {sql_text}")


class _ForbiddenQuerySession:
    def execute(self, _sql: object, _params: dict[str, object]) -> object:
        pytest.fail("must not query spatial layers for non-profile-enabled adapters")


def _sample_layer_row(
    *,
    layer_id: UUID | None = None,
    source_id: UUID | None = None,
    name: str = "VN2000 Quang Ninh Points",
    geometry_family: str = "Point",
    stored_crs_code: int | None = 5899,
    source_crs_code: int | None = 5899,
    validation_state: str = "ready",
    content_revision: str = _CONTENT_REV_A,
    schema_revision: str = _SCHEMA_REV_B,
) -> dict[str, object]:
    return {
        "layer_id": layer_id or uuid4(),
        "source_id": source_id or uuid4(),
        "name": name,
        "geometry_family": geometry_family,
        "dimensions": "XY",
        "validation_state": validation_state,
        "source_crs_code": source_crs_code,
        "stored_crs_code": stored_crs_code,
        "layer_revision": 3,
        "content_revision": content_revision,
        "schema_revision": schema_revision,
    }


def _build_sync_gpkg(
    *,
    crs_code: int = 5899,
    features: list[tuple[str, dict[str, Any], NativeGeometry]],
) -> bytes:
    field_mappings = [
        ExportFieldMapping("fld_code", "Code", "fld_code", FieldType.TEXT),
        ExportFieldMapping("fld_rank", "Rank", "fld_rank", FieldType.INTEGER),
        ExportFieldMapping("fld_active", "Active", "fld_active", FieldType.BOOLEAN),
    ]
    payload, _ext, _media_type = write_geopackage(
        layer_name="VN2000 Control Points",
        crs_code=crs_code,
        geometry_family="Point",
        field_mappings=field_mappings,
        features=features,
    )
    return payload


def _sync_fixture_session(
    *,
    layer_id: UUID,
    source_id: UUID,
    stored_crs_code: int = 5899,
    content_revision: str = _CONTENT_REV_A,
    schema_revision: str = _SCHEMA_REV_B,
) -> _FakeExportSession:
    return _FakeExportSession(
        layer_row=_sample_layer_row(
            layer_id=layer_id,
            source_id=source_id,
            name="VN2000 Control Points",
            geometry_family="Point",
            stored_crs_code=stored_crs_code,
            source_crs_code=stored_crs_code,
            content_revision=content_revision,
            schema_revision=schema_revision,
        ),
        field_rows=[
            {"field_id": "fld_code", "name": "Code", "type": "text"},
            {"field_id": "fld_rank", "name": "Rank", "type": "integer"},
            {"field_id": "fld_active", "name": "Active", "type": "boolean"},
        ],
        feature_rows=[
            {
                "feature_id": "HVS-QN1",
                "attributes": {"fld_code": "QN-01", "fld_rank": 1, "fld_active": True},
                "geom_json": json.dumps({"type": "Point", "coordinates": [438442.69, 2291039.83]}),
            },
            {
                "feature_id": "HVS-QN2",
                "attributes": {"fld_code": "QN-02", "fld_rank": 2, "fld_active": False},
                "geom_json": json.dumps({"type": "Point", "coordinates": [437957.55, 2291878.33]}),
            },
            {
                "feature_id": "HVS-QN3",
                "attributes": {"fld_code": "QN-03", "fld_rank": 3, "fld_active": True},
                "geom_json": json.dumps({"type": "Point", "coordinates": [439000.0, 2292000.0]}),
            },
        ],
    )


def test_handler_denies_before_catalog(monkeypatch: pytest.MonkeyPatch) -> None:
    org, project = uuid4(), uuid4()
    calls: list[tuple[Any, Permission]] = []

    def deny(_session: object, context: Any, *, permission: Permission) -> None:
        calls.append((context, permission))
        raise PermissionError("project denied")

    monkeypatch.setattr(integration_hub, "prepare_project_scope", deny)
    monkeypatch.setattr(
        integration_hub,
        "list_adapters",
        lambda _permissions: pytest.fail("must not return catalog before authorization"),
    )
    with pytest.raises(PermissionError, match="project denied"):
        integration_hub.read_integration_catalog(org, project, _auth(), _as_session(object()))
    assert len(calls) == 1
    assert calls[0][1] is Permission.PROJECT_READ
    assert calls[0][0].organization_id == org
    assert calls[0][0].project_id == project


@pytest.mark.parametrize("scoped_flag", [False, True])
def test_authorized_reader_receives_read_only_inventory(
    monkeypatch: pytest.MonkeyPatch, scoped_flag: bool
) -> None:
    org, project = uuid4(), uuid4()
    seen: list[object] = []

    def prepare(_session: object, context: object, *, permission: Permission) -> None:
        assert permission is Permission.PROJECT_READ
        seen.append(context)

    monkeypatch.setattr(integration_hub, "prepare_project_scope", prepare)
    monkeypatch.setattr(integration_hub, "SqlEffectivePermissionProvider", _ReaderProvider)
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: scoped_flag)
    payload = integration_hub.read_integration_catalog(
        org, project, _auth(), _as_session(object())
    )
    assert len(seen) == 1
    assert payload["organization_id"] == str(org)
    assert payload["project_id"] == str(project)
    assert payload["scoped_access"] is scoped_flag
    assert payload["default_write_policy"] == "deny"
    adapters = payload["adapters"]
    assert isinstance(adapters, list)
    assert len(adapters) == 12
    for item in adapters:
        allowed_actions = {
            operation["action"] for operation in item["actions"] if operation["allowed"]
        }
        if item["key"] in ("qgis", "postgis", "autocad"):
            assert allowed_actions == {"catalog.discover", "data.export"}
            export_op = next(op for op in item["actions"] if op["action"] == "data.export")
            assert export_op["reason"] == "read_only_export"
        else:
            assert allowed_actions == {"catalog.discover"}


def test_router_exposes_exact_six_routes_and_no_put_patch_delete() -> None:
    api_routes = [route for route in integration_hub.router.routes if isinstance(route, APIRoute)]
    assert len(api_routes) == len(integration_hub.router.routes) == 6

    route_map = {
        route.path.split("/projects/{project_id}", 1)[1]: (
            set(route.methods or ()),
            route.status_code,
        )
        for route in api_routes
    }
    expected = {
        "/integration-hub/catalog": {"GET"},
        "/integration-hub/export-scopes": {"GET"},
        "/integration-hub/adapters/{adapter_key}": {"GET"},
        "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export": {"GET"},
        "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-preview": {"POST"},
        "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-push": {"POST"},
    }
    assert {path: methods for path, (methods, _status) in route_map.items()} == expected
    assert (
        route_map["/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-push"][1] == 201
    )

    forbidden_methods = {"PUT", "PATCH", "DELETE"}
    for _path, (methods, _status) in route_map.items():
        assert methods.isdisjoint(forbidden_methods)


def test_adapter_profile_denies_before_adapter_lookup(monkeypatch: pytest.MonkeyPatch) -> None:
    org, project = uuid4(), uuid4()
    calls: list[tuple[Any, Permission]] = []

    def deny(_session: object, context: Any, *, permission: Permission) -> None:
        calls.append((context, permission))
        raise PermissionError("project denied")

    monkeypatch.setattr(integration_hub, "prepare_project_scope", deny)
    monkeypatch.setattr(
        integration_hub,
        "get_adapter",
        lambda _key: pytest.fail("must not look up adapters before authorization"),
    )
    with pytest.raises(PermissionError, match="project denied"):
        integration_hub.read_integration_adapter_profile(
            org, project, "nonexistent", _auth(), _as_session(object())
        )
    assert len(calls) == 1
    assert calls[0][1] is Permission.PROJECT_READ
    assert calls[0][0].organization_id == org
    assert calls[0][0].project_id == project


def test_adapter_profile_returns_404_for_unknown_adapter_after_auth(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project = uuid4(), uuid4()
    seen: list[tuple[object, Permission]] = []

    def prepare(_session: object, context: object, *, permission: Permission) -> None:
        seen.append((context, permission))

    monkeypatch.setattr(integration_hub, "prepare_project_scope", prepare)
    with pytest.raises(HTTPException) as exc_info:
        integration_hub.read_integration_adapter_profile(
            org, project, "unknown_vendor_adapter", _auth(), _as_session(_ForbiddenQuerySession())
        )
    assert exc_info.value.status_code == 404
    assert len(seen) == 1
    assert seen[0][1] is Permission.PROJECT_READ


@pytest.mark.parametrize("adapter_key", ["qgis", "postgis", "autocad"])
def test_adapter_profile_ready_adapters_enforce_scoped_acl_and_crs(
    monkeypatch: pytest.MonkeyPatch, adapter_key: str
) -> None:
    org, project = uuid4(), uuid4()
    src_exportable = uuid4()
    src_read_only = uuid4()
    src_missing_crs = uuid4()
    src_denied = uuid4()

    rows: list[dict[str, object]] = [
        {
            "layer_id": uuid4(),
            "source_id": src_exportable,
            "name": "VN2000 Exportable",
            "geometry_family": "Point",
            "dimensions": "XY",
            "validation_state": "ready",
            "source_crs_code": 5899,
            "stored_crs_code": 5899,
            "layer_revision": 1,
            "content_revision": "rev-1",
            "schema_revision": "sch-1",
        },
        {
            "layer_id": uuid4(),
            "source_id": src_read_only,
            "name": "VN2000 Read Only",
            "geometry_family": "Polygon",
            "dimensions": "XY",
            "validation_state": "ready",
            "source_crs_code": 4756,
            "stored_crs_code": 4756,
            "layer_revision": 2,
            "content_revision": "rev-2",
            "schema_revision": "sch-2",
        },
        {
            "layer_id": uuid4(),
            "source_id": src_missing_crs,
            "name": "Missing CRS Layer",
            "geometry_family": "Point",
            "dimensions": "XY",
            "validation_state": "missing_crs",
            "source_crs_code": None,
            "stored_crs_code": None,
            "layer_revision": 1,
            "content_revision": "rev-3",
            "schema_revision": "sch-3",
        },
        {
            "layer_id": uuid4(),
            "source_id": src_denied,
            "name": "Denied Layer",
            "geometry_family": "LineString",
            "dimensions": "XY",
            "validation_state": "ready",
            "source_crs_code": 4326,
            "stored_crs_code": 4326,
            "layer_revision": 1,
            "content_revision": "rev-4",
            "schema_revision": "sch-4",
        },
    ]

    acl_matrix: dict[tuple[object, str], bool] = {
        (src_exportable, "read"): True,
        (src_exportable, "export"): True,
        (src_read_only, "read"): True,
        (src_read_only, "export"): False,
        (src_missing_crs, "read"): True,
        (src_missing_crs, "export"): True,
        (src_denied, "read"): False,
        (src_denied, "export"): True,
    }

    session = _FakeLayerSession(rows)
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "SqlEffectivePermissionProvider", _ReaderProvider)
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: True)
    monkeypatch.setattr(
        integration_hub,
        "_source_allowed",
        lambda _s, _c, source_id, action: acl_matrix[(source_id, action)],
    )

    payload = integration_hub.read_integration_adapter_profile(
        org, project, adapter_key, _auth(), _as_session(session)
    )
    assert session.execute_calls == 1
    assert payload["execution_status"] == "read_only_profile_ready"
    assert payload["default_write_policy"] == "deny"
    assert payload["scoped_access"] is True

    profile = payload["connection_profile"]
    assert isinstance(profile, dict)
    assert profile["supports_direct_db_credentials"] is False
    assert profile["default_write_policy"] == "deny"

    layers = payload["layers"]
    assert isinstance(layers, list)
    by_name = {layer["name"]: layer for layer in layers}
    assert set(by_name) == {
        "VN2000 Exportable",
        "VN2000 Read Only",
        "Missing CRS Layer",
    }
    assert "Denied Layer" not in by_name

    assert by_name["VN2000 Exportable"]["stored_crs_code"] == 5899
    assert by_name["VN2000 Exportable"]["crs_explicit"] is True
    assert by_name["VN2000 Exportable"]["export_permitted"] is True

    assert by_name["VN2000 Read Only"]["stored_crs_code"] == 4756
    assert by_name["VN2000 Read Only"]["crs_explicit"] is True
    assert by_name["VN2000 Read Only"]["export_permitted"] is False

    assert by_name["Missing CRS Layer"]["stored_crs_code"] is None
    assert by_name["Missing CRS Layer"]["crs_explicit"] is False
    assert by_name["Missing CRS Layer"]["export_permitted"] is False


@pytest.mark.parametrize("adapter_key", ["arcgis", "mapinfo"])
def test_adapter_profile_planned_adapters_skip_layer_query(
    monkeypatch: pytest.MonkeyPatch, adapter_key: str
) -> None:
    org, project = uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "SqlEffectivePermissionProvider", _ReaderProvider)
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    payload = integration_hub.read_integration_adapter_profile(
        org, project, adapter_key, _auth(), _as_session(_ForbiddenQuerySession())
    )
    assert payload["execution_status"] == "planned_not_implemented"
    assert payload["connection_profile"] is None
    assert payload["layers"] == []
    assert payload["default_write_policy"] == "deny"


def test_adapter_profile_microstation_candidate_external_branch(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project = uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "SqlEffectivePermissionProvider", _ReaderProvider)
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    payload = integration_hub.read_integration_adapter_profile(
        org, project, "microstation_v8i", _auth(), _as_session(_ForbiddenQuerySession())
    )
    assert payload["execution_status"] == "candidate_external_branch"
    assert payload["connection_profile"] is None
    assert payload["layers"] == []
    assert payload["default_write_policy"] == "deny"


def test_export_integration_layer_checks_auth_before_adapter_lookup(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    calls: list[tuple[Any, Permission]] = []

    def deny(_session: object, context: Any, *, permission: Permission) -> None:
        calls.append((context, permission))
        raise PermissionError("project denied")

    monkeypatch.setattr(integration_hub, "prepare_project_scope", deny)
    monkeypatch.setattr(
        integration_hub,
        "get_adapter",
        lambda _key: pytest.fail("must not look up adapters before authorization"),
    )
    with pytest.raises(PermissionError, match="project denied"):
        integration_hub.export_integration_layer(
            org, project, "nonexistent", layer_id, _auth(), _as_session(_ForbiddenQuerySession())
        )
    assert len(calls) == 1
    assert calls[0][1] is Permission.PROJECT_READ
    assert calls[0][0].organization_id == org
    assert calls[0][0].project_id == project


def test_export_integration_layer_unknown_adapter_returns_404(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org,
            project,
            "unknown_adapter",
            layer_id,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )
    assert exc_info.value.status_code == 404


@pytest.mark.parametrize("adapter_key", ["arcgis", "ogc_services", "microstation_v8i"])
def test_export_integration_layer_non_export_adapter_returns_409(
    monkeypatch: pytest.MonkeyPatch,
    adapter_key: str,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org, project, adapter_key, layer_id, _auth(), _as_session(_ForbiddenQuerySession())
        )
    assert exc_info.value.status_code == 409


def test_export_integration_layer_autocad_dxf_succeeds_on_epsg_5899(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    session = _FakeExportSession(
        layer_row=_sample_layer_row(
            layer_id=layer_id,
            name="VN2000 CAD Survey",
            geometry_family="Point",
            stored_crs_code=5899,
            source_crs_code=5899,
        ),
        field_rows=[{"field_id": "fld_code", "name": "Code", "type": "text"}],
        feature_rows=[
            {
                "feature_id": "HVS-QN1",
                "attributes": {"fld_code": "QN-01"},
                "geom_json": json.dumps({"type": "Point", "coordinates": [438442.69, 2291039.83]}),
            },
            {
                "feature_id": "HVS-QN2",
                "attributes": {"fld_code": "QN-02"},
                "geom_json": json.dumps({"type": "Point", "coordinates": [437957.55, 2291878.33]}),
            },
        ],
    )
    audit_calls: list[dict[str, Any]] = []
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)
    monkeypatch.setattr(
        integration_hub,
        "_record_export_audit",
        lambda _s, _c, **kwargs: audit_calls.append(kwargs),
    )

    response = integration_hub.export_integration_layer(
        org, project, "autocad", layer_id, _auth(), _as_session(session), format="dxf"
    )
    payload = bytes(response.body)
    expected_sha = hashlib.sha256(payload).hexdigest()
    dxf_text = payload.decode("utf-8")

    assert response.status_code == 200
    assert response.media_type == "application/dxf"
    assert response.headers["Cache-Control"] == "no-store"
    assert response.headers["X-OpenVietGIS-Attribute-Loss"] == "true"
    assert response.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
    assert response.headers["X-Content-SHA256"] == expected_sha
    assert response.headers["X-OpenVietGIS-Feature-Count"] == "2"
    assert response.headers["Content-Disposition"].endswith('-autocad.dxf"')

    assert "SECTION" in dxf_text
    assert "ENDSEC" in dxf_text
    assert "EOF" in dxf_text
    doc = read_dxf(io.StringIO(dxf_text))
    assert doc.dxfversion == "AC1024"  # R2010
    entities = list(doc.modelspace())
    assert len(entities) == 2
    assert all(ent.dxftype() == "POINT" for ent in entities)

    assert len(audit_calls) == 1
    assert audit_calls[0]["adapter_key"] == "autocad"
    assert audit_calls[0]["export_format"] == "dxf"
    assert audit_calls[0]["stored_crs_code"] == 5899
    assert audit_calls[0]["sha256"] == expected_sha


def test_export_integration_layer_autocad_rejects_geopackage_format_422(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org,
            project,
            "autocad",
            layer_id,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
            format="geopackage",
        )
    assert exc_info.value.status_code == 422
    assert "dxf" in str(exc_info.value.detail).lower()


# EPSG:4326 is now converted exactly to EPSG:3857 for DXF; other degree CRS still fail closed.
@pytest.mark.parametrize("geographic_crs", [4756])
def test_export_integration_layer_dxf_rejects_geographic_degree_crs_422(
    monkeypatch: pytest.MonkeyPatch,
    geographic_crs: int,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    session = _FakeExportSession(
        layer_row=_sample_layer_row(
            layer_id=layer_id,
            stored_crs_code=geographic_crs,
            source_crs_code=geographic_crs,
        )
    )
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org, project, "autocad", layer_id, _auth(), _as_session(session), format="dxf"
        )
    assert exc_info.value.status_code == 422
    assert "metre" in str(exc_info.value.detail).lower()


def test_export_integration_layer_scoped_denied_read_returns_404(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    session = _FakeExportSession(layer_row=_sample_layer_row(layer_id=layer_id))
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: True)
    monkeypatch.setattr(
        integration_hub,
        "_source_allowed",
        lambda _s, _c, _sid, action: action != "read",
    )

    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org, project, "qgis", layer_id, _auth(), _as_session(session)
        )
    assert exc_info.value.status_code == 404


def test_export_integration_layer_scoped_read_only_denied_export_returns_403(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    session = _FakeExportSession(layer_row=_sample_layer_row(layer_id=layer_id))
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: True)
    monkeypatch.setattr(
        integration_hub,
        "_source_allowed",
        lambda _s, _c, _sid, action: action == "read",
    )

    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org, project, "qgis", layer_id, _auth(), _as_session(session)
        )
    assert exc_info.value.status_code == 403


@pytest.mark.parametrize(
    ("stored_crs", "validation_state"),
    [
        (None, "missing_crs"),
        (None, "ready"),
        (3405, "ready"),
        (3406, "ready"),
    ],
)
def test_export_integration_layer_rejects_missing_or_definition_only_crs_422(
    monkeypatch: pytest.MonkeyPatch,
    stored_crs: int | None,
    validation_state: str,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    session = _FakeExportSession(
        layer_row=_sample_layer_row(
            layer_id=layer_id,
            stored_crs_code=stored_crs,
            source_crs_code=stored_crs,
            validation_state=validation_state,
        )
    )
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org, project, "qgis", layer_id, _auth(), _as_session(session), format="geopackage"
        )
    assert exc_info.value.status_code == 422


def test_export_integration_layer_refuses_geojson_on_non_4326_layer_422(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    session = _FakeExportSession(
        layer_row=_sample_layer_row(layer_id=layer_id, stored_crs_code=5899, source_crs_code=5899)
    )
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    with pytest.raises(HTTPException) as exc_info:
        integration_hub.export_integration_layer(
            org, project, "qgis", layer_id, _auth(), _as_session(session), format="geojson"
        )
    assert exc_info.value.status_code == 422
    assert "4326" in str(exc_info.value.detail)


def test_export_integration_layer_qgis_geopackage_round_trips_vn2000_5899_and_audits(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    layer_row = _sample_layer_row(
        layer_id=layer_id,
        name="VN2000 Control Points",
        geometry_family="Point",
        stored_crs_code=5899,
        source_crs_code=5899,
        content_revision="content-rev-5899",
        schema_revision="schema-rev-5899",
    )
    field_rows: list[dict[str, object]] = [
        {"field_id": "fld_code", "name": "Code", "type": "text"},
        {"field_id": "fld_rank", "name": "Rank", "type": "integer"},
        {"field_id": "fld_active", "name": "Active", "type": "boolean"},
    ]
    feature_rows: list[dict[str, object]] = [
        {
            "feature_id": "HVS-QN1",
            "attributes": {"fld_code": "QN-01", "fld_rank": 1, "fld_active": True},
            "geom_json": json.dumps({"type": "Point", "coordinates": [438442.69, 2291039.83]}),
        },
        {
            "feature_id": "HVS-QN2",
            "attributes": {"fld_code": "QN-02", "fld_rank": 2, "fld_active": False},
            "geom_json": json.dumps({"type": "Point", "coordinates": [437957.55, 2291878.33]}),
        },
    ]
    session = _FakeExportSession(
        layer_row=layer_row,
        field_rows=field_rows,
        feature_rows=feature_rows,
    )
    audit_calls: list[dict[str, Any]] = []

    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)
    monkeypatch.setattr(
        integration_hub,
        "_record_export_audit",
        lambda _s, _c, **kwargs: audit_calls.append(kwargs),
    )

    response = integration_hub.export_integration_layer(
        org, project, "qgis", layer_id, _auth(), _as_session(session), format="geopackage"
    )
    payload = bytes(response.body)
    expected_sha = hashlib.sha256(payload).hexdigest()

    assert response.status_code == 200
    assert response.media_type == "application/geopackage+sqlite3"
    assert response.headers["Cache-Control"] == "no-store"
    assert response.headers["X-Content-SHA256"] == expected_sha
    assert response.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
    assert response.headers["X-OpenVietGIS-Content-Revision"] == "content-rev-5899"
    assert response.headers["X-OpenVietGIS-Schema-Revision"] == "schema-rev-5899"
    assert response.headers["X-OpenVietGIS-Feature-Count"] == "2"
    assert response.headers["Content-Disposition"].endswith('-qgis.gpkg"')

    members = inspect_geopackage(payload)
    assert len(members) == 1
    member = members[0]
    assert member.eligible is True
    assert member.detected_crs == 5899
    assert member.feature_count == 2

    parsed = parse_geopackage_member(payload, member_name=member.name)
    assert parsed.detected_crs == 5899
    assert parsed.findings == ()
    assert [f.feature_id for f in parsed.features] == ["HVS-QN1", "HVS-QN2"]
    assert parsed.features[0].geometry.srid == 5899
    assert parsed.features[0].geometry.type == "Point"
    assert parsed.features[0].geometry.coordinates == pytest.approx((438442.69, 2291039.83))
    assert parsed.features[0].attributes == {
        "fld_code": "QN-01",
        "fld_rank": 1,
        "fld_active": True,
    }
    assert parsed.features[1].geometry.srid == 5899
    assert parsed.features[1].geometry.coordinates == pytest.approx((437957.55, 2291878.33))
    assert parsed.features[1].attributes == {
        "fld_code": "QN-02",
        "fld_rank": 2,
        "fld_active": False,
    }

    assert len(audit_calls) == 1
    assert audit_calls[0] == {
        "layer_id": layer_id,
        "adapter_key": "qgis",
        "export_format": "geopackage",
        "stored_crs_code": 5899,
        "feature_count": 2,
        "content_revision": "content-rev-5899",
        "schema_revision": "schema-rev-5899",
        "sha256": expected_sha,
        "scoped_access": False,
    }


def test_export_integration_layer_postgis_sql_returns_transactional_sql_with_srid_and_sha256(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    layer_row = _sample_layer_row(
        layer_id=layer_id,
        name="VN2000 Stations",
        geometry_family="Point",
        stored_crs_code=5899,
        source_crs_code=5899,
    )
    field_rows: list[dict[str, object]] = [
        {"field_id": "fld_name", "name": "Name", "type": "text"},
        {"field_id": "fld_rank", "name": "Rank", "type": "integer"},
    ]
    feature_rows: list[dict[str, object]] = [
        {
            "feature_id": "HVS-QN1",
            "attributes": {"fld_name": "Tram 1", "fld_rank": 7},
            "geom_json": json.dumps({"type": "Point", "coordinates": [438442.69, 2291039.83]}),
        },
    ]
    session = _FakeExportSession(
        layer_row=layer_row,
        field_rows=field_rows,
        feature_rows=feature_rows,
    )
    audit_calls: list[dict[str, Any]] = []

    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)
    monkeypatch.setattr(
        integration_hub,
        "_record_export_audit",
        lambda _s, _c, **kwargs: audit_calls.append(kwargs),
    )

    response = integration_hub.export_integration_layer(
        org, project, "postgis", layer_id, _auth(), _as_session(session), format="postgis_sql"
    )
    payload = bytes(response.body)
    expected_sha = hashlib.sha256(payload).hexdigest()
    sql_text = payload.decode("utf-8")

    assert response.status_code == 200
    assert response.media_type == "application/sql; charset=utf-8"
    assert response.headers["Cache-Control"] == "no-store"
    assert response.headers["X-Content-SHA256"] == expected_sha
    assert response.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
    assert response.headers["X-OpenVietGIS-Feature-Count"] == "1"
    assert response.headers["Content-Disposition"].endswith('-postgis.sql"')

    assert "BEGIN;" in sql_text
    assert "CREATE TABLE IF NOT EXISTS" in sql_text
    assert "geom geometry(POINT, 5899)" in sql_text
    assert "ST_GeomFromText('POINT(438442.69 2291039.83)', 5899)" in sql_text
    assert "'HVS-QN1'" in sql_text
    assert "'Tram 1'" in sql_text
    assert "USING GIST (geom);" in sql_text
    assert sql_text.rstrip().endswith("COMMIT;")

    assert len(audit_calls) == 1
    assert audit_calls[0]["adapter_key"] == "postgis"
    assert audit_calls[0]["export_format"] == "postgis_sql"
    assert audit_calls[0]["sha256"] == expected_sha


def test_sync_preview_and_push_check_project_edit_before_adapter_lookup(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    calls: list[tuple[Any, Permission]] = []

    def deny(_session: object, context: Any, *, permission: Permission) -> None:
        calls.append((context, permission))
        raise PermissionError("edit denied")

    monkeypatch.setattr(integration_hub, "prepare_project_scope", deny)
    monkeypatch.setattr(
        integration_hub,
        "get_adapter",
        lambda _key: pytest.fail("must not look up adapters before PROJECT_EDIT authorization"),
    )

    with pytest.raises(PermissionError, match="edit denied"):
        integration_hub.preview_integration_layer_sync(
            org,
            project,
            "nonexistent",
            layer_id,
            _upload(b"stub"),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )

    with pytest.raises(PermissionError, match="edit denied"):
        integration_hub.push_integration_layer_sync(
            org,
            project,
            "nonexistent",
            layer_id,
            _upload(b"stub"),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            "qgis-op-1",
            _RESULTING_REV_C,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )

    assert len(calls) == 2
    assert all(perm is Permission.PROJECT_EDIT for _ctx, perm in calls)
    assert all(ctx.organization_id == org and ctx.project_id == project for ctx, _perm in calls)


@pytest.mark.parametrize("adapter_key", ["postgis", "autocad", "arcgis"])
def test_sync_preview_and_push_reject_non_qgis_adapters_409(
    monkeypatch: pytest.MonkeyPatch,
    adapter_key: str,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )

    with pytest.raises(HTTPException) as preview_exc:
        integration_hub.preview_integration_layer_sync(
            org,
            project,
            adapter_key,
            layer_id,
            _upload(b"stub"),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )
    assert preview_exc.value.status_code == 409

    with pytest.raises(HTTPException) as push_exc:
        integration_hub.push_integration_layer_sync(
            org,
            project,
            adapter_key,
            layer_id,
            _upload(b"stub"),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            "qgis-op-1",
            _RESULTING_REV_C,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )
    assert push_exc.value.status_code == 409


def test_sync_preview_and_push_reject_unknown_adapter_404(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id = uuid4(), uuid4(), uuid4()
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )

    with pytest.raises(HTTPException) as preview_exc:
        integration_hub.preview_integration_layer_sync(
            org,
            project,
            "unknown_adapter",
            layer_id,
            _upload(b"stub"),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )
    assert preview_exc.value.status_code == 404

    with pytest.raises(HTTPException) as push_exc:
        integration_hub.push_integration_layer_sync(
            org,
            project,
            "unknown_adapter",
            layer_id,
            _upload(b"stub"),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            "qgis-op-1",
            _RESULTING_REV_C,
            _auth(),
            _as_session(_ForbiddenQuerySession()),
        )
    assert push_exc.value.status_code == 404


def test_sync_preview_rejects_oversized_upload_413(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id, source_id = uuid4(), uuid4(), uuid4(), uuid4()
    session = _sync_fixture_session(layer_id=layer_id, source_id=source_id)
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    oversized = b"\x00" * (integration_hub.MAX_SYNC_UPLOAD_BYTES + 1)
    with pytest.raises(HTTPException) as exc_info:
        integration_hub.preview_integration_layer_sync(
            org,
            project,
            "qgis",
            layer_id,
            _upload(oversized),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            _auth(),
            _as_session(session),
        )
    assert exc_info.value.status_code == 413


def test_sync_preview_rejects_crs_mismatch_422(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id, source_id = uuid4(), uuid4(), uuid4(), uuid4()
    session = _sync_fixture_session(layer_id=layer_id, source_id=source_id, stored_crs_code=5899)
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    gpkg_4326 = _build_sync_gpkg(
        crs_code=4326,
        features=[
            (
                "HVS-QN1",
                {"fld_code": "QN-01", "fld_rank": 1, "fld_active": True},
                NativeGeometry("Point", (107.156, 20.711), 4326),
            )
        ],
    )
    with pytest.raises(HTTPException) as exc_info:
        integration_hub.preview_integration_layer_sync(
            org,
            project,
            "qgis",
            layer_id,
            _upload(gpkg_4326),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            _auth(),
            _as_session(session),
        )
    assert exc_info.value.status_code == 422
    assert "4326" in str(exc_info.value.detail)
    assert "5899" in str(exc_info.value.detail)


def test_sync_preview_and_push_with_real_geopackage_and_guardrails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    org, project, layer_id, source_id = uuid4(), uuid4(), uuid4(), uuid4()
    session = _sync_fixture_session(layer_id=layer_id, source_id=source_id)
    monkeypatch.setattr(
        integration_hub, "prepare_project_scope", lambda _s, _c, *, permission: None
    )
    monkeypatch.setattr(integration_hub, "_is_scoped", lambda _s, _c: False)

    # Upload edits HVS-QN1 (attribute update), keeps HVS-QN2 unchanged,
    # omits HVS-QN3, and creates HVS-QN4.
    edited_gpkg = _build_sync_gpkg(
        crs_code=5899,
        features=[
            (
                "HVS-QN1",
                {"fld_code": "QN-01-EDITED", "fld_rank": 1, "fld_active": True},
                NativeGeometry("Point", (438442.69, 2291039.83), 5899),
            ),
            (
                "HVS-QN2",
                {"fld_code": "QN-02", "fld_rank": 2, "fld_active": False},
                NativeGeometry("Point", (437957.55, 2291878.33), 5899),
            ),
            (
                "HVS-QN4",
                {"fld_code": "QN-04-NEW", "fld_rank": 4, "fld_active": True},
                NativeGeometry("Point", (438500.0, 2291500.0), 5899),
            ),
        ],
    )

    preview = integration_hub.preview_integration_layer_sync(
        org,
        project,
        "qgis",
        layer_id,
        _upload(edited_gpkg),
        _CONTENT_REV_A,
        _SCHEMA_REV_B,
        _auth(),
        _as_session(session),
    )
    assert preview["adapter_key"] == "qgis"
    assert preview["layer_id"] == str(layer_id)
    assert preview["source_id"] == str(source_id)
    assert preview["stored_crs_code"] == 5899
    assert preview["stale_revision"] is False
    assert preview["deletions_blocked"] is True
    assert preview["create_count"] == 1
    assert preview["update_count"] == 1
    assert preview["unchanged_count"] == 1
    assert preview["omitted_server_feature_count"] == 1
    assert preview["omitted_server_feature_ids"] == ["HVS-QN3"]
    assert preview["can_commit"] is True
    diff_sha = str(preview["diff_sha256"])
    assert len(diff_sha) == 64

    # 1. Reserved field-sync-* operation_id is rejected with 422 before committing.
    with pytest.raises(HTTPException) as reserved_exc:
        integration_hub.push_integration_layer_sync(
            org,
            project,
            "qgis",
            layer_id,
            _upload(edited_gpkg),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            "field-sync-reserved-1",
            diff_sha,
            _auth(),
            _as_session(session),
        )
    assert reserved_exc.value.status_code == 422

    # 2. Stale revision is rejected with 409.
    stale_content_rev = "d" * 64
    stale_preview = integration_hub.preview_integration_layer_sync(
        org,
        project,
        "qgis",
        layer_id,
        _upload(edited_gpkg),
        stale_content_rev,
        _SCHEMA_REV_B,
        _auth(),
        _as_session(session),
    )
    assert stale_preview["stale_revision"] is True
    assert stale_preview["can_commit"] is False
    with pytest.raises(HTTPException) as stale_exc:
        integration_hub.push_integration_layer_sync(
            org,
            project,
            "qgis",
            layer_id,
            _upload(edited_gpkg),
            stale_content_rev,
            _SCHEMA_REV_B,
            "qgis-op-stale",
            str(stale_preview["diff_sha256"]),
            _auth(),
            _as_session(session),
        )
    assert stale_exc.value.status_code == 409

    # 3. Mismatched expected_diff_sha256 is rejected with 409.
    with pytest.raises(HTTPException) as mismatch_exc:
        integration_hub.push_integration_layer_sync(
            org,
            project,
            "qgis",
            layer_id,
            _upload(edited_gpkg),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            "qgis-op-mismatch",
            "0" * 64,
            _auth(),
            _as_session(session),
        )
    assert mismatch_exc.value.status_code == 409

    # 4. Empty diff (no creates or updates, even if features are omitted) is rejected with 409.
    unchanged_gpkg = _build_sync_gpkg(
        crs_code=5899,
        features=[
            (
                "HVS-QN1",
                {"fld_code": "QN-01", "fld_rank": 1, "fld_active": True},
                NativeGeometry("Point", (438442.69, 2291039.83), 5899),
            )
        ],
    )
    empty_preview = integration_hub.preview_integration_layer_sync(
        org,
        project,
        "qgis",
        layer_id,
        _upload(unchanged_gpkg),
        _CONTENT_REV_A,
        _SCHEMA_REV_B,
        _auth(),
        _as_session(session),
    )
    assert empty_preview["create_count"] == 0
    assert empty_preview["update_count"] == 0
    assert empty_preview["omitted_server_feature_count"] == 2
    assert empty_preview["can_commit"] is False
    with pytest.raises(HTTPException) as empty_exc:
        integration_hub.push_integration_layer_sync(
            org,
            project,
            "qgis",
            layer_id,
            _upload(unchanged_gpkg),
            _CONTENT_REV_A,
            _SCHEMA_REV_B,
            "qgis-op-empty",
            str(empty_preview["diff_sha256"]),
            _auth(),
            _as_session(session),
        )
    assert empty_exc.value.status_code == 409

    # 5. Valid commit delegates cleanly to _commit_sync_changeset (zero DELETE ops).
    changeset_id = uuid4()
    audit_event_id = uuid4()
    committed_calls: list[dict[str, Any]] = []

    def fake_commit(_session: object, **kwargs: Any) -> ChangesetRecord:
        committed_calls.append(kwargs)
        ops: list[EditOperationInput] = list(kwargs["operations"])
        return ChangesetRecord(
            id=changeset_id,
            organization_id=org,
            project_id=project,
            source_id=kwargs["source_id"],
            actor_type="human",
            actor_id=uuid4(),
            actor_display_snapshot="test-user",
            operation_id=kwargs["operation_id"],
            base_content_revision=kwargs["base_content_revision"],
            resulting_content_revision=_RESULTING_REV_C,
            base_catalog_revision=1,
            resulting_catalog_revision=2,
            summary=kwargs["summary"],
            status="committed",
            operation_count=len(ops),
            audit_event_id=audit_event_id,
            reverted_from_changeset_id=None,
            created_at=datetime.now(UTC),
        )

    monkeypatch.setattr(integration_hub, "_commit_sync_changeset", fake_commit)

    pushed = integration_hub.push_integration_layer_sync(
        org,
        project,
        "qgis",
        layer_id,
        _upload(edited_gpkg),
        _CONTENT_REV_A,
        _SCHEMA_REV_B,
        "qgis-op-commit-1",
        diff_sha,
        _auth(),
        _as_session(session),
        summary="Synced from QGIS",
    )

    assert len(committed_calls) == 1
    committed_ops: list[EditOperationInput] = list(committed_calls[0]["operations"])
    assert len(committed_ops) == 2
    assert {op.op_type for op in committed_ops} == {EditOpType.CREATE, EditOpType.UPDATE}
    assert all(op.op_type is not EditOpType.DELETE for op in committed_ops)
    assert {op.feature_id for op in committed_ops} == {"HVS-QN1", "HVS-QN4"}

    assert pushed["changeset_id"] == str(changeset_id)
    assert pushed["source_id"] == str(source_id)
    assert pushed["layer_id"] == str(layer_id)
    assert pushed["operation_id"] == "qgis-op-commit-1"
    assert pushed["resulting_content_revision"] == _RESULTING_REV_C
    assert pushed["operation_count"] == 2
    assert pushed["create_count"] == 1
    assert pushed["update_count"] == 1
    assert pushed["omitted_server_feature_count"] == 1
    assert pushed["deletions_blocked"] is True
    assert pushed["diff_sha256"] == diff_sha
    assert pushed["audit_event_id"] == str(audit_event_id)
