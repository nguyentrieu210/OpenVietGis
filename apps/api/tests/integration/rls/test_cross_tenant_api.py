from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.main import create_app
from openvietgis_api.security.sessions import issue_session_token
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world


def client_for_user(user_id: UUID) -> TestClient:
    session_id = uuid4()
    raw_token, digest = issue_session_token(session_id)
    with get_admin_engine().begin() as connection:
        link_id = connection.execute(
            text("SELECT id FROM identity.identity_links WHERE user_id=:user"), {"user": user_id}
        ).scalar_one()
        connection.execute(
            text("""
            INSERT INTO identity.web_sessions
                (id, user_id, identity_link_id, secret_hash, issued_at, expires_at)
            VALUES (:id,:user,:link,:digest,now(),now()+interval '1 hour')
        """),
            {"id": session_id, "user": user_id, "link": link_id, "digest": digest},
        )
    client = TestClient(create_app())
    client.cookies.set("ovg_session", raw_token)
    csrf = client.get("/api/v1/auth/csrf")
    assert csrf.status_code == 200
    client.headers["X-OpenVietGIS-CSRF"] = csrf.json()["token"]
    return client


def test_cross_tenant_routes_and_identifier_substitution(migrated_database: None) -> None:
    a, b = seed_access_world(), seed_access_world()
    with client_for_user(a.owner_user_id) as owner_a, client_for_user(b.owner_user_id) as owner_b:
        root_a = f"/api/v1/organizations/{a.organization_id}"
        root_b = f"/api/v1/organizations/{b.organization_id}"
        grant = owner_b.post(
            f"{root_b}/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(b.member_membership_id),
                "scope_kind": "project",
                "project_id": str(b.project_id),
                "role": "viewer",
            },
        )
        assert grant.status_code == 201, grant.text
        grant_id = grant.json()["id"]
        invitation = owner_b.post(
            f"{root_b}/invitations",
            json={
                "target": {"kind": "user", "user_id": str(b.wrong_user_id)},
                "organization_role": "guest",
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        )
        assert invitation.status_code == 201
        invitation_id = invitation.json()["id"]
        assert owner_a.get(root_a).status_code == 200
        assert owner_a.get(f"{root_a}/projects/{a.project_id}").status_code == 200
        for path in (
            root_b,
            f"{root_b}/members",
            f"{root_b}/workspaces",
            f"{root_b}/projects",
            f"{root_b}/projects/{b.project_id}",
            f"{root_b}/grants",
            f"{root_b}/effective-permissions",
        ):
            denied = owner_a.get(path)
            assert denied.status_code == 404, (path, denied.status_code, denied.text)
        for path, payload in (
            (root_b, {"name": "Not allowed"}),
            (f"{root_b}/members/{b.member_membership_id}", {"status": "revoked"}),
            (f"{root_b}/workspaces/{b.workspace_a_id}", {"name": "Not allowed"}),
            (f"{root_b}/projects/{b.project_id}", {"name": "Not allowed"}),
            (f"{root_a}/members/{b.member_membership_id}", {"status": "revoked"}),
            (f"{root_a}/workspaces/{b.workspace_a_id}", {"name": "Not allowed"}),
            (f"{root_a}/projects/{b.project_id}", {"name": "Not allowed"}),
        ):
            denied = owner_a.patch(path, json=payload)
            assert denied.status_code == 404, (path, denied.status_code, denied.text)
        for path in (
            f"{root_b}/grants/{grant_id}/revoke",
            f"{root_a}/grants/{grant_id}/revoke",
            f"{root_b}/invitations/{invitation_id}/revoke",
            f"{root_a}/invitations/{invitation_id}/revoke",
        ):
            denied = owner_a.post(path)
            assert denied.status_code == 404, (path, denied.status_code, denied.text)
        invalid = owner_a.post(
            "/api/v1/invitations/accept",
            json={
                "organization_id": str(b.organization_id),
                "invitation_id": invitation_id,
                "secret": "invalid-invitation-fixture-value-000000",
            },
        )
        assert invalid.status_code == 400
        assert invalid.json()["detail"] == "invalid invitation"
        assert owner_b.get(f"{root_b}/projects/{b.project_id}").status_code == 200
    with client_for_user(a.member_user_id) as member_a:
        denied = member_a.get(f"/api/v1/organizations/{a.organization_id}/members")
        assert denied.status_code == 403

        root_a = f"/api/v1/organizations/{a.organization_id}"
        assert (
            member_a.patch(f"{root_a}/projects/{a.project_id}", json={"name": "Denied"}).status_code
            == 403
        )
        assert (
            member_a.patch(f"{root_a}/projects/{b.project_id}", json={"name": "Denied"}).status_code
            == 404
        )
        assert (
            member_a.patch(
                f"{root_a}/workspaces/{a.workspace_a_id}", json={"name": "Denied"}
            ).status_code
            == 403
        )
        assert (
            member_a.patch(
                f"{root_a}/workspaces/{b.workspace_a_id}", json={"name": "Denied"}
            ).status_code
            == 404
        )


def test_invitation_accept_and_immediate_revocation_via_runtime_api(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.wrong_user_id) as target,
        client_for_user(world.member_user_id) as wrong,
    ):
        issued = owner.post(
            f"{root}/invitations",
            json={
                "target": {"kind": "email", "email": "wrong@example.test"},
                "organization_role": "guest",
                "resource_grant": {
                    "scope_kind": "project",
                    "project_id": str(world.project_id),
                    "role": "viewer",
                },
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        )
        assert issued.status_code == 201
        payload = {
            "organization_id": str(world.organization_id),
            "invitation_id": issued.json()["id"],
            "secret": issued.json()["secret"],
        }
        assert target.get(root).status_code == 404
        assert wrong.post("/api/v1/invitations/accept", json=payload).status_code == 400
        accepted = target.post("/api/v1/invitations/accept", json=payload)
        assert accepted.status_code == 200
        membership_id = accepted.json()["membership"]["id"]
        assert target.get(f"{root}/projects/{world.project_id}").status_code == 200
        assert target.post("/api/v1/invitations/accept", json=payload).status_code == 400
        assert (
            owner.patch(f"{root}/members/{membership_id}", json={"status": "revoked"}).status_code
            == 200
        )
        assert target.get(f"{root}/projects/{world.project_id}").status_code == 404
