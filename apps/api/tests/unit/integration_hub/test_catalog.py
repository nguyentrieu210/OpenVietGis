"""Invariant tests for the public adapter inventory and its fail-closed policy."""

from __future__ import annotations

import pytest

from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.integration_hub.catalog import (
    ADAPTERS,
    AdapterLifecycle,
    ConnectorAction,
    evaluate_action,
    get_adapter,
    list_adapters,
)

_EXPORT_ENABLED_KEYS = frozenset({"qgis", "postgis", "autocad"})
_READ_ONLY_EXPORT_ONLY_KEYS = frozenset({"postgis", "autocad"})
_ALWAYS_DENIED_ACTIONS = (
    ConnectorAction.CONFIGURE,
    ConnectorAction.IMPORT,
    ConnectorAction.SYNC_PULL,
    ConnectorAction.APPROVE,
)


def test_all_keys_unique_and_every_adapter_has_status() -> None:
    assert len(ADAPTERS) == 12
    assert len({a.key for a in ADAPTERS}) == len(ADAPTERS)
    assert all(a.lifecycle in AdapterLifecycle for a in ADAPTERS)


def test_guest_cannot_browse_catalog() -> None:
    for adapter in ADAPTERS:
        assert not evaluate_action(adapter, ConnectorAction.DISCOVER, frozenset())["allowed"]
        assert not evaluate_action(adapter, ConnectorAction.EXPORT, frozenset())["allowed"]
        assert not evaluate_action(adapter, ConnectorAction.SYNC_PUSH, frozenset())["allowed"]


def test_qgis_allows_discover_export_and_edit_gated_sync_push() -> None:
    adapter = get_adapter("qgis")
    assert adapter is not None
    assert adapter.lifecycle is AdapterLifecycle.EXISTING_READ_ONLY
    assert adapter.hub_executable_actions == (
        ConnectorAction.DISCOVER,
        ConnectorAction.EXPORT,
        ConnectorAction.SYNC_PUSH,
    )

    read_only = frozenset({Permission.PROJECT_READ})
    discover = evaluate_action(adapter, ConnectorAction.DISCOVER, read_only)
    assert discover["allowed"] is True
    assert discover["executable_in_hub"] is True
    assert discover["reason"] == "catalog_only"

    export = evaluate_action(adapter, ConnectorAction.EXPORT, read_only)
    assert export["allowed"] is True
    assert export["executable_in_hub"] is True
    assert export["eligible_by_role"] is True
    assert export["required_permission"] == Permission.PROJECT_READ.value
    assert export["reason"] == "read_only_export"

    # Viewer with PROJECT_READ only is denied SYNC_PUSH.
    sync_push_reader = evaluate_action(adapter, ConnectorAction.SYNC_PUSH, read_only)
    assert sync_push_reader["allowed"] is False
    assert sync_push_reader["eligible_by_role"] is False
    assert sync_push_reader["executable_in_hub"] is True
    assert sync_push_reader["required_permission"] == Permission.PROJECT_EDIT.value
    assert sync_push_reader["reason"] == "permission_denied"

    # Editor with PROJECT_EDIT is allowed SYNC_PUSH via controlled changeset sync.
    editor = frozenset({Permission.PROJECT_READ, Permission.PROJECT_EDIT})
    sync_push_editor = evaluate_action(adapter, ConnectorAction.SYNC_PUSH, editor)
    assert sync_push_editor["allowed"] is True
    assert sync_push_editor["eligible_by_role"] is True
    assert sync_push_editor["executable_in_hub"] is True
    assert sync_push_editor["required_permission"] == Permission.PROJECT_EDIT.value
    assert sync_push_editor["reason"] == "controlled_changeset_sync"


@pytest.mark.parametrize("adapter_key", ["autocad", "postgis"])
def test_autocad_and_postgis_allow_discover_and_export_only_and_deny_sync_push(
    adapter_key: str,
) -> None:
    adapter = get_adapter(adapter_key)
    assert adapter is not None
    assert adapter.lifecycle is AdapterLifecycle.EXISTING_READ_ONLY
    assert adapter.hub_executable_actions == (
        ConnectorAction.DISCOVER,
        ConnectorAction.EXPORT,
    )

    read_only = frozenset({Permission.PROJECT_READ})
    discover = evaluate_action(adapter, ConnectorAction.DISCOVER, read_only)
    assert discover["allowed"] is True
    assert discover["executable_in_hub"] is True
    assert discover["reason"] == "catalog_only"

    export = evaluate_action(adapter, ConnectorAction.EXPORT, read_only)
    assert export["allowed"] is True
    assert export["executable_in_hub"] is True
    assert export["eligible_by_role"] is True
    assert export["required_permission"] == Permission.PROJECT_READ.value
    assert export["reason"] == "read_only_export"

    # Even with full permissions, SYNC_PUSH stays denied on autocad and postgis.
    full_permissions = frozenset(Permission)
    sync_push = evaluate_action(adapter, ConnectorAction.SYNC_PUSH, full_permissions)
    assert sync_push["allowed"] is False
    assert sync_push["executable_in_hub"] is False
    assert ConnectorAction.SYNC_PUSH not in adapter.hub_executable_actions

    allowed_with_full = {
        action
        for action in ConnectorAction
        if evaluate_action(adapter, action, full_permissions)["allowed"]
    }
    assert allowed_with_full == {ConnectorAction.DISCOVER, ConnectorAction.EXPORT}


def test_all_other_nine_adapters_only_allow_discover_even_with_full_permissions() -> None:
    full_permissions = frozenset(Permission)
    other_adapters = [a for a in ADAPTERS if a.key not in _EXPORT_ENABLED_KEYS]
    assert len(other_adapters) == 9

    for adapter in other_adapters:
        assert adapter.hub_executable_actions == (ConnectorAction.DISCOVER,)
        discover = evaluate_action(adapter, ConnectorAction.DISCOVER, full_permissions)
        assert discover["allowed"] is True
        assert discover["reason"] == "catalog_only"
        for action in ConnectorAction:
            if action is ConnectorAction.DISCOVER:
                continue
            result = evaluate_action(adapter, action, full_permissions)
            assert result["allowed"] is False, (adapter.key, action)
            assert result["executable_in_hub"] is False, (adapter.key, action)


def test_project_reader_catalog_matches_read_only_export_and_discover_policy() -> None:
    permissions = frozenset({Permission.PROJECT_READ})
    catalog = list_adapters(permissions)
    assert len(catalog) == 12
    for row in catalog:
        key = row["key"]
        raw_actions = row["actions"]
        assert isinstance(raw_actions, list)
        actions = {op["action"]: op for op in raw_actions}
        assert actions[ConnectorAction.DISCOVER.value]["allowed"] is True
        assert actions[ConnectorAction.SYNC_PUSH.value]["allowed"] is False
        if key in _EXPORT_ENABLED_KEYS:
            assert actions[ConnectorAction.EXPORT.value]["allowed"] is True
            assert actions[ConnectorAction.EXPORT.value]["reason"] == "read_only_export"
            expected_allowed = {ConnectorAction.DISCOVER.value, ConnectorAction.EXPORT.value}
        else:
            assert actions[ConnectorAction.EXPORT.value]["allowed"] is False
            expected_allowed = {ConnectorAction.DISCOVER.value}
        actual_allowed = {name for name, op in actions.items() if op["allowed"]}
        assert actual_allowed == expected_allowed


def test_configure_import_sync_pull_approve_denied_across_all_12_adapters() -> None:
    permissions = frozenset(Permission)
    assert len(ADAPTERS) == 12
    for adapter in ADAPTERS:
        for action in _ALWAYS_DENIED_ACTIONS:
            result = evaluate_action(adapter, action, permissions)
            assert result["allowed"] is False, (adapter.key, action)
            assert result["executable_in_hub"] is False, (adapter.key, action)
        if adapter.key != "qgis":
            sync_push = evaluate_action(adapter, ConnectorAction.SYNC_PUSH, permissions)
            assert sync_push["allowed"] is False, adapter.key
            assert sync_push["executable_in_hub"] is False, adapter.key


def test_get_adapter_returns_manifest_or_none() -> None:
    qgis = get_adapter("qgis")
    assert qgis is not None
    assert qgis.key == "qgis"
    assert get_adapter("unknown_adapter") is None


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
    assert ogc.hub_executable_actions == (ConnectorAction.DISCOVER,)
    assert ConnectorAction.SYNC_PUSH not in ogc.hub_executable_actions
    assert _READ_ONLY_EXPORT_ONLY_KEYS == frozenset({"postgis", "autocad"})
