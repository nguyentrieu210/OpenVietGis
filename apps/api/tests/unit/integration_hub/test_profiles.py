"""Pure domain tests for read-only integration hub adapter profiles."""

from __future__ import annotations

import pytest

from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.integration_hub.catalog import ADAPTERS, get_adapter
from openvietgis_api.domains.integration_hub.profiles import (
    LayerAccessSnapshot,
    build_adapter_profile,
    execution_status_for,
)

_FORBIDDEN_CREDENTIAL_KEYS = {
    "password",
    "passwd",
    "secret",
    "token",
    "db_host",
    "host",
    "hostname",
    "port",
    "dsn",
    "connection_string",
    "username",
    "db_user",
    "pgpassword",
    "database_url",
}

_FORBIDDEN_VALUE_SUBSTRINGS = (
    "postgresql://",
    "postgres://",
    "password=",
    "pgpassword",
)

_WRITE_OR_SYNC_ACTIONS = {
    "connection.configure",
    "data.import",
    "sync.pull",
    "sync.push",
    "sync.approve",
}


def _layer(
    *,
    name: str,
    source_crs_code: int | None,
    stored_crs_code: int | None,
    validation_state: str = "ready",
    export_permitted: bool = True,
) -> LayerAccessSnapshot:
    return LayerAccessSnapshot(
        layer_id=f"layer-{name}",
        source_id=f"source-{name}",
        name=name,
        geometry_family="Point",
        dimensions="XY",
        validation_state=validation_state,
        source_crs_code=source_crs_code,
        stored_crs_code=stored_crs_code,
        layer_revision=1,
        content_revision="content-rev-1",
        schema_revision="schema-rev-1",
        export_permitted=export_permitted,
    )


def _walk_nested(obj: object) -> list[tuple[str | None, object]]:
    found: list[tuple[str | None, object]] = []
    if isinstance(obj, dict):
        for key, value in obj.items():
            found.append((str(key), value))
            found.extend(_walk_nested(value))
    elif isinstance(obj, list | tuple):
        for item in obj:
            found.append((None, item))
            found.extend(_walk_nested(item))
    return found


@pytest.mark.parametrize("epsg_code", [4326, 3857, 4756, 5899])
def test_explicit_crs_preserved_without_coercion(epsg_code: int) -> None:
    qgis = get_adapter("qgis")
    assert qgis is not None
    profile = build_adapter_profile(
        qgis,
        frozenset({Permission.PROJECT_READ}),
        organization_id="org-1",
        project_id="proj-1",
        scoped_access=False,
        layers=[
            _layer(
                name=f"epsg-{epsg_code}",
                source_crs_code=epsg_code,
                stored_crs_code=epsg_code,
                validation_state="ready",
                export_permitted=True,
            )
        ],
    )
    layers = profile["layers"]
    assert isinstance(layers, list)
    assert len(layers) == 1
    layer = layers[0]
    assert layer["source_crs_code"] == epsg_code
    assert layer["stored_crs_code"] == epsg_code
    assert layer["crs_explicit"] is True
    assert layer["export_permitted"] is True


@pytest.mark.parametrize(
    ("source_crs", "stored_crs", "validation_state"),
    [
        (None, None, "ready"),
        (4326, None, "ready"),
        (None, None, "missing_crs"),
        (5899, 5899, "invalid"),
        (4756, 4756, "pending"),
    ],
)
def test_missing_crs_or_unready_validation_never_infers_4326_and_blocks_export(
    source_crs: int | None,
    stored_crs: int | None,
    validation_state: str,
) -> None:
    postgis = get_adapter("postgis")
    assert postgis is not None
    profile = build_adapter_profile(
        postgis,
        frozenset({Permission.PROJECT_READ}),
        organization_id="org-1",
        project_id="proj-1",
        scoped_access=False,
        layers=[
            _layer(
                name="unready",
                source_crs_code=source_crs,
                stored_crs_code=stored_crs,
                validation_state=validation_state,
                export_permitted=True,
            )
        ],
    )
    layers = profile["layers"]
    assert isinstance(layers, list)
    assert len(layers) == 1
    layer = layers[0]
    assert layer["stored_crs_code"] == stored_crs
    if stored_crs is None:
        assert layer["stored_crs_code"] != 4326
    assert layer["crs_explicit"] is False
    assert layer["export_permitted"] is False


@pytest.mark.parametrize("definition_only_crs", [3405, 3406])
@pytest.mark.parametrize("adapter_key", ["qgis", "postgis"])
def test_definition_only_crs_blocks_explicit_crs_and_export(
    adapter_key: str,
    definition_only_crs: int,
) -> None:
    adapter = get_adapter(adapter_key)
    assert adapter is not None
    profile = build_adapter_profile(
        adapter,
        frozenset({Permission.PROJECT_READ}),
        organization_id="org-1",
        project_id="proj-1",
        scoped_access=False,
        layers=[
            _layer(
                name=f"def-only-{definition_only_crs}",
                source_crs_code=definition_only_crs,
                stored_crs_code=definition_only_crs,
                validation_state="ready",
                export_permitted=True,
            )
        ],
    )
    layers = profile["layers"]
    assert isinstance(layers, list)
    assert len(layers) == 1
    layer = layers[0]
    assert layer["stored_crs_code"] == definition_only_crs
    assert layer["crs_explicit"] is False
    assert layer["export_permitted"] is False


@pytest.mark.parametrize(
    ("adapter_key", "expected_formats"),
    [
        ("qgis", ["geopackage", "geojson", "postgis_sql"]),
        ("postgis", ["postgis_sql", "geopackage"]),
    ],
)
def test_qgis_and_postgis_connection_profiles_advertise_read_only_hub_export(
    adapter_key: str,
    expected_formats: list[str],
) -> None:
    adapter = get_adapter(adapter_key)
    assert adapter is not None
    profile = build_adapter_profile(
        adapter,
        frozenset({Permission.PROJECT_READ}),
        organization_id="org-1",
        project_id="proj-1",
        scoped_access=False,
        layers=[],
    )
    conn = profile["connection_profile"]
    assert isinstance(conn, dict)
    assert conn["supports_direct_db_credentials"] is False
    assert conn["default_write_policy"] == "deny"
    assert conn["export_formats"] == expected_formats

    endpoints = conn["existing_endpoints"]
    assert isinstance(endpoints, list)
    by_name = {ep["name"]: ep for ep in endpoints}
    assert "hub_layer_export" in by_name
    hub_export = by_name["hub_layer_export"]
    assert hub_export["method"] == "GET"
    assert hub_export["path_template"] == (
        f"/api/v1/organizations/org-1/projects/proj-1"
        f"/integration-hub/adapters/{adapter_key}/layers/{{layer_id}}/export"
    )
    assert hub_export["required_permission"] == Permission.PROJECT_READ.value


def test_connection_profiles_never_expose_db_credentials_or_hosts() -> None:
    full_permissions = frozenset(Permission)
    for adapter in ADAPTERS:
        profile = build_adapter_profile(
            adapter,
            full_permissions,
            organization_id="org-1",
            project_id="proj-1",
            scoped_access=False,
            layers=[],
        )
        conn = profile["connection_profile"]
        if conn is None:
            continue
        assert isinstance(conn, dict)
        assert conn["supports_direct_db_credentials"] is False
        assert conn["default_write_policy"] == "deny"
        assert conn["crs_policy"] == "explicit_only_never_inferred"

        for key, value in _walk_nested(conn):
            if key is not None:
                lowered = key.lower()
                assert lowered not in _FORBIDDEN_CREDENTIAL_KEYS, (adapter.key, key)
            if isinstance(value, str):
                lowered_val = value.lower()
                for forbidden in _FORBIDDEN_VALUE_SUBSTRINGS:
                    assert forbidden not in lowered_val, (adapter.key, key, value)


def test_all_write_and_sync_actions_remain_denied_even_for_full_permission_managers() -> None:
    full_permissions = frozenset(Permission)
    for adapter in ADAPTERS:
        profile = build_adapter_profile(
            adapter,
            full_permissions,
            organization_id="org-1",
            project_id="proj-1",
            scoped_access=False,
            layers=[],
        )
        assert profile["default_write_policy"] == "deny"
        serialized_adapter = profile["adapter"]
        assert isinstance(serialized_adapter, dict)
        actions = serialized_adapter["actions"]
        assert isinstance(actions, list)
        seen_write_actions = set()
        for action in actions:
            if action["action"] in _WRITE_OR_SYNC_ACTIONS:
                seen_write_actions.add(action["action"])
                assert action["allowed"] is False, (adapter.key, action)
                assert action["executable_in_hub"] is False, (adapter.key, action)
        assert seen_write_actions == _WRITE_OR_SYNC_ACTIONS


def test_execution_status_and_planned_profile_suppress_layers() -> None:
    sample_layer = _layer(
        name="vn2000",
        source_crs_code=5899,
        stored_crs_code=5899,
        validation_state="ready",
        export_permitted=True,
    )
    for adapter in ADAPTERS:
        status = execution_status_for(adapter)
        profile = build_adapter_profile(
            adapter,
            frozenset({Permission.PROJECT_READ}),
            organization_id="org-1",
            project_id="proj-1",
            scoped_access=False,
            layers=[sample_layer],
        )
        assert profile["execution_status"] == status
        if adapter.key == "ogc_services":
            assert status == "existing_read_only_api"
            assert profile["connection_profile"] is not None
        elif adapter.key in ("qgis", "postgis"):
            assert status == "read_only_profile_ready"
            assert profile["connection_profile"] is not None
        elif adapter.key == "microstation_v8i":
            assert status == "candidate_external_branch"
            assert profile["connection_profile"] is None
            assert profile["layers"] == []
        else:
            assert status == "planned_not_implemented"
            assert profile["connection_profile"] is None
            assert profile["layers"] == []
