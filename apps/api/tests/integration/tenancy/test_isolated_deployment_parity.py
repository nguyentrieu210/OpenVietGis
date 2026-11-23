from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Literal
from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user
from tests.integration.service_identity.test_service_authorization import _service_client


def _exercise(mode: Literal["shared", "isolated"]) -> dict[str, object]:
    # Logical deployment fixture: the shared case has another tenant in its
    # visible test universe; the isolated case exercises only the single tenant.
    # Both intentionally use the same runtime roles, API contracts, and domain.
    world = seed_access_world()
    other = seed_access_world() if mode == "shared" else None
    root = f"/api/v1/organizations/{world.organization_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.wrong_user_id) as recipient,
    ):
        account = owner.get("/api/v1/me")
        assert account.status_code == 200
        assert account.json()["id"] == str(world.owner_user_id)
        organizations = owner.get("/api/v1/me/organizations")
        assert organizations.status_code == 200
        assert [(org["id"], org["role"]) for org in organizations.json()] == [
            (str(world.organization_id), "owner")
        ]
        if other is not None:
            assert owner.get(f"/api/v1/organizations/{other.organization_id}").status_code == 404
        project_path = f"{root}/projects/{world.project_id}"
        assert recipient.get(project_path).status_code == 404
        invitation = owner.post(
            f"{root}/invitations",
            json={
                "target": {"kind": "user", "user_id": str(world.wrong_user_id)},
                "organization_role": "guest",
                "resource_grant": {
                    "scope_kind": "project",
                    "project_id": str(world.project_id),
                    "role": "viewer",
                },
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        )
        assert invitation.status_code == 201
        accepted = recipient.post(
            "/api/v1/invitations/accept",
            json={
                "organization_id": str(world.organization_id),
                "invitation_id": invitation.json()["id"],
                "secret": invitation.json()["secret"],
            },
        )
        assert accepted.status_code == 200
        member = accepted.json()["membership"]
        grant = accepted.json()["grant"]
        assert member["role"] == "guest"
        assert grant["role"] == "viewer"
        member_read = recipient.get(project_path).status_code
        member_write = recipient.patch(project_path, json={"name": "Denied"}).status_code
        assert (member_read, member_write) == (200, 403)

        created = owner.post(f"{root}/service-principals", json={"name": f"parity-{uuid4().hex}"})
        assert created.status_code == 201
        principal_id = created.json()["id"]
        service_grant = owner.post(
            f"{root}/grants",
            json={
                "subject_kind": "service_principal",
                "subject_id": principal_id,
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "viewer",
            },
        )
        assert service_grant.status_code == 201
        issued = owner.post(
            f"{root}/service-principals/{principal_id}/credentials",
            json={
                "name": "parity",
                "permission_ceiling": ["project.read"],
            },
        )
        assert issued.status_code == 201
        service_credential = issued.json()["id"]
        with _service_client(issued.json()["secret"]) as service:
            service_read = service.get(project_path).status_code
            service_write = service.patch(project_path, json={"name": "Denied"}).status_code
            assert (service_read, service_write) == (200, 403)
            assert (
                owner.post(
                    f"{root}/service-principals/{principal_id}/credentials/{service_credential}/revoke"
                ).status_code
                == 204
            )
            service_after_revoke = service.get(project_path).status_code
            assert service_after_revoke == 401

        assert (
            owner.patch(f"{root}/members/{member['id']}", json={"status": "revoked"}).status_code
            == 200
        )
        member_after_revoke = recipient.get(project_path).status_code
        assert member_after_revoke == 404
        audit = owner.get(f"{root}/audit-events", params={"limit": 200})
        assert audit.status_code == 200
        actions = {event["action"] for event in audit.json()}
        assert {
            "invitation.create",
            "invitation.accept",
            "authorization.grant.create",
            "organization.membership.update",
            "service_principal.create",
            "service_credential.issue",
            "service_credential.revoke",
        } <= actions
        assert all(event["organization_id"] == str(world.organization_id) for event in audit.json())
        return {
            "identity": account.json()["status"],
            "owner_role": organizations.json()[0]["role"],
            "membership_role": member["role"],
            "grant_role": grant["role"],
            "member_access": (member_read, member_write, member_after_revoke),
            "service_access": (service_read, service_write, service_after_revoke),
            "audit_actions": actions,
        }


def test_same_domain_and_contracts_in_shared_and_isolated_deployment_fixtures(
    migrated_database: None,
) -> None:
    assert _exercise("shared") == _exercise("isolated")
