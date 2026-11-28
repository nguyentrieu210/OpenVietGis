"""Deferred database relation guards cover canonical and alternate SQL write paths.

A synthetic relation definition is seeded as fixture data; direct writes run as
the restricted runtime role to exercise the deferred database guards.
"""
from __future__ import annotations

from uuid import uuid4

import pytest
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client, _register
from tests.integration.spatial.test_sources import _path as _source_path

pytestmark = pytest.mark.usefixtures("migrated_database")


def _bind(connection, world) -> None:
    connection.execute(
        text("""
            SELECT set_config('openvietgis.organization_id', :org, true),
                   set_config('openvietgis.project_id', :project, true)
        """),
        {"org": str(world.organization_id), "project": str(world.project_id)},
    )


def test_relation_constraints_reject_direct_runtime_writes_and_rollback() -> None:
    world = seed_access_world()
    source_path = _source_path(world)
    with _client(world.owner_user_id) as client:
        local_mutation = _register(
            client, source_path, "reference_polygons_4326", "Local relation source"
        )
        target_mutation = _register(
            client, source_path, "reference_points_4326", "Target relation source"
        )
        local_id = local_mutation["resource_id"]
        target_id = target_mutation["resource_id"]

        # Every local polygon key must point at one unique target point before
        # the persistent relation definition can be created.
        with get_admin_engine().begin() as connection:
            aligned = connection.execute(
                text("""
                    UPDATE spatial.reference_features
                    SET attributes=jsonb_set(
                        attributes, '{fld_feature_code}',
                        to_jsonb('point-' || substring(feature_id from 9)), true
                    )
                    WHERE organization_id=:org AND project_id=:project AND source_id=:source
                      AND feature_id LIKE 'polygon-%'
                """),
                {"org": world.organization_id, "project": world.project_id, "source": local_id},
            )
            assert aligned.rowcount == 3000

        # Seed the valid definition as fixture data; the production relation
        # management endpoint is served by Rust, not this FastAPI test app.
        relation_id = uuid4()
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                    INSERT INTO editing.relations (
                        id, organization_id, project_id, source_id, local_field,
                        target_source_id, target_field, cardinality, field_type
                    ) VALUES (
                        :id, :org, :project, :source, 'fld_feature_code',
                        :target, 'fld_feature_code', 'one', 'text'
                    )
                """),
                {
                    "id": relation_id,
                    "org": world.organization_id,
                    "project": world.project_id,
                    "source": local_id,
                    "target": target_id,
                },
            )

    def runtime_write(statement: str, values: dict) -> None:
        with pytest.raises(DBAPIError) as caught:
            with get_engine().begin() as connection:
                _bind(connection, world)
                connection.execute(text(statement), values)
        original = caught.value.orig
        sqlstate = getattr(original, "sqlstate", None) or getattr(original, "pgcode", None)
        assert sqlstate == "OV409", str(original)
        assert "Relation constraint would be violated" in str(original), str(original)

    # Deleting a referenced target would orphan polygon-000001.
    runtime_write(
        """
        DELETE FROM spatial.reference_features
        WHERE organization_id=:org AND project_id=:project AND source_id=:source
          AND feature_id='point-000001'
        """,
        {"org": world.organization_id, "project": world.project_id, "source": target_id},
    )
    # Changing a local key to a nonexistent point would create an orphan.
    runtime_write(
        """
        UPDATE spatial.reference_features
        SET attributes=jsonb_set(
            attributes, '{fld_feature_code}', to_jsonb('missing-target'::text), true
        )
        WHERE organization_id=:org AND project_id=:project AND source_id=:source
          AND feature_id='polygon-000001'
        """,
        {"org": world.organization_id, "project": world.project_id, "source": local_id},
    )
    # Duplicating a target key violates target-side cardinality one.
    runtime_write(
        """
        UPDATE spatial.reference_features
        SET attributes=jsonb_set(
            attributes, '{fld_feature_code}', to_jsonb('point-000001'::text), true
        )
        WHERE organization_id=:org AND project_id=:project AND source_id=:source
          AND feature_id='point-000002'
        """,
        {"org": world.organization_id, "project": world.project_id, "source": target_id},
    )

    # Each deferred failure rolls back its statement and preserves the valid
    # source population and relation definition.
    with get_admin_engine().connect() as connection:
        assert connection.execute(
            text("""
                SELECT count(*) FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:project
                  AND source_id=:source AND feature_id='point-000001'
            """),
            {"org": world.organization_id, "project": world.project_id, "source": target_id},
        ).scalar_one() == 1
        assert connection.execute(
            text("""
                SELECT attributes->>'fld_feature_code'
                FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:project
                  AND source_id=:source AND feature_id='polygon-000001'
            """),
            {"org": world.organization_id, "project": world.project_id, "source": local_id},
        ).scalar_one() == "point-000001"
        assert connection.execute(
            text("""
                SELECT attributes->>'fld_feature_code'
                FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:project
                  AND source_id=:source AND feature_id='point-000002'
            """),
            {"org": world.organization_id, "project": world.project_id, "source": target_id},
        ).scalar_one() == "point-000002"
        assert connection.execute(
            text("""
                SELECT id FROM editing.relations
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """),
            {"org": world.organization_id, "project": world.project_id, "id": relation_id},
        ).scalar_one() == relation_id
