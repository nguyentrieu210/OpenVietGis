from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import uuid4

from fastapi.testclient import TestClient

from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def _create_grant_and_invitation(owner: TestClient, world: AccessWorld) -> tuple[str, str]:
    root = f"/api/v1/organizations/{world.organization_id}"
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
    invite = owner.post(
        f"{root}/invitations",
        json={
            "target": {"kind": "user", "user_id": str(world.wrong_user_id)},
            "organization_role": "guest",
            "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
        },
    )
    assert invite.status_code == 201
    return grant.json()["id"], invite.json()["id"]


def test_foreign_resource_is_indistinguishable_from_absent_and_not_same_tenant_403(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    root_a = f"/api/v1/organizations/{a.organization_id}"
    root_b = f"/api/v1/organizations/{b.organization_id}"
    with (
        client_for_user(a.owner_user_id) as owner_a,
        client_for_user(b.owner_user_id) as owner_b,
        client_for_user(a.member_user_id) as member,
    ):
        grant_a, invite_a = _create_grant_and_invitation(owner_a, a)
        grant_b, invite_b = _create_grant_and_invitation(owner_b, b)
        # Tenant ownership itself cannot be inferred from a 404 body.
        other_org = member.get(root_b)
        absent_org = member.get(f"/api/v1/organizations/{uuid4()}")
        assert other_org.status_code == absent_org.status_code == 404
        assert other_org.json() == absent_org.json()

        for method, path, own_id, foreign_id, payload in (
            (
                "PATCH",
                "members/{id}",
                a.member_membership_id,
                b.member_membership_id,
                {"status": "revoked"},
            ),
            ("PATCH", "workspaces/{id}", a.workspace_a_id, b.workspace_a_id, {"name": "Denied"}),
            ("PATCH", "projects/{id}", a.project_id, b.project_id, {"name": "Denied"}),
            ("POST", "grants/{id}/revoke", grant_a, grant_b, None),
            ("POST", "invitations/{id}/revoke", invite_a, invite_b, None),
        ):
            own, foreign, absent = [
                member.request(method, f"{root_a}/{path.format(id=resource_id)}", json=payload)
                for resource_id in (own_id, foreign_id, uuid4())
            ]
            # Same tenant is distinguishable only as an authorization denial.
            assert own.status_code == 403, (method, path, own.status_code)
            assert foreign.status_code == absent.status_code == 404, (
                method,
                path,
                foreign.status_code,
                absent.status_code,
            )
            assert foreign.json() == absent.json(), (method, path)
            assert str(foreign_id) not in foreign.text

        # Failed actions cannot change foreign resources or their permissions.
        assert owner_b.get(f"{root_b}/projects/{b.project_id}").status_code == 200
        assert any(item["id"] == grant_b for item in owner_b.get(f"{root_b}/grants").json())
