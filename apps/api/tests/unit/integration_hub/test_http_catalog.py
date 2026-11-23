"""HTTP handler security invariants without claiming real DB/RLS coverage."""

from __future__ import annotations

import hashlib
import json
from types import SimpleNamespace
from typing import Any
from uuid import UUID, uuid4

import pytest
from fastapi import HTTPException
from fastapi.routing import APIRoute

from openvietgis_api.adapters.formats.geopackage import (
    inspect_geopackage,
    parse_geopackage_member,
)
from openvietgis_api.adapters.http import integration_hub
from openvietgis_api.domains.authorization.context import Actor
from openvietgis_api.domains.authorization.permissions import Permission


def _auth() -> Any:
    return SimpleNamespace(
        actor=Actor("human", uuid4(), "test-user"),
        credential_id=None,
    )


def _as_session(obj: object) -> Any:
    return obj


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
        "content_revision": "content-rev-5899",
        "schema_revision": "schema-rev-5899",
    }


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
        if item["key"] in ("qgis", "postgis"):
            assert allowed_actions == {"catalog.discover", "data.export"}
            export_op = next(op for op in item["actions"] if op["action"] == "data.export")
            assert export_op["reason"] == "read_only_export"
        else:
            assert allowed_actions == {"catalog.discover"}


def test_router_exposes_only_three_get_routes_and_no_mutation_endpoints() -> None:
    api_routes = [route for route in integration_hub.router.routes if isinstance(route, APIRoute)]
    assert len(api_routes) == len(integration_hub.router.routes) == 3
    routes = [(route.path, set(route.methods or ())) for route in api_routes]
    expected_suffixes = {
        "/integration-hub/catalog",
        "/integration-hub/adapters/{adapter_key}",
        "/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export",
    }
    actual_suffixes = {
        path.split("/projects/{project_id}", 1)[1] for path, _methods in routes
    }
    assert actual_suffixes == expected_suffixes
    forbidden_methods = {"POST", "PUT", "PATCH", "DELETE"}
    for _path, methods in routes:
        assert methods == {"GET"}
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


@pytest.mark.parametrize("adapter_key", ["qgis", "postgis"])
def test_adapter_profile_qgis_and_postgis_enforces_scoped_acl_and_crs(
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


@pytest.mark.parametrize("adapter_key", ["autocad", "arcgis"])
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


@pytest.mark.parametrize("adapter_key", ["autocad", "ogc_services", "microstation_v8i"])
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
