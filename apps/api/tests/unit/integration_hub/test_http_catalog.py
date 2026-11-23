"""HTTP handler security invariants without claiming real DB/RLS coverage."""

from __future__ import annotations

from types import SimpleNamespace
from uuid import uuid4

import pytest
from fastapi import HTTPException

from openvietgis_api.adapters.http import integration_hub
from openvietgis_api.domains.authorization.context import Actor
from openvietgis_api.domains.authorization.permissions import Permission


def _auth() -> SimpleNamespace:
    return SimpleNamespace(
        actor=Actor("human", uuid4(), "test-user"),
        credential_id=None,
    )


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


class _FakeLayerSession:
    def __init__(self, rows: list[dict[str, object]]) -> None:
        self._rows = rows
        self.execute_calls = 0

    def execute(self, _sql: object, _params: dict[str, object]) -> _FakeMappingsResult:
        self.execute_calls += 1
        return _FakeMappingsResult(self._rows)


class _ForbiddenQuerySession:
    def execute(self, _sql: object, _params: dict[str, object]) -> object:
        pytest.fail("must not query spatial layers for non-profile-enabled adapters")


def test_handler_denies_before_catalog(monkeypatch: pytest.MonkeyPatch) -> None:
    org, project = uuid4(), uuid4()
    calls: list[tuple[object, Permission]] = []

    def deny(_session: object, context: object, *, permission: Permission) -> None:
        calls.append((context, permission))
        raise PermissionError("project denied")

    monkeypatch.setattr(integration_hub, "prepare_project_scope", deny)
    monkeypatch.setattr(
        integration_hub,
        "list_adapters",
        lambda _permissions: pytest.fail("must not return catalog before authorization"),
    )
    with pytest.raises(PermissionError, match="project denied"):
        integration_hub.read_integration_catalog(org, project, _auth(), object())
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
    payload = integration_hub.read_integration_catalog(org, project, _auth(), object())
    assert len(seen) == 1
    assert payload["organization_id"] == str(org)
    assert payload["project_id"] == str(project)
    assert payload["scoped_access"] is scoped_flag
    assert payload["default_write_policy"] == "deny"
    assert len(payload["adapters"]) >= 10
    assert all(
        not operation["allowed"]
        for item in payload["adapters"]
        for operation in item["actions"]
        if operation["action"] != "catalog.discover"
    )


def test_router_exposes_only_get_no_mutation_endpoint() -> None:
    routes = [(route.path, set(route.methods)) for route in integration_hub.router.routes]
    assert len(routes) == 2
    expected_suffixes = {
        "/integration-hub/catalog",
        "/integration-hub/adapters/{adapter_key}",
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
    calls: list[tuple[object, Permission]] = []

    def deny(_session: object, context: object, *, permission: Permission) -> None:
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
            org, project, "nonexistent", _auth(), object()
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
            org, project, "unknown_vendor_adapter", _auth(), _ForbiddenQuerySession()
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
        org, project, adapter_key, _auth(), session
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
        org, project, adapter_key, _auth(), _ForbiddenQuerySession()
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
        org, project, "microstation_v8i", _auth(), _ForbiddenQuerySession()
    )
    assert payload["execution_status"] == "candidate_external_branch"
    assert payload["connection_profile"] is None
    assert payload["layers"] == []
    assert payload["default_write_policy"] == "deny"
