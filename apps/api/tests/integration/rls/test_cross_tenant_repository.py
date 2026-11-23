from __future__ import annotations

from uuid import UUID, uuid4

import pytest
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.adapters.persistence.rls import set_request_context
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world

# Fixed table allowlist; never interpolate caller-controlled identifiers.
TABLES = (
    "org.organization_memberships",
    "workspace.workspaces",
    "workspace.projects",
    "org.resource_grants",
    "org.invitations",
    "audit.audit_events",
)


def _seed_protected_rows(world: AccessWorld) -> dict[str, UUID]:
    grant, invite, audit = uuid4(), uuid4(), uuid4()
    with get_admin_engine().begin() as connection:
        args = {
            "org": world.organization_id,
            "member": world.member_membership_id,
            "actor": world.owner_user_id,
            "grant": grant,
            "invite": invite,
            "audit": audit,
        }
        connection.execute(
            text("""
            INSERT INTO org.resource_grants
              (id, organization_id, subject_kind, membership_id, scope_kind,
               role, created_at, created_by_actor_type, created_by_actor_id)
            VALUES (:grant,:org,'membership',:member,'organization',
                    'viewer',now(),'human',:actor)
        """),
            args,
        )
        connection.execute(
            text("""
            INSERT INTO org.invitations
              (id, organization_id, status, target_kind, target_email_normalized,
               organization_role, token_hash, token_prefix, expires_at,
               created_at, created_by_actor_type, created_by_actor_id)
            VALUES (:invite,:org,'pending','email','rls@example.test',
                    'member',decode('0102','hex'),'test',now()+interval '1 hour',
                    now(),'human',:actor)
        """),
            args,
        )
        connection.execute(
            text("""
            INSERT INTO audit.audit_events
              (id, organization_id, actor_type, actor_id, actor_display_snapshot,
               action, outcome, metadata, created_at)
            VALUES (:audit,:org,'human',:actor,'RLS fixture','test.rls',
                    'success','{}'::jsonb,now())
        """),
            args,
        )
    return dict(
        zip(
            TABLES,
            (
                world.owner_membership_id,
                world.workspace_a_id,
                world.project_id,
                grant,
                invite,
                audit,
            ),
            strict=True,
        )
    )


def test_all_p1_protected_tables_and_pool_reuse(migrated_database: None) -> None:
    a, b = seed_access_world(), seed_access_world()
    rows_a, rows_b = _seed_protected_rows(a), _seed_protected_rows(b)
    engine = get_engine()
    # Reuse the exact same authenticated physical connection for A/B/none.
    with engine.connect() as connection:
        assert connection.execute(text("SELECT current_user")).scalar_one() == (
            "openvietgis_runtime"
        )
        connection.rollback()
        for selected, visible in ((a, rows_a), (b, rows_b), (None, {}), (a, rows_a)):
            with Session(bind=connection) as session:
                set_request_context(
                    session,
                    organization_id=selected.organization_id if selected else None,
                    actor_user_id=selected.owner_user_id if selected else None,
                )
                for table in TABLES:
                    found = set(
                        session.execute(
                            text(f"SELECT id FROM {table} WHERE id IN (:a, :b)"),
                            {"a": rows_a[table], "b": rows_b[table]},
                        ).scalars()
                    )
                    assert found == ({visible[table]} if selected else set()), table
                    if selected is not None and table != "audit.audit_events":
                        hidden = rows_b[table] if selected is a else rows_a[table]
                        with session.begin_nested():
                            changed = (
                                session.execute(
                                    text(
                                        f"UPDATE {table} SET organization_id=organization_id "
                                        "WHERE id=:hidden RETURNING id"
                                    ),
                                    {"hidden": hidden},
                                )
                                .scalars()
                                .all()
                            )
                            assert changed == [], table
                session.rollback()


def test_runtime_cannot_change_audit_even_with_tenant_context(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    rows = _seed_protected_rows(world)
    with Session(bind=get_engine()) as session:
        set_request_context(
            session,
            organization_id=world.organization_id,
            actor_user_id=world.owner_user_id,
        )
        with pytest.raises(DBAPIError):
            session.execute(
                text("UPDATE audit.audit_events SET action='tampered' WHERE id=:id"),
                {"id": rows["audit.audit_events"]},
            )
        session.rollback()
