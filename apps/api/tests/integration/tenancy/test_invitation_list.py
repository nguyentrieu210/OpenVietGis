from datetime import UTC, datetime, timedelta

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_invitation_list_is_admin_only_and_excludes_secret(migrated_database):
    world = seed_access_world()
    outsider = seed_access_world()
    url = f"/api/v1/organizations/{world.organization_id}/invitations"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
        client_for_user(outsider.owner_user_id) as other,
    ):
        create = owner.post(
            url,
            json={
                "target": {"kind": "email", "email": "invite@example.test"},
                "organization_role": "guest",
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        )
        assert create.status_code == 201, create.text
        invitation_id = create.json()["id"]
        payload = {
            "organization_id": str(world.organization_id),
            "invitation_id": invitation_id,
            "secret": create.json()["secret"],
        }
        check = member.post("/api/v1/invitations/check", json=payload)
        assert check.status_code == 200 and check.json() == {"status": "mismatch"}
        bad = member.post("/api/v1/invitations/check", json={**payload, "secret": "x" * 48})
        assert bad.status_code == 200 and bad.json() == {"status": "invalid"}
        response = owner.get(url)
        assert response.status_code == 200, response.text
        row = next(r for r in response.json() if r["id"] == invitation_id)
        assert row["email"] == "invite@example.test" and row["status"] == "pending"
        assert "secret" not in row and "token_hash" not in row
        assert member.get(url).status_code == 403
        assert other.get(url).status_code in {403, 404}
        assert owner.post(f"{url}/{invitation_id}/revoke").status_code == 204
        assert (
            next(r for r in owner.get(url).json() if r["id"] == invitation_id)["status"]
            == "revoked"
        )
        assert member.post("/api/v1/invitations/check", json=payload).json() == {
            "status": "invalid"
        }
        eligible = owner.post(
            url,
            json={
                "target": {"kind": "email", "email": "member-a@example.test"},
                "organization_role": "member",
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        ).json()
        assert member.post(
            "/api/v1/invitations/check",
            json={**payload, "invitation_id": eligible["id"], "secret": eligible["secret"]},
        ).json() == {"status": "eligible"}
