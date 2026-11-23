from __future__ import annotations

from uuid import uuid4

import pytest
from sqlalchemy import inspect, text
from sqlalchemy.exc import IntegrityError

from tests.fixtures.db_admin import get_admin_engine


def test_identity_tenant_schemas_and_tables_exist(migrated_database: None) -> None:
    inspector = inspect(get_admin_engine())

    assert {"users", "identity_links", "web_sessions"} <= set(
        inspector.get_table_names(schema="identity")
    )
    assert {"organizations", "organization_memberships", "resource_grants", "invitations"} <= set(
        inspector.get_table_names(schema="org")
    )
    assert {"workspaces", "projects"} <= set(inspector.get_table_names(schema="workspace"))
    assert "audit_events" in inspector.get_table_names(schema="audit")
    assert "installation_state" in inspector.get_table_names(schema="platform")


def test_installation_state_is_singleton(migrated_database: None) -> None:
    engine = get_admin_engine()
    with engine.begin() as connection:
        row = (
            connection.execute(
                text(
                    """
                SELECT singleton_key, initialized_at
                FROM platform.installation_state
                WHERE singleton_key = 'installation'
                """
                )
            )
            .mappings()
            .one()
        )
        assert row["singleton_key"] == "installation"

    with pytest.raises(IntegrityError):
        with engine.begin() as connection:
            connection.execute(
                text(
                    """
                    INSERT INTO platform.installation_state (
                        singleton_key, initialized_at, initial_organization_id,
                        initialized_by_user_id, updated_at
                    )
                    VALUES ('another', NULL, NULL, NULL, now())
                    """
                )
            )


def test_project_workspace_composite_fk_rejects_cross_organization_grouping(
    migrated_database: None,
) -> None:
    user_id = uuid4()
    org_a = uuid4()
    org_b = uuid4()
    workspace_b = uuid4()
    project_a = uuid4()

    engine = get_admin_engine()
    with engine.begin() as connection:
        connection.execute(
            text(
                """
                INSERT INTO identity.users (id, display_name, status, created_at, updated_at)
                VALUES (:id, 'Tester', 'active', now(), now())
                """
            ),
            {"id": user_id},
        )
        for org_id, slug in ((org_a, f"a-{org_a.hex}"), (org_b, f"b-{org_b.hex}")):
            connection.execute(
                text(
                    """
                    INSERT INTO org.organizations (
                        id, name, slug, status, created_by_actor_type,
                        created_by_actor_id, created_at, updated_at
                    )
                    VALUES (:id, 'Org', :slug, 'active', 'human', :user_id, now(), now())
                    """
                ),
                {"id": org_id, "slug": slug, "user_id": user_id},
            )
        connection.execute(
            text(
                """
                INSERT INTO workspace.workspaces (
                    id, organization_id, name, slug, status, created_by_actor_type,
                    created_by_actor_id, created_at, updated_at
                )
                VALUES (
                    :id, :org_id, 'Workspace B', :slug, 'active', 'human',
                    :user_id, now(), now()
                )
                """
            ),
            {
                "id": workspace_b,
                "org_id": org_b,
                "slug": f"w-{workspace_b.hex}",
                "user_id": user_id,
            },
        )

    with pytest.raises(IntegrityError):
        with engine.begin() as connection:
            connection.execute(
                text(
                    """
                    INSERT INTO workspace.projects (
                        id, organization_id, workspace_id, name, slug, status,
                        created_by_actor_type, created_by_actor_id, created_at, updated_at
                    )
                    VALUES (
                        :id, :org_id, :workspace_id, 'Project', :slug, 'active',
                        'human', :user_id, now(), now()
                    )
                    """
                ),
                {
                    "id": project_a,
                    "org_id": org_a,
                    "workspace_id": workspace_b,
                    "slug": f"p-{project_a.hex}",
                    "user_id": user_id,
                },
            )


def test_identity_link_issuer_subject_is_unique(migrated_database: None) -> None:
    user_a, user_b = uuid4(), uuid4()
    subject = f"subject-{uuid4()}"
    engine = get_admin_engine()
    with engine.begin() as connection:
        for user_id in (user_a, user_b):
            connection.execute(
                text(
                    """
                    INSERT INTO identity.users (id, display_name, status, created_at, updated_at)
                    VALUES (:id, 'Identity User', 'active', now(), now())
                    """
                ),
                {"id": user_id},
            )
        connection.execute(
            text(
                """
                INSERT INTO identity.identity_links (
                    id, user_id, issuer, subject, email_verified, created_at
                )
                VALUES (:id, :user_id, 'https://issuer.example', :subject, true, now())
                """
            ),
            {"id": uuid4(), "user_id": user_a, "subject": subject},
        )

    with pytest.raises(IntegrityError):
        with engine.begin() as connection:
            connection.execute(
                text(
                    """
                    INSERT INTO identity.identity_links (
                        id, user_id, issuer, subject, email_verified, created_at
                    )
                    VALUES (:id, :user_id, 'https://issuer.example', :subject, true, now())
                    """
                ),
                {"id": uuid4(), "user_id": user_b, "subject": subject},
            )
