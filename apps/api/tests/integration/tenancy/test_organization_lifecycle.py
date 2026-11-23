from __future__ import annotations

from dataclasses import replace

import pytest
from sqlalchemy import text

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.platform_repository import SqlPlatformRepository
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.tenancy.domain import MembershipRole, OrganizationStatus
from openvietgis_api.domains.tenancy.service import OwnerRequiredError, TenancyService
from tests.fixtures.db_admin import get_admin_engine, get_admin_session_factory
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_organization_lifecycle_is_owner_only_and_suspension_blocks_normal_access(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    owner_context = AuthorizationContext(
        actor=Actor(actor_type="human", actor_id=world.owner_user_id, display_name="Owner"),
        organization_id=world.organization_id,
    )
    session = get_admin_session_factory()()
    try:
        repository = SqlTenancyRepository(session)
        member = repository.get_membership(
            context=owner_context,
            organization_id=world.organization_id,
            membership_id=world.member_membership_id,
            for_update=True,
        )
        assert member is not None
        repository.update_membership(
            replace(member, role=MembershipRole.ADMIN), context=owner_context
        )
        session.commit()
    finally:
        session.close()

    owner = Actor(actor_type="human", actor_id=world.owner_user_id, display_name="Owner")
    admin = Actor(actor_type="human", actor_id=world.member_user_id, display_name="Admin")

    session = get_admin_session_factory()()
    try:
        service = TenancyService(
            SqlTenancyRepository(session),
            SqlPlatformRepository(session),
            SqlAuditRepository(session),
        )
        with pytest.raises(OwnerRequiredError):
            service.update_organization(
                actor=admin,
                organization_id=world.organization_id,
                status=OrganizationStatus.SUSPENDED,
            )
        session.rollback()
    finally:
        session.close()

    session = get_admin_session_factory()()
    try:
        TenancyService(
            SqlTenancyRepository(session),
            SqlPlatformRepository(session),
            SqlAuditRepository(session),
        ).update_organization(
            actor=owner,
            organization_id=world.organization_id,
            status=OrganizationStatus.SUSPENDED,
        )
        session.commit()
    finally:
        session.close()

    session = get_admin_session_factory()()
    try:
        context = AuthorizationContext(
            actor=admin,
            organization_id=world.organization_id,
            project_id=world.project_id,
        )
        permissions = SqlEffectivePermissionProvider(session).resolve(context)
        assert Permission.PROJECT_READ not in permissions
        assert permissions == frozenset()
        session.rollback()
    finally:
        session.close()

    session = get_admin_session_factory()()
    try:
        restored = TenancyService(
            SqlTenancyRepository(session),
            SqlPlatformRepository(session),
            SqlAuditRepository(session),
        ).update_organization(
            actor=owner,
            organization_id=world.organization_id,
            status=OrganizationStatus.ACTIVE,
        )
        assert restored.status == OrganizationStatus.ACTIVE
        session.commit()
    finally:
        session.close()


def test_runtime_owner_controls_suspend_restore_and_soft_delete(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("UPDATE org.organization_memberships SET role = 'admin' WHERE id = :id"),
            {"id": world.member_membership_id},
        )
    root = f"/api/v1/organizations/{world.organization_id}"
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as admin,
    ):
        for status in ("suspended", "deleted"):
            assert admin.patch(root, json={"status": status}).status_code == 403
            assert owner.get(root).json()["status"] == "active"

        suspended = owner.patch(root, json={"status": "suspended"})
        assert suspended.status_code == 200, suspended.text
        assert suspended.json()["status"] == "suspended"
        assert admin.patch(root, json={"status": "active"}).status_code == 403
        assert (
            owner.patch(f"{root}/projects/{world.project_id}", json={"name": "blocked"}).status_code
            == 403
        )
        assert admin.get(f"{root}/projects").status_code == 403

        restored = owner.patch(root, json={"status": "active"})
        assert restored.status_code == 200
        assert restored.json()["status"] == "active"
        assert admin.get(f"{root}/projects/{world.project_id}").status_code == 200

        deleted = owner.patch(root, json={"status": "deleted"})
        assert deleted.status_code == 200
        assert deleted.json()["status"] == "deleted"
        assert admin.patch(root, json={"status": "active"}).status_code == 403
        assert (
            owner.post(f"{root}/projects", json={"name": "blocked", "slug": "blocked"}).status_code
            == 403
        )

        # Persisted ownership and references survive deletion; this is not hard purge.
        with get_admin_engine().connect() as connection:
            stored = connection.execute(
                text("""
                    SELECT o.deleted_at, p.organization_id, p.workspace_id
                    FROM org.organizations o
                    JOIN workspace.projects p ON p.organization_id = o.id
                    WHERE o.id = :org AND p.id = :pid
                """),
                {"org": world.organization_id, "pid": world.project_id},
            ).one()
            assert stored.deleted_at is not None
            assert stored.organization_id == world.organization_id
            assert stored.workspace_id == world.workspace_a_id
            audit_statuses = set(
                connection.execute(
                    text("""
                    SELECT metadata ->> 'status'
                    FROM audit.audit_events
                    WHERE organization_id = :org AND action = 'organization.update'
                      AND actor_id = :actor AND outcome = 'success'
                """),
                    {"org": world.organization_id, "actor": world.owner_user_id},
                ).scalars()
            )
            assert {"suspended", "active", "deleted"} <= audit_statuses
        assert owner.patch(root, json={"status": "active"}).status_code == 200
        assert admin.get(f"{root}/projects/{world.project_id}").status_code == 200
