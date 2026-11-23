from __future__ import annotations

from uuid import uuid4

import pytest
from sqlalchemy import text
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.adapters.persistence.rls import set_request_context
from openvietgis_api.security.secrets import digest_secret
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world


def _seed_service_identity(world) -> tuple[object, object]:
    principal_id = uuid4()
    credential_id = uuid4()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text(
                """
                INSERT INTO org.service_principals (
                    id, organization_id, name, description, status,
                    created_at, created_by_actor_type, created_by_actor_id
                )
                VALUES (
                    :id, :organization_id, :name, 'fixture', 'active',
                    now(), 'human', :actor_id
                )
                """
            ),
            {
                "id": principal_id,
                "organization_id": world.organization_id,
                "name": f"service-{principal_id.hex}",
                "actor_id": world.owner_user_id,
            },
        )
        connection.execute(
            text(
                """
                INSERT INTO org.service_credentials (
                    id, organization_id, service_principal_id, name, prefix,
                    secret_hash, permission_ceiling, created_at
                )
                VALUES (
                    :id, :organization_id, :principal_id, 'fixture', :prefix,
                    :secret_hash, ARRAY['project.read']::text[], now()
                )
                """
            ),
            {
                "id": credential_id,
                "organization_id": world.organization_id,
                "principal_id": principal_id,
                "prefix": f"ovgsvc-{credential_id.hex[:12]}",
                "secret_hash": digest_secret("fixture-secret-" + credential_id.hex),
            },
        )
    return principal_id, credential_id


def test_machine_identity_tables_are_force_rls_and_runtime_tenant_scoped(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    principal_a, credential_a = _seed_service_identity(a)
    principal_b, credential_b = _seed_service_identity(b)

    with get_admin_engine().connect() as connection:
        rows = connection.execute(
            text(
                """
                SELECT n.nspname, c.relname, c.relrowsecurity, c.relforcerowsecurity
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname='org'
                  AND c.relname IN ('service_principals','service_credentials')
                ORDER BY c.relname
                """
            )
        ).all()
    assert rows == [
        ("org", "service_credentials", True, True),
        ("org", "service_principals", True, True),
    ]

    with Session(bind=get_engine()) as session:
        set_request_context(
            session,
            organization_id=a.organization_id,
            actor_user_id=a.owner_user_id,
        )
        assert set(
            session.execute(
                text("SELECT id FROM org.service_principals WHERE id IN (:a,:b)"),
                {"a": principal_a, "b": principal_b},
            ).scalars()
        ) == {principal_a}
        assert set(
            session.execute(
                text("SELECT id FROM org.service_credentials WHERE id IN (:a,:b)"),
                {"a": credential_a, "b": credential_b},
            ).scalars()
        ) == {credential_a}
        session.rollback()

        set_request_context(session, organization_id=None, actor_user_id=None)
        assert (
            session.execute(text("SELECT count(*) FROM org.service_principals")).scalar_one() == 0
        )
        assert (
            session.execute(text("SELECT count(*) FROM org.service_credentials")).scalar_one() == 0
        )


def test_service_grant_and_credential_cannot_cross_organization(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    principal_b, _ = _seed_service_identity(b)

    with pytest.raises(IntegrityError):
        with get_admin_engine().begin() as connection:
            connection.execute(
                text(
                    """
                    INSERT INTO org.resource_grants (
                        id, organization_id, subject_kind, service_principal_id,
                        scope_kind, project_id, role, created_at,
                        created_by_actor_type, created_by_actor_id
                    )
                    VALUES (
                        :id, :organization_id, 'service_principal', :principal_id,
                        'project', :project_id, 'viewer', now(),
                        'human', :actor_id
                    )
                    """
                ),
                {
                    "id": uuid4(),
                    "organization_id": a.organization_id,
                    "principal_id": principal_b,
                    "project_id": a.project_id,
                    "actor_id": a.owner_user_id,
                },
            )


def test_service_credential_constraints_reject_bad_hash_ceiling_and_expiry(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    principal_id, _ = _seed_service_identity(world)

    invalid_rows = (
        {
            "hash": b"short",
            "ceiling": ["project.read"],
            "expires": "now() + interval '1 hour'",
        },
        {
            "hash": b"x" * 32,
            "ceiling": ["not.a.permission"],
            "expires": "now() + interval '1 hour'",
        },
        {
            "hash": b"x" * 32,
            "ceiling": ["project.read"],
            "expires": "now() - interval '1 hour'",
        },
    )
    for row in invalid_rows:
        with pytest.raises(IntegrityError):
            with get_admin_engine().begin() as connection:
                connection.execute(
                    text(
                        f"""
                        INSERT INTO org.service_credentials (
                            id, organization_id, service_principal_id, name,
                            prefix, secret_hash, permission_ceiling, created_at, expires_at
                        )
                        VALUES (
                            :id, :organization_id, :principal_id, 'invalid',
                            :prefix, :secret_hash, :ceiling, now(), {row["expires"]}
                        )
                        """
                    ),
                    {
                        "id": uuid4(),
                        "organization_id": world.organization_id,
                        "principal_id": principal_id,
                        "prefix": f"ovgsvc-{uuid4().hex[:12]}",
                        "secret_hash": row["hash"],
                        "ceiling": row["ceiling"],
                    },
                )
