from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from threading import Barrier

import pytest
from sqlalchemy import text

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.adapters.persistence.platform_repository import SqlPlatformRepository
from openvietgis_api.adapters.persistence.rls import set_request_context
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.tenancy.domain import MembershipRole
from openvietgis_api.domains.tenancy.service import LastOwnerError, TenancyService
from tests.fixtures.db_admin import get_admin_engine, get_admin_session_factory
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def _service(session):
    return TenancyService(
        SqlTenancyRepository(session),
        SqlPlatformRepository(session),
        SqlAuditRepository(session),
    )


def test_concurrent_owner_downgrades_never_leave_zero_active_owners(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    owner_context = AuthorizationContext(
        actor=Actor(actor_type="human", actor_id=world.owner_user_id, display_name="Owner"),
        organization_id=world.organization_id,
    )
    setup = get_admin_session_factory()()
    try:
        member = SqlTenancyRepository(setup).get_membership(
            context=owner_context,
            organization_id=world.organization_id,
            membership_id=world.member_membership_id,
            for_update=True,
        )
        assert member is not None
        from dataclasses import replace

        SqlTenancyRepository(setup).update_membership(
            replace(member, role=MembershipRole.OWNER), context=owner_context
        )
        setup.commit()
    finally:
        setup.close()

    start = Barrier(2)

    def downgrade(user_id, membership_id, name: str) -> str:
        session = get_session_factory()()
        try:
            set_request_context(
                session, organization_id=world.organization_id, actor_user_id=user_id
            )
            start.wait(timeout=10)
            service = _service(session)
            actor = Actor(actor_type="human", actor_id=user_id, display_name=name)
            try:
                service.update_membership(
                    actor=actor,
                    organization_id=world.organization_id,
                    membership_id=membership_id,
                    role=MembershipRole.ADMIN,
                )
            except LastOwnerError:
                session.rollback()
                return "rejected"
            session.commit()
            return "updated"
        finally:
            session.close()

    with ThreadPoolExecutor(max_workers=2) as pool:
        first = pool.submit(
            downgrade,
            world.owner_user_id,
            world.owner_membership_id,
            "Owner",
        )
        second = pool.submit(
            downgrade,
            world.member_user_id,
            world.member_membership_id,
            "Member Owner",
        )
        results = [first.result(), second.result()]

    assert sorted(results) == ["rejected", "updated"]


@pytest.mark.parametrize(
    ("first_change", "second_change"),
    [
        ({"status": "suspended"}, {"status": "revoked"}),
        ({"role": "admin"}, {"status": "suspended"}),
        ({"role": "member"}, {"status": "revoked"}),
    ],
)
def test_runtime_concurrent_owner_role_and_status_changes(
    migrated_database: None,
    first_change: dict[str, str],
    second_change: dict[str, str],
) -> None:
    world = seed_access_world()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("UPDATE org.organization_memberships SET role = 'owner' WHERE id = :id"),
            {"id": world.member_membership_id},
        )
    root = f"/api/v1/organizations/{world.organization_id}"
    start = Barrier(2)
    with (
        client_for_user(world.owner_user_id) as owner_a,
        client_for_user(world.member_user_id) as owner_b,
    ):

        def change(client, membership_id, payload: dict[str, str]) -> int:
            start.wait(timeout=10)
            return client.patch(f"{root}/members/{membership_id}", json=payload).status_code

        with ThreadPoolExecutor(max_workers=2) as pool:
            first = pool.submit(change, owner_a, world.owner_membership_id, first_change)
            second = pool.submit(change, owner_b, world.member_membership_id, second_change)
            assert sorted([first.result(timeout=20), second.result(timeout=20)]) == [200, 400]

    # Validate the committed invariant, not merely status codes from racing requests.
    with get_admin_engine().connect() as connection:
        count = connection.execute(
            text("""
                SELECT count(*) FROM org.organization_memberships
                WHERE organization_id = :org AND role = 'owner' AND status = 'active'
            """),
            {"org": world.organization_id},
        ).scalar_one()
    assert count == 1


@pytest.mark.parametrize(
    "change",
    [{"role": "admin"}, {"status": "suspended"}, {"status": "revoked"}],
)
def test_runtime_cannot_remove_the_last_active_owner(
    migrated_database: None, change: dict[str, str]
) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    with client_for_user(world.owner_user_id) as owner:
        denied = owner.patch(f"{root}/members/{world.owner_membership_id}", json=change)
        assert denied.status_code == 400
        members = owner.get(f"{root}/members")
        assert members.status_code == 200
        assert [
            member["id"]
            for member in members.json()
            if member["role"] == "owner" and member["status"] == "active"
        ] == [str(world.owner_membership_id)]
