"""HTTP handler security invariants without claiming real DB/RLS coverage."""

from types import SimpleNamespace
from uuid import uuid4

import pytest

from openvietgis_api.adapters.http import integration_hub
from openvietgis_api.domains.authorization.context import Actor
from openvietgis_api.domains.authorization.permissions import Permission


def _auth() -> SimpleNamespace:
    return SimpleNamespace(
        actor=Actor("human", uuid4(), "test-user"),
        credential_id=None,
    )


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


def test_authorized_reader_receives_read_only_inventory(monkeypatch: pytest.MonkeyPatch) -> None:
    org, project = uuid4(), uuid4()
    seen: list[object] = []

    def prepare(_session: object, context: object, *, permission: Permission) -> None:
        assert permission is Permission.PROJECT_READ
        seen.append(context)

    class ReaderProvider:
        def __init__(self, _session: object) -> None:
            pass

        def resolve(self, _context: object) -> frozenset[Permission]:
            return frozenset({Permission.PROJECT_READ})

    monkeypatch.setattr(integration_hub, "prepare_project_scope", prepare)
    monkeypatch.setattr(integration_hub, "SqlEffectivePermissionProvider", ReaderProvider)
    payload = integration_hub.read_integration_catalog(org, project, _auth(), object())
    assert len(seen) == 1
    assert payload["organization_id"] == str(org)
    assert payload["project_id"] == str(project)
    assert payload["default_write_policy"] == "deny"
    assert len(payload["adapters"]) >= 10
    assert all(
        not operation["allowed"]
        for item in payload["adapters"]
        for operation in item["actions"]
        if operation["action"] != "catalog.discover"
    )


def test_router_exposes_only_get_no_mutation_endpoint() -> None:
    paths = [(route.path, route.methods) for route in integration_hub.router.routes]
    assert len(paths) == 1
    assert paths[0][0].endswith("/integration-hub/catalog")
    assert paths[0][1] == {"GET"}
