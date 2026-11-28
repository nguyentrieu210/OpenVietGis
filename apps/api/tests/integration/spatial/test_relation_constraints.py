"""Deferred database relation guards cover canonical and alternate SQL write paths.

The application API creates the relation; direct writes run as the restricted
runtime role to prove constraints do not depend on the Rust changeset path.
"""
from __future__ import annotations

import pytest
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client, _get, _register
from tests.integration.spatial.test_sources import _path as _source_path

pytestmark = pytest.mark.usefixtures("migrated_database")


def _relation_path(world) -> str:
    return (
        f"/api/v1/organizations/{world.organization_id}/projects/"
        f"{world.project_id}/editing/relations"
    )


def _bind(connection, world) -> None:
    connection.execute(
        text("""
            SELECT set_config('openvietgis.organization_id', :org, true),
                   set_config('openvietgis.project_id', :project, true)
        """),
        {"org": str(world.organization_id), "project": str(world.project_id)},
    )


def _set_feature_code(connection, world, source_id: str, feature_id: str, value: str) -> None:
    result = connection.execute(
        text("""
            UPDATE spatial.reference_features
            SET attributes=jsonb_set(attributes, '{fld_feature_code}', to_jsonb(CAST(:value AS text)), true)
            WHERE organization_id=:org AND project_id=:project AND source_id=:source
              AND feature_id=:feature
        """),
        {
            "org": world.organization_id,
            "project": world.project_id,
            "source": source_id,
            "feature": feature_id,
            "value": value,
        },
    )
    assert result.rowcount == 1


def test_relation_constraints_reject_direct_runtime_writes_and_rollback() -> None:
    world = seed_access_world()
    source_path = _source_path(world)
    with _client(world.owner_user_id) as client:
        local_mutation = _register(client, source_path, "reference_points_4326", "Local relation source")
        target_mutation = _register(client, source_path, "reference_polygons_4326", "Target relation source")
        local_id = local_mutation["resource_id"]
        target_id = target_mutation["resource_id"]
        local = _get(client, source_path, local_id)
        target = _get(client, source_path, target_id)

        # Create one valid reference using synthetic rows and an independently
        # chosen key; other target keys remain unique.
        with get_admin_engine().begin() as connection:
            _set_feature_code(connection, world, local_id, "point-000001", "ward-001")
            _set_feature_code(connection, world, target_id, "polygon-000001", "ward-001")

        request = {
            "source_id": local_id,
            "local_field": "fld_feature_code",
            "target_source_id": target_id,
            "target_field": "fld_feature_code",
            "cardinality": "one",
            "source_schema_revision": local["schema_revision"],
            "target_schema_revision": target["schema_revision"],
        }
        response = client.post(_relation_path(world), json=request)
        assert response.status_code == 200, response.text
        rule_id = response.json()["id"]

    def runtime_write(statement: str, values: dict) -> None:
        with pytest.raises(DBAPIError):
            with get_engine().begin() as connection:
                _bind(connection, world)
                connection.execute(text(statement), values)

    # A target delete would orphan the local key.
    runtime_write(
        """
        DELETE FROM spatial.reference_features
        WHERE organization_id=:org AND project_id=:project AND source_id=:source
          AND feature_id='polygon-000001'
        """,
        {"org": world.organization_id, "project": world.project_id, "source": target_id},
    )
    # A local key update would point at no target.
    runtime_write(
        """
        UPDATE spatial.reference_features
        SET attributes=jsonb_set(attributes, '{fld_feature_code}', to_jsonb('missing-target'::text), true)
        WHERE organization_id=:org AND project_id=:project AND source_id=:source
          AND feature_id='point-000001'
        """,
        {"org": world.organization_id, "project": world.project_id, "source": local_id},
    )
    # A second target with the same key violates the target-side one-cardinality.
    runtime_write(
        """
        UPDATE spatial.reference_features
        SET attributes=jsonb_set(attributes, '{fld_feature_code}', to_jsonb('ward-001'::text), true)
        WHERE organization_id=:org AND project_id=:project AND source_id=:source
          AND feature_id='polygon-000002'
        """,
        {"org": world.organization_id, "project": world.project_id, "source": target_id},
    )

    # Deferred failures roll back the attempted write and leave the valid
    # relation population intact for subsequent reads.
    with get_admin_engine().connect() as connection:
        assert connection.execute(
            text("""
                SELECT count(*) FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:project
                  AND source_id=:source AND feature_id='polygon-000001'
            """),
            {"org": world.organization_id, "project": world.project_id, "source": target_id},
        ).scalar_one() == 1
        assert connection.execute(
            text("""
                SELECT attributes->>'fld_feature_code'
                FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:project
                  AND source_id=:source AND feature_id='point-000001'
            """),
            {"org": world.organization_id, "project": world.project_id, "source": local_id},
        ).scalar_one() == "ward-001"
        assert connection.execute(
            text("""
                SELECT attributes->>'fld_feature_code'
                FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:project
                  AND source_id=:source AND feature_id='polygon-000002'
            """),
            {"org": world.organization_id, "project": world.project_id, "source": target_id},
        ).scalar_one() == "polygon-000002"

    with _client(world.owner_user_id) as client:
        response = client.get(_relation_path(world))
        assert response.status_code == 200, response.text
        assert rule_id in {item["id"] for item in response.json()["items"]}
