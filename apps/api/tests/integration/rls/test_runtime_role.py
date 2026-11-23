from __future__ import annotations

from uuid import uuid4

import pytest
from sqlalchemy import text
from sqlalchemy.engine import make_url

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.config import get_settings
from tests.fixtures.db_admin import get_admin_engine


def test_runtime_role_is_not_superuser_or_bypassrls(migrated_database: None) -> None:
    with get_engine().connect() as connection:
        row = (
            connection.execute(
                text(
                    """
                SELECT rolsuper, rolbypassrls, rolcanlogin
                FROM pg_roles
                WHERE rolname = current_user AND current_user = 'openvietgis_runtime'
                """
                )
            )
            .mappings()
            .one()
        )

    assert row["rolsuper"] is False
    assert row["rolbypassrls"] is False
    assert row["rolcanlogin"] is True


def test_runtime_role_rls_hides_other_organization_and_audit_rows(
    migrated_database: None,
) -> None:
    user_id = uuid4()
    org_a, org_b = uuid4(), uuid4()
    workspace_a, workspace_b = uuid4(), uuid4()
    engine = get_engine()

    with get_admin_engine().begin() as connection:
        connection.execute(
            text(
                """
                INSERT INTO identity.users (id, display_name, status, created_at, updated_at)
                VALUES (:id, 'RLS User', 'active', now(), now())
                """
            ),
            {"id": user_id},
        )
        for org_id, workspace_id, slug in (
            (org_a, workspace_a, f"wa-{workspace_a.hex}"),
            (org_b, workspace_b, f"wb-{workspace_b.hex}"),
        ):
            connection.execute(
                text(
                    """
                    INSERT INTO org.organizations (
                        id, name, slug, status, created_by_actor_type,
                        created_by_actor_id, created_at, updated_at
                    )
                    VALUES (:id, 'Org', :org_slug, 'active', 'human', :user_id, now(), now())
                    """
                ),
                {"id": org_id, "org_slug": f"org-{org_id.hex}", "user_id": user_id},
            )
            connection.execute(
                text(
                    """
                    INSERT INTO workspace.workspaces (
                        id, organization_id, name, slug, status, created_by_actor_type,
                        created_by_actor_id, created_at, updated_at
                    )
                    VALUES (
                        :id, :org_id, 'Workspace', :slug, 'active', 'human',
                        :user_id, now(), now()
                    )
                    """
                ),
                {
                    "id": workspace_id,
                    "org_id": org_id,
                    "slug": slug,
                    "user_id": user_id,
                },
            )
            connection.execute(
                text(
                    """
                    INSERT INTO audit.audit_events (
                        id, organization_id, actor_type, actor_id, actor_display_snapshot,
                        action, outcome, metadata, created_at
                    )
                    VALUES (
                        :id, :org_id, 'human', :user_id, 'RLS User',
                        'test.rls', 'success', '{}'::jsonb, now()
                    )
                    """
                ),
                {"id": uuid4(), "org_id": org_id, "user_id": user_id},
            )

    with engine.connect() as connection:
        transaction = connection.begin()
        try:
            connection.execute(
                text("SELECT set_config('openvietgis.organization_id', :org, true)"),
                {"org": str(org_a)},
            )
            workspace_ids = set(
                connection.execute(text("SELECT id FROM workspace.workspaces")).scalars()
            )
            audit_orgs = set(
                connection.execute(text("SELECT organization_id FROM audit.audit_events")).scalars()
            )
            assert workspace_a in workspace_ids
            assert workspace_b not in workspace_ids
            assert audit_orgs == {org_a}
        finally:
            transaction.rollback()


def test_application_rejects_privileged_runtime_connection(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    owner = make_url(get_settings().migration_db_url)
    # A different string with the SAME privileged actor must still fail at connection.
    disguised_owner = owner.update_query_dict({"application_name": "rls-negative-test"})
    monkeypatch.setenv(
        "OPENVIETGIS_RUNTIME_DATABASE_URL",
        disguised_owner.render_as_string(hide_password=False),
    )
    get_settings.cache_clear()
    try:
        with pytest.raises(RuntimeError, match="Unsafe database role"):
            with get_engine().connect():
                pytest.fail("Privileged runtime connection was accepted")
    finally:
        if get_engine.cache_info().currsize:
            get_engine().dispose()
        get_engine.cache_clear()
        get_settings.cache_clear()
