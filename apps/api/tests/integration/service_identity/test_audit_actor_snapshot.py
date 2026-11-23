from __future__ import annotations

from datetime import UTC, datetime
from uuid import UUID, uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def _insert_snapshot_event(
    *,
    organization_id: UUID,
    actor_type: str,
    actor_id: UUID | None,
    snapshot: str,
    action: str,
) -> UUID:
    event_id = uuid4()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text(
                """
                INSERT INTO audit.audit_events (
                    id, organization_id, actor_type, actor_id, actor_display_snapshot,
                    action, resource_type, resource_id, outcome, metadata, created_at
                )
                VALUES (
                    :id, :organization_id, :actor_type, :actor_id, :snapshot,
                    :action, 'fixture', NULL, 'success', '{}'::jsonb, :created_at
                )
                """
            ),
            {
                "id": event_id,
                "organization_id": organization_id,
                "actor_type": actor_type,
                "actor_id": actor_id,
                "snapshot": snapshot,
                "action": action,
                "created_at": datetime.now(UTC),
            },
        )
    return event_id


def test_actor_snapshots_survive_human_and_service_revocation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"

    with client_for_user(world.owner_user_id) as owner:
        created = owner.post(
            f"{root}/service-principals",
            json={"name": f"snapshot-{uuid4().hex}", "description": "audit snapshot fixture"},
        )
        assert created.status_code == 201, created.text
        principal_id = UUID(created.json()["id"])

        human_event = _insert_snapshot_event(
            organization_id=world.organization_id,
            actor_type="human",
            actor_id=world.member_user_id,
            snapshot="Member before revoke",
            action="fixture.snapshot.human",
        )
        service_event = _insert_snapshot_event(
            organization_id=world.organization_id,
            actor_type="service_principal",
            actor_id=principal_id,
            snapshot="Automation before revoke",
            action="fixture.snapshot.service",
        )
        system_event = _insert_snapshot_event(
            organization_id=world.organization_id,
            actor_type="system",
            actor_id=None,
            snapshot="system:bootstrap",
            action="fixture.snapshot.system",
        )

        revoked_member = owner.patch(
            f"{root}/members/{world.member_membership_id}",
            json={"status": "revoked"},
        )
        assert revoked_member.status_code == 200, revoked_member.text
        revoked_service = owner.post(f"{root}/service-principals/{principal_id}/revoke")
        assert revoked_service.status_code == 204, revoked_service.text

        response = owner.get(f"{root}/audit-events", params={"limit": 200})
        assert response.status_code == 200, response.text
        by_id = {row["id"]: row for row in response.json()}

    assert by_id[str(human_event)]["actor_type"] == "human"
    assert by_id[str(human_event)]["actor_display_snapshot"] == "Member before revoke"
    assert by_id[str(service_event)]["actor_type"] == "service_principal"
    assert by_id[str(service_event)]["actor_display_snapshot"] == "Automation before revoke"
    assert by_id[str(system_event)]["actor_type"] == "system"
    assert by_id[str(system_event)]["actor_display_snapshot"] == "system:bootstrap"


def test_runtime_service_mutations_are_audited_and_survive_revocation(
    migrated_database: None,
) -> None:
    from tests.integration.service_identity.test_service_authorization import _service_client

    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    principal_name = f"runtime-audit-{uuid4().hex}"
    with client_for_user(world.owner_user_id) as owner:
        created = owner.post(f"{root}/service-principals", json={"name": principal_name})
        assert created.status_code == 201, created.text
        principal_id = created.json()["id"]
        grant = owner.post(
            f"{root}/grants",
            json={
                "subject_kind": "service_principal",
                "subject_id": principal_id,
                "scope_kind": "organization",
                "role": "manager",
            },
        )
        assert grant.status_code == 201, grant.text
        issued = owner.post(
            f"{root}/service-principals/{principal_id}/credentials",
            json={
                "name": "runtime-audit",
                "permission_ceiling": ["workspace.manage", "project.manage", "project.read"],
            },
        )
        assert issued.status_code == 201
        credential_id = issued.json()["id"]
        secret = issued.json()["secret"]
        with _service_client(secret) as service:
            workspace = service.post(
                f"{root}/workspaces",
                json={
                    "name": "Service workspace",
                    "slug": f"svc-ws-{uuid4().hex}",
                },
            )
            assert workspace.status_code == 201, workspace.text
            workspace_id = workspace.json()["id"]
            modified_workspace = service.patch(
                f"{root}/workspaces/{workspace_id}", json={"name": "Updated workspace"}
            )
            assert modified_workspace.status_code == 200, modified_workspace.text
            project = service.post(
                f"{root}/projects",
                json={
                    "name": "Service project",
                    "slug": f"svc-project-{uuid4().hex}",
                    "workspace_id": workspace_id,
                },
            )
            assert project.status_code == 201, project.text
            project_id = project.json()["id"]
            updated = service.patch(
                f"{root}/projects/{project_id}", json={"name": "Service update"}
            )
            assert updated.status_code == 200, updated.text
        assert (
            owner.post(
                f"{root}/service-principals/{principal_id}/credentials/{credential_id}/revoke"
            ).status_code
            == 204
        )
        assert owner.post(f"{root}/service-principals/{principal_id}/revoke").status_code == 204
        with _service_client(secret) as service:
            assert (
                service.patch(
                    f"{root}/projects/{project_id}", json={"name": "Denied update"}
                ).status_code
                == 401
            )
        response = owner.get(f"{root}/audit-events", params={"limit": 200})
        assert response.status_code == 200
        secret_exposed = secret in response.text
        assert not secret_exposed
        rows = response.json()

    service_rows = [row for row in rows if row["actor_id"] == principal_id]
    assert len(service_rows) == 4
    assert {row["action"] for row in service_rows} == {
        "workspace.create",
        "workspace.update",
        "project.create",
        "project.update",
    }
    assert {row["resource_id"] for row in service_rows} == {workspace_id, project_id}
    assert all(
        row["actor_type"] == "service_principal"
        and row["actor_display_snapshot"] == principal_name
        and row["organization_id"] == str(world.organization_id)
        and row["outcome"] == "success"
        for row in service_rows
    )
    owner_rows = [row for row in rows if row["actor_id"] == str(world.owner_user_id)]
    assert {
        "service_principal.create",
        "service_credential.issue",
        "service_credential.revoke",
        "service_principal.revoke",
    } <= {row["action"] for row in owner_rows}
