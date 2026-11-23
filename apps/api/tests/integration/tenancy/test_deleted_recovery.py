from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.main import create_app
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_deleted_workspace_and_project_restore_is_status_only_tenant_scoped_and_audited(
    migrated_database: None,
) -> None:
    world, other = seed_access_world(), seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    foreign_root = f"/api/v1/organizations/{other.organization_id}"
    project = f"{root}/projects/{world.project_id}"
    workspace = f"{root}/workspaces/{world.workspace_a_id}"

    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
        client_for_user(other.owner_user_id) as unrelated_owner,
    ):
        before = owner.get(project).json()
        grant = owner.post(
            f"{root}/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "viewer",
            },
        )
        assert grant.status_code == 201, grant.text
        assert member.get(project).status_code == 200

        # Mark both records archived first so the recovery also proves timestamps reset.
        assert owner.patch(workspace, json={"status": "archived"}).status_code == 200
        assert owner.patch(project, json={"status": "archived"}).status_code == 200
        assert owner.patch(project, json={"status": "deleted"}).status_code == 200

        # DELETED is not an ordinary readable/editable Project.
        assert owner.get(project).status_code == 404
        assert member.get(project).status_code == 404
        assert str(world.project_id) not in {
            item["id"] for item in owner.get(f"{root}/projects").json()
        }
        assert owner.patch(project, json={"name": "denied"}).status_code == 400
        assert owner.patch(project, json={"status": "archived"}).status_code == 400
        assert owner.patch(project, json={"status": "active", "name": None}).status_code == 400
        assert (
            owner.patch(project, json={"status": "active", "workspace_id": None}).status_code == 400
        )
        assert member.patch(project, json={"status": "active"}).status_code == 403
        assert (
            unrelated_owner.patch(
                f"{foreign_root}/projects/{world.project_id}", json={"status": "active"}
            ).status_code
            == 404
        )

        # Recovery preserves grouping: first restore the original Workspace.
        assert owner.patch(project, json={"status": "active"}).status_code == 400
        assert owner.patch(workspace, json={"status": "deleted"}).status_code == 200
        assert owner.patch(project, json={"status": "active"}).status_code == 400
        assert str(world.workspace_a_id) not in {
            item["id"] for item in owner.get(f"{root}/workspaces").json()
        }
        assert (
            owner.get(
                f"{root}/projects", params={"workspace_id": str(world.workspace_a_id)}
            ).status_code
            == 404
        )
        assert (
            owner.patch(workspace, json={"status": "active", "name": "denied"}).status_code == 400
        )
        assert owner.patch(workspace, json={"status": "active", "name": None}).status_code == 400
        assert owner.patch(workspace, json={"name": "denied"}).status_code == 400
        assert member.patch(workspace, json={"status": "active"}).status_code == 403
        assert (
            unrelated_owner.patch(
                f"{foreign_root}/workspaces/{world.workspace_a_id}", json={"status": "active"}
            ).status_code
            == 404
        )

        restored_workspace = owner.patch(workspace, json={"status": "active"})
        assert restored_workspace.status_code == 200, restored_workspace.text
        assert restored_workspace.json()["id"] == str(world.workspace_a_id)
        assert restored_workspace.json()["organization_id"] == str(world.organization_id)
        restored_project = owner.patch(project, json={"status": "active"})
        assert restored_project.status_code == 200, restored_project.text
        assert restored_project.json() == before
        assert member.get(project).json() == before

    with get_admin_engine().connect() as conn:
        for table, resource_id in (
            ("workspace.workspaces", world.workspace_a_id),
            ("workspace.projects", world.project_id),
        ):
            row = conn.execute(
                text(f"SELECT status, archived_at, deleted_at FROM {table} WHERE id = :id"),
                {"id": resource_id},
            ).one()
            assert tuple(row) == ("active", None, None)
        restored_events = conn.execute(
            text("""
                SELECT action, actor_type, actor_id, resource_id, metadata
                FROM audit.audit_events
                WHERE organization_id = :org
                  AND metadata->>'previous_status' = 'deleted'
                  AND metadata->>'status' = 'active'
                ORDER BY action
            """),
            {"org": world.organization_id},
        ).all()
        actual = [
            (row.action, row.actor_type, row.actor_id, row.resource_id) for row in restored_events
        ]
        assert actual == [
            ("project.update", "human", world.owner_user_id, world.project_id),
            ("workspace.update", "human", world.owner_user_id, world.workspace_a_id),
        ]


def test_deleted_project_restore_requires_both_permissions_even_with_service_credential(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    project = f"{root}/projects/{world.project_id}"

    with client_for_user(world.owner_user_id) as owner:
        principal = owner.post(f"{root}/service-principals", json={"name": "deleted-restore"})
        assert principal.status_code == 201, principal.text
        principal_id = principal.json()["id"]
        grant = owner.post(
            f"{root}/grants",
            json={
                "subject_kind": "service_principal",
                "subject_id": principal_id,
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "manager",
            },
        )
        assert grant.status_code == 201, grant.text
        assert owner.patch(project, json={"workspace_id": None}).status_code == 200
        assert owner.patch(project, json={"status": "deleted"}).status_code == 200
        for name, ceiling, expected in (
            ("archive-only", ["project.archive"], 403),
            ("delete-only", ["project.delete"], 403),
            ("recovery", ["project.archive", "project.delete"], 200),
        ):
            credential = owner.post(
                f"{root}/service-principals/{principal_id}/credentials",
                json={"name": name, "permission_ceiling": ceiling},
            )
            assert credential.status_code == 201, credential.text
            with TestClient(create_app()) as service:
                service.headers["Authorization"] = "Bearer " + credential.json()["secret"]
                response = service.patch(project, json={"status": "active"})
                assert response.status_code == expected, response.text
        restored = owner.get(project)
        assert restored.status_code == 200, restored.text
        assert restored.json()["id"] == str(world.project_id)
        assert restored.json()["organization_id"] == str(world.organization_id)
        assert restored.json()["workspace_id"] is None
