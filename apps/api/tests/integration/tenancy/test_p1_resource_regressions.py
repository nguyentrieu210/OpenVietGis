from __future__ import annotations

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_archived_project_cannot_restore_inside_deleted_workspace(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    project_url = f"{root}/projects/{world.project_id}"
    with client_for_user(world.owner_user_id) as owner:
        archived = owner.patch(project_url, json={"status": "archived"})
        assert archived.status_code == 200, archived.text
        deleted = owner.patch(
            f"{root}/workspaces/{world.workspace_a_id}", json={"status": "deleted"}
        )
        assert deleted.status_code == 200, deleted.text
        rejected = owner.patch(project_url, json={"status": "active"})
        assert rejected.status_code == 400
        before = owner.get(project_url).json()
        assert before["status"] == "archived"
        assert before["workspace_id"] == str(world.workspace_a_id)

        restored = owner.patch(project_url, json={"status": "active", "workspace_id": None})
        assert restored.status_code == 200, restored.text
        assert restored.json()["status"] == "active"
        assert restored.json()["workspace_id"] is None
        assert restored.json()["organization_id"] == str(world.organization_id)
        assert restored.json()["id"] == str(world.project_id)


def test_empty_project_patch_cannot_mutate_or_append_audit(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    url = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    with client_for_user(world.owner_user_id) as owner:
        before = owner.get(url)
        assert before.status_code == 200
        rejected = owner.patch(url, json={})
        assert rejected.status_code in (400, 422)
        assert owner.get(url).json() == before.json()
    from sqlalchemy import text

    from tests.fixtures.db_admin import get_admin_engine

    with get_admin_engine().connect() as connection:
        count = connection.execute(
            text("""
                SELECT count(*) FROM audit.audit_events
                WHERE organization_id = :org AND resource_id = :project
                  AND action = 'project.update'
            """),
            {"org": world.organization_id, "project": world.project_id},
        ).scalar_one()
        assert count == 0
