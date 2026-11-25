from pathlib import Path

import pytest
from alembic import command
from alembic.config import Config
from sqlalchemy import inspect, text
from sqlalchemy.exc import DBAPIError
from tools.test_database import assert_safe_test_target

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.config import get_settings
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

ROOT = Path(__file__).resolve().parents[4]


def test_migrations_apply_from_zero_to_head(destructive_migration_database: None) -> None:
    assert_safe_test_target(get_settings().database_url)
    config = Config(str(ROOT / "apps" / "api" / "alembic.ini"))
    command.downgrade(config, "base")
    command.upgrade(config, "head")

    inspector = inspect(get_engine())
    assert "sample_records" not in inspector.get_table_names(schema="foundation")
    assert {"service_principals", "service_credentials"} <= set(
        inspector.get_table_names(schema="org")
    )
    assert {"workspaces", "projects"} <= set(inspector.get_table_names(schema="workspace"))


def test_populated_extension_history_refuses_lossy_downgrade(
    destructive_migration_database: None,
) -> None:
    """A blocked rollback must leave revision, protected history and data intact."""
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"
    with _client(world.owner_user_id) as client:
        manifest = client.post(
            f"{org}/extensions/manifests",
            json={
                "extension_key": "rollback_notes",
                "version": "1.0.0",
                "display_name": "Rollback notes",
                "permissions": ["ext.rollback_notes.write"],
                "entity_schemas": {
                    "note": {"fields": {"text": {"type": "text", "required": True}}}
                },
            },
        )
        assert manifest.status_code == 201, manifest.text
        activation = client.post(
            f"{project}/extensions/activations", json={"manifest_id": manifest.json()["id"]}
        )
        assert activation.status_code == 201, activation.text
        route = f"{project}/extensions/rollback_notes/records"
        record = client.post(route, json={"entity_name": "note", "payload": {"text": "retain"}})
        assert record.status_code == 201, record.text
        history_route = f"{route}/{record.json()['id']}/history"
        before = client.get(history_route)
        assert before.status_code == 200, before.text

    config = Config(str(ROOT / "apps" / "api" / "alembic.ini"))
    with get_admin_engine().connect() as connection:
        before_revisions = set(
            connection.execute(text("SELECT version_num FROM alembic_version")).scalars()
        )
    with pytest.raises(DBAPIError, match="R014 history exists; restore pre-upgrade backup"):
        command.downgrade(config, "0018_production_ops")

    with get_admin_engine().connect() as connection:
        # The complete downgrade is transactional, including all later revisions.
        assert set(
            connection.execute(text("SELECT version_num FROM alembic_version")).scalars()
        ) == before_revisions
        for table in ("record_revisions", "activation_events", "import_receipts"):
            assert (
                connection.execute(
                    text(
                        "SELECT relrowsecurity AND relforcerowsecurity FROM pg_class "
                        "WHERE oid=CAST(:name AS regclass)"
                    ),
                    {"name": f"extensions.{table}"},
                ).scalar_one()
                is True
            )
    with _client(world.owner_user_id) as client:
        after = client.get(history_route)
        assert after.status_code == 200 and after.json() == before.json()
        retained = client.get(route)
        assert retained.status_code == 200
        assert retained.json()["items"] == [record.json()]
