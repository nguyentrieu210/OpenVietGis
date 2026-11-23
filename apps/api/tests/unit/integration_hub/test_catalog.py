"""Invariant tests for the public adapter inventory and its deny policy."""

from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.integration_hub.catalog import (
    ADAPTERS,
    AdapterLifecycle,
    ConnectorAction,
    evaluate_action,
    list_adapters,
)


def test_all_keys_unique_and_every_adapter_has_status() -> None:
    assert len(ADAPTERS) >= 10
    assert len({a.key for a in ADAPTERS}) == len(ADAPTERS)
    assert all(a.lifecycle in AdapterLifecycle for a in ADAPTERS)


def test_guest_cannot_browse_catalog() -> None:
    first = ADAPTERS[0]
    assert not evaluate_action(first, ConnectorAction.DISCOVER, frozenset())["allowed"]


def test_project_reader_sees_catalog_only_and_never_syncs() -> None:
    permissions = frozenset({Permission.PROJECT_READ})
    catalog = list_adapters(permissions)
    assert all(row["actions"][0]["allowed"] for row in catalog)
    assert all(
        not operation["allowed"]
        for row in catalog
        for operation in row["actions"]
        if operation["action"] != ConnectorAction.DISCOVER.value
    )


def test_manager_cannot_invoke_unimplemented_write_or_sync() -> None:
    permissions = frozenset(Permission)
    for adapter in ADAPTERS:
        for action in (ConnectorAction.IMPORT, ConnectorAction.SYNC_PUSH, ConnectorAction.APPROVE):
            result = evaluate_action(adapter, action, permissions)
            assert result["allowed"] is False
            assert result["executable_in_hub"] is False


def test_service_credential_ceiling_is_reflected_by_permissions() -> None:
    permissions = frozenset({Permission.PROJECT_READ})
    adapter = next(a for a in ADAPTERS if a.key == "microstation_v8i")
    denied = evaluate_action(adapter, ConnectorAction.IMPORT, permissions)
    assert denied["reason"] == "permission_denied"
    assert denied["eligible_by_role"] is False


def test_existing_ogc_api_is_not_misrepresented_as_hub_sync() -> None:
    ogc = next(a for a in ADAPTERS if a.key == "ogc_services")
    assert ogc.lifecycle is AdapterLifecycle.EXISTING_READ_ONLY
    assert ogc.existing_api == "external-sources"
    assert ConnectorAction.SYNC_PUSH not in ogc.hub_executable_actions
