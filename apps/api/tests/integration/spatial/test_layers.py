"""US2 same-source Layer identity, lifecycle and concurrency acceptance.

All product actions run through actual R002 session/CSRF and the restricted
runtime role. Owner connections only seed fixtures, inspect committed rows or
observe a genuine database lock wait; they never create or retire a Layer.
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from threading import Event
from time import monotonic, sleep
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.domains.spatial.domain import CatalogOperation
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.spatial.test_sources import (
    _changes,
    _client,
    _error,
    _register,
    _rows,
)
from tests.integration.spatial.test_sources import (
    _get as _get_source,
)
from tests.integration.spatial.test_sources import (
    _path as _source_path,
)


def _path(world: AccessWorld, project: UUID | None = None) -> str:
    return (
        f"/api/v1/organizations/{world.organization_id}/projects/"
        f"{project or world.project_id}/layers"
    )


def _mutated(
    response,
    world: AccessWorld,
    *,
    status: int,
    prior: int | None,
    revision: int,
    lifecycle: str = "active",
):
    assert response.status_code == status, response.text
    result = response.json()
    assert set(result) == {
        "organization_id",
        "project_id",
        "resource_type",
        "resource_id",
        "change_id",
        "prior_revision",
        "new_revision",
        "lifecycle",
    }
    assert result["organization_id"] == str(world.organization_id)
    assert result["project_id"] == str(world.project_id)
    assert result["resource_type"] == "layer"
    assert result["prior_revision"] == ({"layer_revision": prior} if prior is not None else None)
    assert result["new_revision"] == {"layer_revision": revision}
    assert result["lifecycle"] == lifecycle
    UUID(result["resource_id"])
    UUID(result["change_id"])
    return result


def _create(client: TestClient, world: AccessWorld, source_id: str, name: str):
    return _mutated(
        client.post(_path(world), json={"source_id": source_id, "name": name}),
        world,
        status=201,
        prior=None,
        revision=1,
    )


def _detail(client: TestClient, path: str, layer_id: str):
    response = client.get(f"{path}/{layer_id}")
    assert response.status_code == 200, response.text
    return response.json()


def _capabilities(layer) -> None:
    expected = {
        "metadata_read",
        "feature_read",
        "render",
        "edit",
        "style",
        "export",
        "analysis",
        "offline",
        "query",
    }
    assert len(layer["capabilities"]) == len(expected)
    capabilities = {item["name"]: item for item in layer["capabilities"]}
    assert set(capabilities) == expected
    for name, item in capabilities.items():
        supported = name in {
            "metadata_read",
            "feature_read",
            "render",
            "style",
            "export",
            "query",
            "analysis",
            "offline",
        }
        req_perm = "project.edit" if name == "analysis" else ("project.read" if supported else None)
        assert item == {
            "name": name,
            "technical_supported": supported,
            "available": supported,
            "required_permission": req_perm,
            "actor_authorized": supported,
        }


def _layers(world: AccessWorld):
    with get_admin_engine().begin() as connection:
        return (
            connection.execute(
                text("""
                SELECT id,organization_id,project_id,source_id,name,lifecycle,
                       layer_revision,created_at,updated_at,retired_at
                FROM spatial.layers WHERE organization_id=:org AND project_id=:project
                ORDER BY id
            """),
                {"org": world.organization_id, "project": world.project_id},
            )
            .mappings()
            .all()
        )


def _counts(world: AccessWorld):
    with get_admin_engine().begin() as connection:
        return tuple(
            connection.execute(
                text("""
                SELECT
                  (SELECT count(*) FROM spatial.data_sources WHERE project_id=:project),
                  (SELECT count(*) FROM spatial.source_fields WHERE project_id=:project),
                  (SELECT count(*) FROM spatial.reference_features WHERE project_id=:project),
                  (SELECT count(*) FROM spatial.layers WHERE project_id=:project),
                  (SELECT count(*) FROM spatial.catalog_changes WHERE project_id=:project),
                  (SELECT count(*) FROM audit.audit_events WHERE organization_id=:org
                     AND resource_type IN ('data_source','layer') AND outcome='success')
            """),
                {"org": world.organization_id, "project": world.project_id},
            ).one()
        )


def _layer_change(world: AccessWorld, mutation, operation: str, expected) -> None:
    assert _changes(world, mutation, operation) == expected
    with get_admin_engine().begin() as connection:
        row = (
            connection.execute(
                text("""
                SELECT c.resource_type,c.layer_id,c.source_id,c.actor_type,c.created_at,
                       c.actor_display_snapshot,a.created_at AS audit_time,
                       a.resource_id AS audit_id,
                       a.resource_type AS audit_type
                FROM spatial.catalog_changes c
                JOIN audit.audit_events a
                  ON a.id=c.audit_event_id AND a.organization_id=c.organization_id
                WHERE c.id=:change AND c.organization_id=:org AND c.project_id=:project
            """),
                {
                    "change": mutation["change_id"],
                    "org": world.organization_id,
                    "project": world.project_id,
                },
            )
            .mappings()
            .one()
        )
    assert row["resource_type"] == row["audit_type"] == "layer"
    assert row["layer_id"] == row["audit_id"] == UUID(mutation["resource_id"])
    assert row["source_id"] is None
    assert row["actor_type"] == "human"
    assert row["actor_display_snapshot"] == "Owner"
    assert row["created_at"] == row["audit_time"]


def test_two_layers_share_one_source_and_retirement_is_logical(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    with get_engine().connect() as connection:
        assert tuple(
            connection.execute(
                text("""
            SELECT current_user,rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user
        """)
            ).one()
        ) == ("openvietgis_runtime", False, False)
    layer_path, source_path = _path(world), _source_path(world)
    with _client(world.owner_user_id) as client:
        registered = _register(client, source_path, "reference_points_5899", "Independent source")
        source_id = registered["resource_id"]
        source = _get_source(client, source_path, source_id)
        native = _rows(source_id)
        assert len(native) == source["feature_count"] == 4
        assert source["stored_crs"]["code"] == 5899
        assert _counts(world) == (1, 7, 4, 0, 1, 1)

        # Duplicate display names are intentionally permitted: identity is not a name.
        first = _create(client, world, source_id, "  Same display name  ")
        second = _create(client, world, source_id, "Same display name")
        first_id, second_id = first["resource_id"], second["resource_id"]
        assert first_id != second_id
        assert _counts(world) == (1, 7, 4, 2, 3, 3)
        assert _rows(source_id) == native
        for mutation in (first, second):
            _layer_change(
                world,
                mutation,
                "create_layer",
                {
                    "name": (None, "Same display name"),
                    "source_id": (None, source_id),
                    "lifecycle": (None, "active"),
                    "layer_revision": (None, 1),
                },
            )
            layer = _detail(client, layer_path, mutation["resource_id"])
            assert layer["id"] == mutation["resource_id"]
            assert layer["organization_id"] == str(world.organization_id)
            assert layer["project_id"] == str(world.project_id)
            assert layer["name"] == "Same display name" and layer["lifecycle"] == "active"
            assert layer["layer_revision"] == 1 and layer["retired_at"] is None
            assert layer["source_id"] == source_id and layer["source"] == source
            assert layer["source"]["name"] != layer["name"]
            _capabilities(layer)

        ordered = sorted([first_id, second_id])
        listed = client.get(layer_path, params={"limit": 1})
        assert listed.status_code == 200, listed.text
        page = listed.json()
        assert page["organization_id"] == str(world.organization_id)
        assert page["project_id"] == str(world.project_id)
        assert [item["id"] for item in page["items"]] == ordered[:1]
        assert page["next_after_id"] == ordered[0]
        last = client.get(layer_path, params={"limit": 1, "after_id": page["next_after_id"]})
        assert last.status_code == 200, last.text
        assert [item["id"] for item in last.json()["items"]] == ordered[1:]
        assert last.json()["next_after_id"] is None

        # Source metadata is current, not copied into each Layer at creation.
        renamed_source = client.patch(
            f"{source_path}/{source_id}", json={"expected_revision": 1, "name": "Renamed source"}
        )
        assert renamed_source.status_code == 200
        current_source = _get_source(client, source_path, source_id)
        assert current_source["name"] == "Renamed source"
        assert current_source["catalog_revision"] == 2
        assert current_source["content_revision"] == source["content_revision"]
        assert current_source["schema_revision"] == source["schema_revision"]
        for layer_id in (first_id, second_id):
            layer = _detail(client, layer_path, layer_id)
            assert layer["source"] == current_source
            assert layer["name"] == "Same display name" and layer["layer_revision"] == 1
        second_before = _detail(client, layer_path, second_id)
        source_retire = f"{source_path}/{source_id}/retire"
        _error(
            client.post(source_retire, json={"expected_revision": 2}),
            409,
            "ACTIVE_LAYER_DEPENDENCIES",
        )
        assert _counts(world) == (1, 7, 4, 2, 4, 4)

        retired = _mutated(
            client.post(f"{layer_path}/{first_id}/retire", json={"expected_revision": 1}),
            world,
            status=200,
            prior=1,
            revision=2,
            lifecycle="retired",
        )
        assert retired["resource_id"] == first_id
        _layer_change(
            world,
            retired,
            "retire_layer",
            {
                "lifecycle": ("active", "retired"),
                "layer_revision": (1, 2),
            },
        )
        assert client.get(f"{layer_path}/{first_id}").status_code == 404
        assert client.get(layer_path).json()["items"] == [second_before]
        assert _detail(client, layer_path, second_id) == second_before
        assert _get_source(client, source_path, source_id) == current_source
        _error(
            client.post(source_retire, json={"expected_revision": 2}),
            409,
            "ACTIVE_LAYER_DEPENDENCIES",
        )

        retired_second = _mutated(
            client.post(f"{layer_path}/{second_id}/retire", json={"expected_revision": 1}),
            world,
            status=200,
            prior=1,
            revision=2,
            lifecycle="retired",
        )
        _layer_change(
            world,
            retired_second,
            "retire_layer",
            {
                "lifecycle": ("active", "retired"),
                "layer_revision": (1, 2),
            },
        )
        source_done = client.post(source_retire, json={"expected_revision": 2})
        assert source_done.status_code == 200, source_done.text
        assert source_done.json()["new_revision"] == {
            **renamed_source.json()["new_revision"],
            "catalog_revision": 3,
        }
        assert client.get(f"{source_path}/{source_id}").status_code == 404
        assert client.get(layer_path).json()["items"] == []
    # Soft retirement preserves rows, native attributes, geometry and feature pins.
    assert _counts(world) == (1, 7, 4, 2, 7, 7)
    assert _rows(source_id) == native
    rows = _layers(world)
    assert {str(row["id"]) for row in rows} == {first_id, second_id}
    assert all(
        str(row["source_id"]) == source_id
        and row["lifecycle"] == "retired"
        and row["layer_revision"] == 2
        and row["retired_at"] is not None
        for row in rows
    )


def test_layer_rename_cas_stale_and_retired_errors_are_atomic(migrated_database: None) -> None:
    world = seed_access_world()
    layer_path, source_path = _path(world), _source_path(world)
    with _client(world.owner_user_id) as client:
        source_id = _register(client, source_path, "reference_empty_4326")["resource_id"]
        source = _get_source(client, source_path, source_id)
        created = _create(client, world, source_id, "Original")
        layer_id = created["resource_id"]
        path = f"{layer_path}/{layer_id}"
        prior = _detail(client, layer_path, layer_id)
        before = _counts(world)
        _error(
            client.patch(path, json={"expected_revision": 1, "name": " Original "}),
            422,
            "NO_CHANGES",
        )
        assert _detail(client, layer_path, layer_id) == prior
        assert _counts(world) == before
        renamed = _mutated(
            client.patch(path, json={"expected_revision": 1, "name": " Renamed "}),
            world,
            status=200,
            prior=1,
            revision=2,
        )
        assert renamed["resource_id"] == layer_id
        _layer_change(
            world,
            renamed,
            "rename_layer",
            {
                "name": ("Original", "Renamed"),
                "layer_revision": (1, 2),
            },
        )
        current = _detail(client, layer_path, layer_id)
        assert current["name"] == "Renamed" and current["layer_revision"] == 2
        assert current["created_at"] == prior["created_at"]
        assert current["source"] == source
        # A stale caller proposing the current name still receives the stale conflict.
        _error(
            client.patch(path, json={"expected_revision": 1, "name": "Renamed"}),
            409,
            "STALE_LAYER_REVISION",
        )
        _error(
            client.post(f"{path}/retire", json={"expected_revision": 1}),
            409,
            "STALE_LAYER_REVISION",
        )
        assert _detail(client, layer_path, layer_id) == current
        assert _counts(world) == (1, 7, 0, 1, 3, 3)
        retired = _mutated(
            client.post(f"{path}/retire", json={"expected_revision": 2}),
            world,
            status=200,
            prior=2,
            revision=3,
            lifecycle="retired",
        )
        _layer_change(
            world,
            retired,
            "retire_layer",
            {
                "lifecycle": ("active", "retired"),
                "layer_revision": (2, 3),
            },
        )
        absent = f"{layer_path}/{uuid4()}"
        for verb, suffix, payload in (
            (client.get, "", None),
            (client.patch, "", {"expected_revision": 3, "name": "No restore"}),
            (client.post, "/retire", {"expected_revision": 3}),
        ):
            kwargs = {"json": payload} if payload is not None else {}
            actual = verb(path + suffix, **kwargs)
            missing = verb(absent + suffix, **kwargs)
            assert actual.status_code == missing.status_code == 404
            assert actual.json() == missing.json()
        assert _get_source(client, source_path, source_id) == source
    assert _counts(world) == (1, 7, 0, 1, 4, 4)


def test_nonready_retired_missing_and_dual_authorized_cross_project_source(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    sibling = uuid4()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
                INSERT INTO workspace.projects (
                    id,organization_id,workspace_id,name,slug,status,created_by_actor_type,
                    created_by_actor_id,created_at,updated_at
                ) VALUES (:id,:org,:workspace,'Sibling',:slug,'active','human',:actor,now(),now())
            """),
            {
                "id": sibling,
                "org": world.organization_id,
                "workspace": world.workspace_a_id,
                "slug": f"sibling-{sibling.hex}",
                "actor": world.owner_user_id,
            },
        )
    with _client(world.owner_user_id) as client:
        target, sibling_target = _path(world), _path(world, sibling)
        own = _register(client, _source_path(world), "reference_empty_4326")["resource_id"]
        foreign = _register(
            client,
            _source_path(world, sibling),
            "reference_empty_4326",
        )["resource_id"]
        assert (
            _get_source(client, _source_path(world, sibling), foreign)["validation_state"]
            == "ready"
        )
        # Prove both Project scopes are accessible before testing a cross-reference.
        assert client.get(target).status_code == client.get(sibling_target).status_code == 200
        good = _create(client, world, own, "An allowed layer")
        sibling_created = client.post(
            sibling_target, json={"source_id": foreign, "name": "An allowed layer"}
        )
        assert sibling_created.status_code == 201, sibling_created.text
        other_layer = sibling_created.json()["resource_id"]
        assert _detail(client, sibling_target, other_layer)["source_id"] == foreign
        missing = str(uuid4())
        bad = _register(client, _source_path(world), "reference_unknown_crs")["resource_id"]
        inactive = _register(client, _source_path(world), "reference_points_5899")["resource_id"]
        retired = client.post(
            f"{_source_path(world)}/{inactive}/retire", json={"expected_revision": 1}
        )
        assert retired.status_code == 200
        before = _counts(world), _layers(world)
        _error(
            client.post(target, json={"source_id": bad, "name": "Invalid"}), 409, "SOURCE_NOT_READY"
        )
        absent_response = client.post(target, json={"source_id": missing, "name": "Denied"})
        assert absent_response.status_code == 404
        for inaccessible in (foreign, inactive):
            denied = client.post(target, json={"source_id": inaccessible, "name": "Denied"})
            assert denied.status_code == 404 and denied.json() == absent_response.json()
        # Wrong-scope Layer IDs also fail without disclosing that they exist.
        absent_url, foreign_url = f"{target}/{uuid4()}", f"{target}/{other_layer}"
        for action, suffix, payload in (
            (client.get, "", None),
            (client.patch, "", {"expected_revision": 1, "name": "Denied"}),
            (client.post, "/retire", {"expected_revision": 1}),
        ):
            kwargs = {"json": payload} if payload is not None else {}
            actual = action(foreign_url + suffix, **kwargs)
            absent = action(absent_url + suffix, **kwargs)
            assert actual.status_code == absent.status_code == 404
            assert actual.json() == absent.json()
        assert (_counts(world), _layers(world)) == before
        assert client.get(target).json()["items"] == [_detail(client, target, good["resource_id"])]
        assert _detail(client, sibling_target, other_layer)["source_id"] == foreign


def _wait_for_blocked_runtime_query(timeout: float = 10.0) -> None:
    """Observe a real PostgreSQL lock wait, not just two scheduled Python calls."""
    deadline = monotonic() + timeout
    while monotonic() < deadline:
        with get_admin_engine().begin() as connection:
            blocked = connection.execute(
                text("""
                SELECT count(*) FROM pg_stat_activity
                WHERE datname=current_database() AND usename='openvietgis_runtime'
                  AND wait_event_type='Lock' AND state='active'
            """)
            ).scalar_one()
        if blocked:
            return
        sleep(0.05)
    pytest.fail("contending runtime request never reached a PostgreSQL lock wait")


@pytest.mark.parametrize("winner", ["create_layer", "retire_source"])
def test_concurrent_layer_create_and_source_retire_have_one_winner(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
    winner: str,
) -> None:
    world = seed_access_world()
    layer_path, source_path = _path(world), _source_path(world)
    with _client(world.owner_user_id) as setup:
        registration = _register(setup, source_path, "reference_points_5899")
        source_id = registration["resource_id"]
        native = _rows(source_id)
        assert setup.get(layer_path).status_code == 200
        assert setup.get(layer_path).json()["items"] == []

    # Pause only after the winning real mutation/evidence is written, before its
    # outer transaction commits. The opposing request must genuinely wait on a
    # database lock; after commit it re-evaluates the now-current state.
    recording = Event()
    release = Event()
    append = SqlSpatialRepository.append_change

    def pause_winner(self, change, audit, *, context):
        append(self, change, audit, context=context)
        if change.operation == CatalogOperation(winner) and change.project_id == world.project_id:
            recording.set()
            assert release.wait(20), "test did not release winning transaction"

    source_retire = f"{source_path}/{source_id}/retire"
    with (
        _client(world.owner_user_id) as create_client,
        _client(world.owner_user_id) as retire_client,
    ):

        def create():
            return create_client.post(layer_path, json={"source_id": source_id, "name": "Racing"})

        def retire():
            return retire_client.post(source_retire, json={"expected_revision": 1})

        with monkeypatch.context() as patch:
            patch.setattr(SqlSpatialRepository, "append_change", pause_winner)
            with ThreadPoolExecutor(max_workers=2) as pool:
                first = pool.submit(create if winner == "create_layer" else retire)
                try:
                    assert recording.wait(10), "winning mutation never reached evidence append"
                    second = pool.submit(retire if winner == "create_layer" else create)
                    _wait_for_blocked_runtime_query()
                    assert not second.done(), (
                        "contending transaction returned before winner committed"
                    )
                finally:
                    release.set()
                first_response, second_response = (
                    first.result(timeout=20),
                    second.result(timeout=20),
                )

    with _client(world.owner_user_id) as verify:
        if winner == "create_layer":
            created = _mutated(first_response, world, status=201, prior=None, revision=1)
            _error(second_response, 409, "ACTIVE_LAYER_DEPENDENCIES")
            layer = _detail(verify, layer_path, created["resource_id"])
            assert layer["source_id"] == source_id
            assert layer["source"]["lifecycle"] == "active"
            assert _get_source(verify, source_path, source_id)["catalog_revision"] == 1
            assert _counts(world) == (1, 7, 4, 1, 2, 2)
        else:
            assert first_response.status_code == 200, first_response.text
            assert first_response.json()["lifecycle"] == "retired"
            assert first_response.json()["new_revision"] == {
                **registration["new_revision"],
                "catalog_revision": 2,
            }
            absent = verify.post(layer_path, json={"source_id": str(uuid4()), "name": "Racing"})
            assert second_response.status_code == absent.status_code == 404
            assert second_response.json() == absent.json()
            assert verify.get(f"{source_path}/{source_id}").status_code == 404
            assert verify.get(layer_path).json()["items"] == []
            assert _counts(world) == (1, 7, 4, 0, 2, 2)

    assert _rows(source_id) == native
    with get_admin_engine().connect() as connection:
        orphan_active = connection.execute(
            text("""
            SELECT count(*) FROM spatial.layers l
            LEFT JOIN spatial.data_sources s
              ON (s.id,s.organization_id,s.project_id)=(l.source_id,l.organization_id,l.project_id)
            WHERE l.organization_id=:org AND l.project_id=:project AND l.lifecycle='active'
              AND (s.id IS NULL OR s.lifecycle <> 'active')
        """),
            {"org": world.organization_id, "project": world.project_id},
        ).scalar_one()
    assert orphan_active == 0
