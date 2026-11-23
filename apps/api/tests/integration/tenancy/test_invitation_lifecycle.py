from __future__ import annotations

from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.authorization_repository import SqlResourceGrantRepository
from openvietgis_api.adapters.persistence.identity_repository import SqlIdentityRepository
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.tenancy.domain import (
    InvitationStatus,
    InvitationTargetKind,
    MembershipRole,
    ResourceRole,
    ResourceScopeKind,
)
from openvietgis_api.domains.tenancy.service import (
    InvalidInvitationError,
    InvitationService,
    InvitationTargetMismatchError,
)
from openvietgis_api.main import create_app
from openvietgis_api.security.sessions import issue_session_token
from tests.fixtures.db_admin import get_admin_engine, get_admin_session_factory
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def _service(session):
    return InvitationService(
        SqlTenancyRepository(session),
        SqlIdentityRepository(session),
        SqlResourceGrantRepository(session),
        SqlAuditRepository(session),
    )


def test_invitation_recipient_binding_accept_once_and_optional_grant(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    session = get_admin_session_factory()()
    owner = Actor(actor_type="human", actor_id=world.owner_user_id, display_name="Owner")
    wrong = Actor(actor_type="human", actor_id=world.wrong_user_id, display_name="Wrong")
    member = Actor(actor_type="human", actor_id=world.member_user_id, display_name="Member")
    now = datetime.now(UTC)
    try:
        service = _service(session)
        issued = service.issue(
            actor=owner,
            organization_id=world.organization_id,
            target_kind=InvitationTargetKind.EMAIL,
            target_email="member-a@example.test",
            organization_role=MembershipRole.MEMBER,
            resource_scope_kind=ResourceScopeKind.PROJECT,
            resource_project_id=world.project_id,
            resource_role=ResourceRole.VIEWER,
            expires_at=now + timedelta(hours=1),
            now=now,
        )

        with pytest.raises(InvitationTargetMismatchError):
            service.accept(
                actor=wrong,
                user_id=world.wrong_user_id,
                identity_link_id=SqlIdentityRepository(session)
                .list_identity_links(world.wrong_user_id)[0]
                .id,
                organization_id=world.organization_id,
                invitation_id=issued.invitation.id,
                secret=issued.secret,
                now=now + timedelta(seconds=1),
            )

        membership, grant = service.accept(
            actor=member,
            user_id=world.member_user_id,
            identity_link_id=SqlIdentityRepository(session)
            .list_identity_links(world.member_user_id)[0]
            .id,
            organization_id=world.organization_id,
            invitation_id=issued.invitation.id,
            secret=issued.secret,
            now=now + timedelta(seconds=2),
        )
        assert membership.status.value == "active"
        assert grant is not None
        assert grant.project_id == world.project_id
        assert grant.role == ResourceRole.VIEWER

        with pytest.raises(InvalidInvitationError):
            service.accept(
                actor=member,
                user_id=world.member_user_id,
                identity_link_id=SqlIdentityRepository(session)
                .list_identity_links(world.member_user_id)[0]
                .id,
                organization_id=world.organization_id,
                invitation_id=issued.invitation.id,
                secret=issued.secret,
                now=now + timedelta(seconds=3),
            )
        session.commit()

        stored = SqlTenancyRepository(session).get_invitation(
            context=AuthorizationContext(actor=owner, organization_id=world.organization_id),
            organization_id=world.organization_id,
            invitation_id=issued.invitation.id,
        )
        assert stored is not None
        assert stored.status == InvitationStatus.ACCEPTED
    finally:
        session.close()


def test_invitation_expiry_and_revocation_are_rejected(migrated_database: None) -> None:
    world = seed_access_world()
    session = get_admin_session_factory()()
    owner = Actor(actor_type="human", actor_id=world.owner_user_id, display_name="Owner")
    member = Actor(actor_type="human", actor_id=world.member_user_id, display_name="Member")
    now = datetime.now(UTC)
    try:
        service = _service(session)
        expiring = service.issue(
            actor=owner,
            organization_id=world.organization_id,
            target_kind=InvitationTargetKind.USER,
            target_user_id=world.member_user_id,
            organization_role=MembershipRole.MEMBER,
            expires_at=now + timedelta(seconds=1),
            now=now,
        )
        with pytest.raises(InvalidInvitationError, match="expired"):
            service.accept(
                actor=member,
                user_id=world.member_user_id,
                organization_id=world.organization_id,
                invitation_id=expiring.invitation.id,
                secret=expiring.secret,
                now=now + timedelta(seconds=2),
            )

        revoked_issue = service.issue(
            actor=owner,
            organization_id=world.organization_id,
            target_kind=InvitationTargetKind.USER,
            target_user_id=world.member_user_id,
            organization_role=MembershipRole.MEMBER,
            expires_at=now + timedelta(hours=1),
            now=now,
        )
        revoked = service.revoke(
            actor=owner,
            organization_id=world.organization_id,
            invitation_id=revoked_issue.invitation.id,
            now=now + timedelta(seconds=2),
        )
        assert revoked.status == InvitationStatus.REVOKED
        with pytest.raises(InvalidInvitationError):
            service.accept(
                actor=member,
                user_id=world.member_user_id,
                organization_id=world.organization_id,
                invitation_id=revoked_issue.invitation.id,
                secret=revoked_issue.secret,
                now=now + timedelta(seconds=3),
            )
        session.commit()
    finally:
        session.close()


def _client_for_identity(user_id: UUID, identity_link_id: UUID) -> TestClient:
    session_id = uuid4()
    token, digest = issue_session_token(session_id)
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
                INSERT INTO identity.web_sessions
                    (id, user_id, identity_link_id, secret_hash, issued_at, expires_at)
                VALUES (:id, :user, :link, :digest, now(), now() + interval '1 hour')
            """),
            {"id": session_id, "user": user_id, "link": identity_link_id, "digest": digest},
        )
    client = TestClient(create_app())
    client.cookies.set("ovg_session", token)
    csrf = client.get("/api/v1/auth/csrf")
    assert csrf.status_code == 200
    client.headers["X-OpenVietGIS-CSRF"] = csrf.json()["token"]
    return client


@pytest.mark.parametrize("kind", ["email", "user", "external_identity"])
def test_runtime_accepts_only_matching_invitation_recipient(
    migrated_database: None, kind: str
) -> None:
    world = seed_access_world()
    outsider = seed_access_world()
    alternate_id = uuid4()
    with get_admin_engine().begin() as connection:
        intended_id = connection.execute(
            text("SELECT id FROM identity.identity_links WHERE user_id = :uid"),
            {"uid": world.wrong_user_id},
        ).scalar_one()
        connection.execute(
            text("""
                INSERT INTO identity.identity_links
                    (id, user_id, issuer, subject, email_snapshot, email_verified,
                     display_name_snapshot, created_at)
                VALUES (:id, :uid, 'https://alternate.example', :subject,
                        'alternate@example.test', true, 'Alternative login', now())
            """),
            {"id": alternate_id, "uid": world.wrong_user_id, "subject": str(uuid4())},
        )

    targets = {
        "email": {"kind": "email", "email": "wrong@example.test"},
        "user": {"kind": "user", "user_id": str(world.wrong_user_id)},
        "external_identity": {
            "kind": "external_identity",
            "issuer": "https://issuer.example",
            "subject": f"subject-{world.wrong_user_id}",
        },
    }
    root = f"/api/v1/organizations/{world.organization_id}"
    accept_url = "/api/v1/invitations/accept"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(outsider.owner_user_id) as wrong,
        _client_for_identity(world.wrong_user_id, intended_id) as intended,
        _client_for_identity(world.wrong_user_id, alternate_id) as alternate,
    ):
        issued = owner.post(
            f"{root}/invitations",
            json={
                "target": targets[kind],
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

        rejected = wrong.post(accept_url, json=payload)
        assert rejected.status_code == 400
        assert rejected.json() == {"detail": "invalid invitation"}
        assert wrong.get(root).status_code == 404
        assert intended.get(root).status_code == 404

        # Email and external-identity invites must bind to the currently authenticated
        # link, not another identity linked to the same internal User.
        if kind != "user":
            mismatch = alternate.post(accept_url, json=payload)
            assert mismatch.status_code == 400
            assert mismatch.json() == rejected.json()
            assert intended.get(root).status_code == 404

        with get_admin_engine().connect() as connection:
            assert (
                connection.execute(
                    text("""
                    SELECT count(*) FROM org.organization_memberships
                    WHERE organization_id = :org AND user_id IN (:intended, :wrong)
                """),
                    {
                        "org": world.organization_id,
                        "intended": world.wrong_user_id,
                        "wrong": outsider.owner_user_id,
                    },
                ).scalar_one()
                == 0
            )
            assert (
                connection.execute(
                    text("SELECT status FROM org.invitations WHERE id = :id"),
                    {"id": payload["invitation_id"]},
                ).scalar_one()
                == "pending"
            )

        # User-target accepts any authenticated identity linked to that exact User.
        recipient = alternate if kind == "user" else intended
        accepted = recipient.post(accept_url, json=payload)
        assert accepted.status_code == 200
        assert accepted.json()["membership"]["user_id"] == str(world.wrong_user_id)
        assert accepted.json()["membership"]["status"] == "active"
        assert accepted.json()["grant"]["project_id"] == str(world.project_id)
        assert recipient.get(f"{root}/projects/{world.project_id}").status_code == 200
        reused = recipient.post(accept_url, json=payload)
        assert reused.status_code == 400
        assert reused.json() == rejected.json()


@pytest.mark.parametrize("state", ["expired", "revoked"])
def test_runtime_invitation_expired_or_revoked_is_unusable(
    migrated_database: None, state: str
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.wrong_user_id) as recipient,
    ):
        issued = owner.post(
            f"{root}/invitations",
            json={
                "target": {"kind": "user", "user_id": str(world.wrong_user_id)},
                "organization_role": "guest",
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        )
        assert issued.status_code == 201
        invitation_id = issued.json()["id"]
        if state == "revoked":
            assert owner.post(f"{root}/invitations/{invitation_id}/revoke").status_code == 204
        else:
            # Age only the test fixture deterministically; no sleeps or production clock changes.
            with get_admin_engine().begin() as connection:
                connection.execute(
                    text("""
                        UPDATE org.invitations
                        SET created_at = now() - interval '2 hours',
                            expires_at = now() - interval '1 hour'
                        WHERE id = :id
                    """),
                    {"id": invitation_id},
                )
        denied = recipient.post(
            "/api/v1/invitations/accept",
            json={
                "organization_id": str(world.organization_id),
                "invitation_id": invitation_id,
                "secret": issued.json()["secret"],
            },
        )
        assert denied.status_code == 400
        assert denied.json() == {"detail": "invalid invitation"}
        assert recipient.get(root).status_code == 404
        with get_admin_engine().connect() as connection:
            assert (
                connection.execute(
                    text("SELECT status FROM org.invitations WHERE id = :id"),
                    {"id": invitation_id},
                ).scalar_one()
                == state
            )
            assert (
                connection.execute(
                    text("""
                    SELECT count(*) FROM org.organization_memberships
                    WHERE organization_id = :org AND user_id = :uid
                """),
                    {"org": world.organization_id, "uid": world.wrong_user_id},
                ).scalar_one()
                == 0
            )


def test_acceptance_reuses_existing_identical_active_grant(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as recipient,
    ):
        existing = owner.post(
            f"{root}/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "viewer",
            },
        )
        assert existing.status_code == 201, existing.text
        grant_id = existing.json()["id"]
        issued = owner.post(
            f"{root}/invitations",
            json={
                "target": {"kind": "user", "user_id": str(world.member_user_id)},
                "organization_role": "member",
                "resource_grant": {
                    "scope_kind": "project",
                    "project_id": str(world.project_id),
                    "role": "viewer",
                },
                "expires_at": (datetime.now(UTC) + timedelta(hours=1)).isoformat(),
            },
        )
        assert issued.status_code == 201
        accepted = recipient.post(
            "/api/v1/invitations/accept",
            json={
                "organization_id": str(world.organization_id),
                "invitation_id": issued.json()["id"],
                "secret": issued.json()["secret"],
            },
        )
        assert accepted.status_code == 200
        assert accepted.json()["grant"]["id"] == grant_id
        assert accepted.json()["membership"]["id"] == str(world.member_membership_id)
    with get_admin_engine().connect() as connection:
        active_grants = (
            connection.execute(
                text("""
                SELECT id FROM org.resource_grants
                WHERE organization_id = :org AND membership_id = :member
                  AND project_id = :project AND role = 'viewer' AND revoked_at IS NULL
            """),
                {
                    "org": world.organization_id,
                    "member": world.member_membership_id,
                    "project": world.project_id,
                },
            )
            .scalars()
            .all()
        )
        assert active_grants == [UUID(grant_id)]
        event = connection.execute(
            text("""
                SELECT metadata FROM audit.audit_events
                WHERE organization_id = :org AND action = 'invitation.accept'
                  AND resource_id = :invite
            """),
            {"org": world.organization_id, "invite": issued.json()["id"]},
        ).scalar_one()
        assert event["grant_id"] == grant_id
        assert (
            connection.execute(
                text("""
                SELECT count(*) FROM audit.audit_events
                WHERE organization_id = :org AND action = 'authorization.grant.create'
                  AND resource_id = :grant
            """),
                {"org": world.organization_id, "grant": grant_id},
            ).scalar_one()
            == 1
        )
