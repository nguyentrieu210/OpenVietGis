"""R003 database constraints, real runtime-role RLS and migration cycle."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from uuid import UUID, uuid4

import pytest
from alembic import command
from alembic.config import Config
from sqlalchemy import Connection, inspect, text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.orm import Session
from tools.test_database import assert_safe_test_target

from openvietgis_api.adapters.persistence.db import get_engine, get_session_factory
from openvietgis_api.adapters.persistence.rls import (
    bind_authenticated_actor,
    bind_authenticated_service_actor,
    bind_verified_project_context,
    clear_project_context,
    require_project_context,
    set_request_context,
)
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import (
    AuthorizationDeniedError,
    TenantResourceNotFoundError,
)
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world

ROOT = Path(__file__).resolve().parents[5]
TABLES = ("data_sources", "source_fields", "reference_features", "layers", "catalog_changes")
# The original fixture seeds only the R003 tables. The full current schema
# also includes the immutable model/scene/tile tables, all checked for RLS.
ALL_TABLES = (*TABLES, "model_assets", "model_placements", "scene_versions",
              "tilesets", "tileset_contents")
SCHEMA_REVISION = "a" * 64
CONTENT_REVISION = "b" * 64
MANIFEST_DIGEST = "c" * 64

SOURCE_SQL = """
INSERT INTO spatial.data_sources (
    id, organization_id, project_id, name, kind, reference_key, fixture_version,
    manifest_digest, source_asset_id, lifecycle, validation_state, validation_reason,
    source_crs, stored_crs, schema_state, geometry_family, dimensions,
    feature_count_state, feature_count, extent_state, extent,
    catalog_revision, content_revision, schema_revision, created_at, updated_at, retired_at
) VALUES (
    :source_id, :org, :project, 'Reference', 'reference_fixture', :reference_key, 'v1',
    :manifest, NULL, 'active', 'ready', NULL,
    4326, 4326, 'known', 'Point', 'XY',
    'known', 1, 'available', CAST(:extent AS jsonb),
    1, :content, :schema, now(), now(), NULL
)
"""
FIELD_SQL = """
INSERT INTO spatial.source_fields (
    organization_id, project_id, source_id, field_id, name, ordinal, type, nullable,
    schema_revision
) VALUES (:org, :project, :source_id, 'fld_name', 'Name', 0, 'text', false, :schema)
"""
FEATURE_SQL = """
INSERT INTO spatial.reference_features (
    organization_id, project_id, source_id, feature_id, attributes, geometry,
    geometry_state, content_revision, schema_revision
) VALUES (
    :org, :project, :source_id, :feature_id,
    '{"fld_name":"Reference"}'::jsonb, ST_SetSRID(ST_MakePoint(107, 21), 4326),
    'present', :content, :schema
)
"""
LAYER_SQL = """
INSERT INTO spatial.layers (
    id, organization_id, project_id, source_id, name, lifecycle, layer_revision,
    created_at, updated_at, retired_at
) VALUES (:layer_id, :org, :project, :source_id, 'Layer', 'active', 1, now(), now(), NULL)
"""
CHANGE_SQL = """
INSERT INTO spatial.catalog_changes (
    id, organization_id, project_id, resource_type, resource_id, source_id, layer_id,
    actor_type, actor_id, actor_display_snapshot, operation, created_at, audit_event_id,
    prior_revision, new_revision, changes
) VALUES (
    :change_id, :org, :project, 'data_source', :source_id, :source_id, NULL,
    'human', :actor, 'Synthetic actor', 'register_source', now(), :audit_id,
    NULL, CAST(:revision AS jsonb),
    CAST(:changes AS jsonb)
)
"""
INSERTS = (SOURCE_SQL, FIELD_SQL, FEATURE_SQL, LAYER_SQL, CHANGE_SQL)


@dataclass(frozen=True)
class SpatialScope:
    organization: UUID
    project: UUID
    actor: UUID
    source: UUID
    layer: UUID
    change: UUID
    audit: UUID

    def values(self) -> dict[str, object]:
        return {
            "org": self.organization,
            "project": self.project,
            "actor": self.actor,
            "source_id": self.source,
            "layer_id": self.layer,
            "change_id": self.change,
            "audit_id": self.audit,
            "reference_key": "reference_points_4326",
            "manifest": MANIFEST_DIGEST,
            "schema": SCHEMA_REVISION,
            "content": CONTENT_REVISION,
            "feature_id": "feature-A",
            "extent": json.dumps(
                {
                    "min_x": 107,
                    "min_y": 21,
                    "max_x": 107,
                    "max_y": 21,
                    "crs_code": 4326,
                }
            ),
            "changes": json.dumps([{"field": "name", "old_value": None, "new_value": "Reference"}]),
            "revision": json.dumps(
                {
                    "catalog_revision": 1,
                    "content_revision": CONTENT_REVISION,
                    "schema_revision": SCHEMA_REVISION,
                }
            ),
        }


def _create_project(world: AccessWorld) -> UUID:
    project_id = uuid4()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
            INSERT INTO workspace.projects (
                id, organization_id, workspace_id, name, slug, status,
                created_by_actor_type, created_by_actor_id, created_at, updated_at
            ) VALUES (
                :id, :org, :workspace, 'Sibling Project', :slug, 'active',
                'human', :actor, now(), now()
            )
        """),
            {
                "id": project_id,
                "org": world.organization_id,
                "workspace": world.workspace_a_id,
                "actor": world.owner_user_id,
                "slug": f"project-{project_id.hex}",
            },
        )
    return project_id


def _scope(world: AccessWorld, project: UUID | None = None) -> SpatialScope:
    scope = SpatialScope(
        world.organization_id,
        project or world.project_id,
        world.owner_user_id,
        uuid4(),
        uuid4(),
        uuid4(),
        uuid4(),
    )
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
            INSERT INTO audit.audit_events (
                id, organization_id, actor_type, actor_id, actor_display_snapshot,
                action, outcome, metadata, created_at
            ) VALUES (
                :audit_id, :org, 'human', :actor, 'Synthetic actor',
                'spatial.register', 'success', '{}'::jsonb, now()
            )
        """),
            scope.values(),
        )
    return scope


def _bind(connection: Connection, org: UUID | None, project: UUID | None) -> None:
    connection.execute(
        text("""
        SELECT set_config('openvietgis.organization_id', :org, true),
               set_config('openvietgis.project_id', :project, true)
    """),
        {"org": str(org) if org else "", "project": str(project) if project else ""},
    )


def _seed_scope(scope: SpatialScope) -> SpatialScope:
    # All spatial rows are written by the actual, restricted runtime role.
    with get_engine().begin() as connection:
        _bind(connection, scope.organization, scope.project)
        for statement in INSERTS:
            connection.execute(text(statement), scope.values())
    return scope


@pytest.fixture
def spatial_world(migrated_database: None) -> tuple[SpatialScope, SpatialScope, SpatialScope]:
    org_a, org_b = seed_access_world(), seed_access_world()
    return (
        _seed_scope(_scope(org_a)),
        _seed_scope(_scope(org_a, _create_project(org_a))),
        _seed_scope(_scope(org_b)),
    )


def _assert_denied(
    connection: Connection,
    statement: str,
    values: dict[str, object],
    sqlstate: str,
) -> None:
    with pytest.raises(DBAPIError) as exc, connection.begin_nested():
        connection.execute(text(statement), values)
    assert getattr(exc.value.orig, "sqlstate", None) == sqlstate


def test_spatial_schema_runtime_role_is_non_owner_and_rls_forced(
    migrated_database: None,
) -> None:
    with get_engine().connect() as connection:
        runtime = connection.execute(
            text("""
            SELECT current_user, rolsuper, rolbypassrls, rolcanlogin
            FROM pg_roles WHERE rolname = current_user
        """)
        ).one()
        assert tuple(runtime) == ("openvietgis_runtime", False, False, True)
        rows = (
            connection.execute(
                text("""
            SELECT c.relname, c.relrowsecurity, c.relforcerowsecurity,
                   pg_get_userbyid(c.relowner) AS owner,
                   has_table_privilege(c.oid, 'SELECT') AS readable,
                   has_table_privilege(c.oid, 'INSERT') AS insertable
            FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
            WHERE n.nspname='spatial' AND c.relkind='r'
        """)
            )
            .mappings()
            .all()
        )
        assert {row["relname"] for row in rows} == set(ALL_TABLES)
        for row in rows:
            assert row["relrowsecurity"] and row["relforcerowsecurity"], row["relname"]
            assert row["owner"] != runtime[0]
            assert row["readable"] and row["insertable"]
        assert connection.execute(
            text("SELECT has_schema_privilege('spatial','USAGE')")
        ).scalar_one()
        assert not connection.execute(
            text("SELECT has_schema_privilege('spatial','CREATE')")
        ).scalar_one()


def test_project_rls_reads_hide_same_org_sibling_and_foreign_org(
    spatial_world: tuple[SpatialScope, SpatialScope, SpatialScope],
) -> None:
    a, sibling, b = spatial_world
    for org, project, expected in (
        (a.organization, a.project, {(a.organization, a.project)}),
        (sibling.organization, sibling.project, {(sibling.organization, sibling.project)}),
        (b.organization, b.project, {(b.organization, b.project)}),
        (a.organization, b.project, set()),
        (b.organization, a.project, set()),
        (a.organization, None, set()),
        (None, a.project, set()),
        (None, None, set()),
        (a.organization, uuid4(), set()),
    ):
        with get_engine().begin() as connection:
            _bind(connection, org, project)
            for table in TABLES:
                rows = connection.execute(
                    text(f"SELECT organization_id, project_id FROM spatial.{table}")
                ).all()
                assert set(rows) == expected, table
                assert len(rows) == len(expected), table


def test_project_rls_write_checks_and_composite_foreign_keys(
    spatial_world: tuple[SpatialScope, SpatialScope, SpatialScope],
) -> None:
    a, sibling, b = spatial_world
    with get_engine().begin() as connection:
        _bind(connection, a.organization, a.project)
        for foreign in (sibling, b):
            # RLS rejects a direct write into any foreign project, even same org.
            for statement in INSERTS:
                foreign_values = {
                    **foreign.values(),
                    "source_id": uuid4(),
                    "layer_id": uuid4(),
                    "change_id": uuid4(),
                }
                _assert_denied(connection, statement, foreign_values, "42501")
            # Scoped child rows cannot reference a source in a different project.
            # The reference-feature BEFORE trigger must defer out-of-scope writes to RLS,
            # preserving the established 42501 denial instead of masking it as OV409.
            for statement in (FIELD_SQL, FEATURE_SQL, LAYER_SQL, CHANGE_SQL):
                foreign_reference = {
                    **a.values(),
                    "source_id": foreign.source,
                    "layer_id": uuid4(),
                    "change_id": uuid4(),
                    "feature_id": "new-Feature",
                }
                _assert_denied(connection, statement, foreign_reference, "23503")
        _assert_denied(
            connection,
            CHANGE_SQL,
            {
                **a.values(),
                "change_id": uuid4(),
                "audit_id": b.audit,
            },
            "23503",
        )
        # The scope keys match the current GUC, but not the actual Project ownership.
    with get_engine().begin() as connection:
        _bind(connection, a.organization, b.project)
        _assert_denied(
            connection,
            SOURCE_SQL,
            {
                **a.values(),
                "source_id": uuid4(),
                "project": b.project,
            },
            "23503",
        )
    with get_engine().begin() as connection:
        _bind(connection, a.organization, None)
        _assert_denied(
            connection,
            SOURCE_SQL,
            {
                **a.values(),
                "source_id": uuid4(),
            },
            "42501",
        )


def test_append_only_catalog_evidence_and_source_uniqueness(
    spatial_world: tuple[SpatialScope, SpatialScope, SpatialScope],
) -> None:
    a, _, _ = spatial_world
    with get_engine().begin() as connection:
        _bind(connection, a.organization, a.project)
        # CatalogChange remains append-only via privileges, not only a repository convention.
        for privilege in ("UPDATE", "DELETE", "TRUNCATE"):
            assert not connection.execute(
                text("SELECT has_table_privilege('spatial.catalog_changes', :privilege)"),
                {"privilege": privilege},
            ).scalar_one()
        for statement in (
            "UPDATE spatial.catalog_changes SET operation=operation WHERE id=:change_id",
            "DELETE FROM spatial.catalog_changes WHERE id=:change_id",
            "TRUNCATE spatial.catalog_changes",
        ):
            _assert_denied(connection, statement, a.values(), "42501")
        _assert_denied(connection, SOURCE_SQL, {**a.values(), "source_id": uuid4()}, "23505")
        assert (
            connection.execute(text("SELECT id FROM spatial.catalog_changes")).scalar_one()
            == a.change
        )


def test_storage_constraints_enforce_metadata_identity_and_evidence(
    spatial_world: tuple[SpatialScope, SpatialScope, SpatialScope],
) -> None:
    a, _, _ = spatial_world
    with get_engine().begin() as connection:
        _bind(connection, a.organization, a.project)
        for statement in (
            "UPDATE spatial.data_sources SET name='' WHERE id=:source_id",
            "UPDATE spatial.data_sources SET catalog_revision=0 WHERE id=:source_id",
            "UPDATE spatial.data_sources SET source_crs=3405 WHERE id=:source_id",
            "UPDATE spatial.data_sources SET feature_count=0 WHERE id=:source_id",
            "UPDATE spatial.data_sources SET validation_state='invalid' WHERE id=:source_id",
            "UPDATE spatial.source_fields SET ordinal=100 WHERE source_id=:source_id",
            "UPDATE spatial.reference_features SET geometry_state='absent' "
            "WHERE source_id=:source_id",
            "UPDATE spatial.reference_features SET attributes='[]'::jsonb "
            "WHERE source_id=:source_id",
            "UPDATE spatial.layers SET layer_revision=0 WHERE id=:layer_id",
        ):
            _assert_denied(connection, statement, a.values(), "23514")
        for statement in (
            "UPDATE spatial.data_sources SET id=id WHERE id=:source_id",
            "UPDATE spatial.data_sources SET reference_key=reference_key WHERE id=:source_id",
            "UPDATE spatial.layers SET source_id=source_id WHERE id=:layer_id",
            "DELETE FROM spatial.data_sources WHERE id=:source_id",
        ):
            _assert_denied(connection, statement, a.values(), "42501")
        for changes in (
            [],
            [{"field": "name", "old": None, "new": "Reference"}],
            [{"field": "name", "old_value": None, "new_value": {"nested": "secret"}}],
            [{"field": "name", "old_value": None, "new_value": None}],
            [{"field": "credential_secret", "old_value": None, "new_value": "bad"}],
        ):
            _assert_denied(
                connection,
                CHANGE_SQL,
                {
                    **a.values(),
                    "change_id": uuid4(),
                    "changes": json.dumps(changes),
                },
                "23514",
            )
        _assert_denied(
            connection,
            CHANGE_SQL,
            {
                **a.values(),
                "change_id": uuid4(),
                "revision": json.dumps({"layer_revision": 1}),
            },
            "23514",
        )


def test_revision_pins_require_consistent_atomic_parent_and_children(
    spatial_world: tuple[SpatialScope, SpatialScope, SpatialScope],
) -> None:
    a, _, _ = spatial_world
    new_pin = "d" * 64
    values = {**a.values(), "new_pin": new_pin}
    with get_engine().begin() as connection:
        _bind(connection, a.organization, a.project)
        with pytest.raises(DBAPIError) as exc, connection.begin_nested():
            connection.execute(
                text("""
                UPDATE spatial.data_sources SET schema_revision=:new_pin WHERE id=:source_id
            """),
                values,
            )
            connection.execute(text("SET CONSTRAINTS ALL IMMEDIATE"))
        assert getattr(exc.value.orig, "sqlstate", None) == "23503"
        connection.execute(
            text("""
            UPDATE spatial.data_sources SET schema_revision=:new_pin WHERE id=:source_id
        """),
            values,
        )
        connection.execute(
            text("""
            UPDATE spatial.source_fields SET schema_revision=:new_pin WHERE source_id=:source_id
        """),
            values,
        )
        connection.execute(
            text("""
            UPDATE spatial.reference_features SET schema_revision=:new_pin
            WHERE source_id=:source_id
        """),
            values,
        )
        connection.execute(text("SET CONSTRAINTS ALL IMMEDIATE"))
    with get_engine().begin() as connection:
        _bind(connection, a.organization, a.project)
        for table in ("data_sources", "source_fields", "reference_features"):
            assert (
                connection.execute(
                    text(f"SELECT schema_revision FROM spatial.{table}")
                ).scalar_one()
                == new_pin
            )


def test_project_guc_does_not_survive_pool_checkout_or_savepoint_rollback(
    spatial_world: tuple[SpatialScope, SpatialScope, SpatialScope],
) -> None:
    a, sibling, b = spatial_world
    pid: int | None = None
    for scope, commit in (
        (a, True),
        (None, True),
        (sibling, False),
        (None, False),
        (b, True),
        (None, True),
    ):
        with get_session_factory()() as session:
            current_pid, old_org, old_project = session.execute(
                text("""
                SELECT pg_backend_pid(),
                       nullif(current_setting('openvietgis.organization_id', true), ''),
                       nullif(current_setting('openvietgis.project_id', true), '')
            """)
            ).one()
            if pid is None:
                pid = current_pid
            assert current_pid == pid, "test must exercise physical connection reuse"
            assert (old_org, old_project) == (None, None)
            if scope is not None:
                set_request_context(
                    session,
                    organization_id=scope.organization,
                    actor_user_id=scope.actor,
                )
                _bind(session.connection(), scope.organization, scope.project)
                savepoint = session.begin_nested()
                _bind(session.connection(), a.organization, a.project)
                savepoint.rollback()
            for table in TABLES:
                rows = session.execute(
                    text(f"SELECT organization_id, project_id FROM spatial.{table}")
                ).all()
                expected = [] if scope is None else [(scope.organization, scope.project)]
                assert rows == expected, table
            if commit:
                session.commit()
            else:
                session.rollback()
        assert get_engine().pool.checkedout() == 0


def test_downgrade_to_0005_and_upgrade_preserves_r002_rows_and_policies(
    destructive_migration_database: None,
) -> None:
    assert_safe_test_target(get_settings().migration_db_url)
    world = seed_access_world()
    scope = _seed_scope(_scope(world))
    config = Config(str(ROOT / "apps" / "api" / "alembic.ini"))
    with get_admin_engine().connect() as connection:
        policies_before = connection.execute(
            text("""
            SELECT schemaname, tablename, policyname, roles, cmd, qual, with_check
            FROM pg_policies WHERE schemaname IN ('org','workspace','audit')
            ORDER BY schemaname, tablename, policyname
        """)
        ).all()
    try:
        command.downgrade(config, "0005_retire_foundation_sample")
        assert "spatial" not in inspect(get_admin_engine()).get_schema_names()
        with get_admin_engine().connect() as connection:
            assert (
                connection.execute(
                    text("SELECT id FROM workspace.projects WHERE id=:project"), scope.values()
                ).scalar_one()
                == scope.project
            )
            assert (
                connection.execute(
                    text("SELECT id FROM audit.audit_events WHERE id=:audit_id"), scope.values()
                ).scalar_one()
                == scope.audit
            )
    finally:
        command.upgrade(config, "head")
    assert set(inspect(get_admin_engine()).get_table_names(schema="spatial")) == set(ALL_TABLES)
    with get_admin_engine().connect() as connection:
        policies_after = connection.execute(
            text("""
            SELECT schemaname, tablename, policyname, roles, cmd, qual, with_check
            FROM pg_policies WHERE schemaname IN ('org','workspace','audit')
            ORDER BY schemaname, tablename, policyname
        """)
        ).all()
        assert policies_after == policies_before
    with get_engine().connect() as connection:
        assert (
            connection.execute(text("SELECT count(*) FROM spatial.data_sources")).scalar_one() == 0
        )


def _assert_no_project_claim(session: Session) -> None:
    assert (
        session.execute(
            text("""
        SELECT nullif(current_setting('openvietgis.project_id', true), '')
    """)
        ).scalar_one()
        is None
    )


def _grant_reader(world: AccessWorld) -> UUID:
    grant_id = uuid4()
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
            INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, scope_kind, project_id,
                role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (
                :id, :org, 'membership', :member, 'project', :project,
                'viewer', now(), 'human', :actor
            )
        """),
            {
                "id": grant_id,
                "org": world.organization_id,
                "member": world.member_membership_id,
                "project": world.project_id,
                "actor": world.owner_user_id,
            },
        )
    return grant_id


def test_bound_project_context_requires_auth_and_current_exact_project_grant(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    sibling = _create_project(world)
    grant_id = _grant_reader(world)
    scope = _seed_scope(_scope(world))
    actor = Actor("human", world.member_user_id, "Member")
    context = AuthorizationContext(actor, world.organization_id, project_id=world.project_id)
    with get_session_factory()() as session:
        # A caller-supplied actor UUID and real membership alone are insufficient.
        with pytest.raises(PermissionError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        _assert_no_project_claim(session)
        bind_authenticated_actor(session, actor)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        assert require_project_context(session, context) is Permission.PROJECT_READ
        assert (
            session.execute(text("SELECT id FROM spatial.data_sources")).scalar_one()
            == scope.source
        )

        # A different requested permission or same-org sibling cannot reuse the proof.
        with pytest.raises(PermissionError):
            require_project_context(session, context, permission=Permission.PROJECT_EDIT)
        _assert_no_project_claim(session)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        sibling_context = AuthorizationContext(
            actor,
            world.organization_id,
            project_id=sibling,
        )
        with pytest.raises(AuthorizationDeniedError):
            bind_verified_project_context(
                session,
                sibling_context,
                permission=Permission.PROJECT_READ,
            )
        _assert_no_project_claim(session)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        unknown_context = AuthorizationContext(
            actor,
            world.organization_id,
            project_id=uuid4(),
        )
        with pytest.raises(TenantResourceNotFoundError):
            bind_verified_project_context(
                session,
                unknown_context,
                permission=Permission.PROJECT_READ,
            )
        _assert_no_project_claim(session)

        # Resolve current grants again, even within one existing transaction.
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE org.resource_grants SET revoked_at=now() WHERE id=:id
            """),
                {"id": grant_id},
            )
        with pytest.raises(AuthorizationDeniedError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        _assert_no_project_claim(session)


def test_project_context_invalidation_rebind_savepoints_and_pool(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    scope = _seed_scope(_scope(world))
    actor = Actor("human", world.owner_user_id, "Owner")
    context = AuthorizationContext(actor, world.organization_id, project_id=world.project_id)
    pid = None
    with get_session_factory()() as session:
        bind_authenticated_actor(session, actor)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_EDIT)
        assert (
            require_project_context(
                session,
                context,
                permission=Permission.PROJECT_EDIT,
            )
            is Permission.PROJECT_EDIT
        )

        set_request_context(
            session,
            organization_id=world.organization_id,
            actor_user_id=world.owner_user_id,
        )
        with pytest.raises(PermissionError):
            require_project_context(session, context)
        _assert_no_project_claim(session)

        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        nested = session.begin_nested()
        with pytest.raises(PermissionError):
            require_project_context(session, context)
        _assert_no_project_claim(session)
        nested.rollback()
        # PostgreSQL rollback may restore the outer GUC. The application proof
        # must be invalidated again and clear that restored value before use.
        with pytest.raises(PermissionError):
            require_project_context(session, context)
        _assert_no_project_claim(session)

        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        session.execute(
            text("""
            SELECT set_config('openvietgis.project_id', :wrong, true)
        """),
            {"wrong": str(uuid4())},
        )
        with pytest.raises(PermissionError):
            require_project_context(session, context)
        _assert_no_project_claim(session)

        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        clear_project_context(session)
        _assert_no_project_claim(session)
        with pytest.raises(PermissionError):
            require_project_context(session, context)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        assert (
            session.execute(text("SELECT id FROM spatial.data_sources")).scalar_one()
            == scope.source
        )
        pid = session.execute(text("SELECT pg_backend_pid()")).scalar_one()
        session.commit()

    with get_session_factory()() as session:
        assert session.execute(text("SELECT pg_backend_pid()")).scalar_one() == pid
        _assert_no_project_claim(session)
        with pytest.raises(PermissionError):
            require_project_context(session, context)
        with pytest.raises(PermissionError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        _assert_no_project_claim(session)


def test_project_context_lifecycle_is_current_and_archive_read_only(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    actor = Actor("human", world.owner_user_id, "Owner")
    context = AuthorizationContext(actor, world.organization_id, project_id=world.project_id)
    with get_session_factory()() as session:
        bind_authenticated_actor(session, actor)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_EDIT)
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE workspace.projects SET status='archived', archived_at=now()
                WHERE id=:id
            """),
                {"id": world.project_id},
            )
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        for permission in (Permission.PROJECT_EDIT, Permission.PROJECT_MANAGE):
            with pytest.raises(AuthorizationDeniedError):
                bind_verified_project_context(session, context, permission=permission)
            _assert_no_project_claim(session)
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE workspace.projects SET status='deleted', deleted_at=now()
                WHERE id=:id
            """),
                {"id": world.project_id},
            )
        with pytest.raises(TenantResourceNotFoundError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        _assert_no_project_claim(session)

    another = seed_access_world()
    actor = Actor("human", another.owner_user_id, "Owner")
    context = AuthorizationContext(actor, another.organization_id, project_id=another.project_id)
    with get_session_factory()() as session:
        bind_authenticated_actor(session, actor)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE org.organizations SET status='suspended' WHERE id=:id
            """),
                {"id": another.organization_id},
            )
        with pytest.raises(AuthorizationDeniedError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        _assert_no_project_claim(session)


def test_service_project_context_rechecks_current_credential_ceiling_and_revocation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    principal, credential = uuid4(), uuid4()
    with get_admin_engine().begin() as connection:
        values = {
            "org": world.organization_id,
            "project": world.project_id,
            "principal": principal,
            "credential": credential,
            "actor": world.owner_user_id,
            "grant": uuid4(),
            "prefix": f"synthetic-{credential.hex}",
        }
        connection.execute(
            text("""
            INSERT INTO org.service_principals (
                id, organization_id, name, status, created_at, created_by_actor_type,
                created_by_actor_id
            ) VALUES (:principal, :org, 'Test Service', 'active', now(), 'human', :actor)
        """),
            values,
        )
        connection.execute(
            text("""
            INSERT INTO org.service_credentials (
                id, organization_id, service_principal_id, name, prefix, secret_hash,
                permission_ceiling, created_at
            ) VALUES (
                :credential, :org, :principal, 'Read Only', :prefix, decode(repeat('00',32),'hex'),
                ARRAY['project.read']::text[], now()
            )
        """),
            values,
        )
        connection.execute(
            text("""
            INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, service_principal_id, scope_kind,
                project_id, role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (
                :grant, :org, 'service_principal', :principal, 'project', :project,
                'manager', now(), 'human', :actor
            )
        """),
            values,
        )
    actor = Actor("service_principal", principal, "Test Service")
    context = AuthorizationContext(
        actor,
        world.organization_id,
        project_id=world.project_id,
        credential_id=credential,
    )
    with get_session_factory()() as session:
        with pytest.raises(PermissionError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        bind_authenticated_service_actor(
            session,
            actor,
            organization_id=world.organization_id,
            credential_id=credential,
        )
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        assert require_project_context(session, context) is Permission.PROJECT_READ
        with pytest.raises(AuthorizationDeniedError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_EDIT)
        _assert_no_project_claim(session)
        bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE org.service_credentials SET revoked_at=now() WHERE id=:id
            """),
                {"id": credential},
            )
        with pytest.raises(AuthorizationDeniedError):
            bind_verified_project_context(session, context, permission=Permission.PROJECT_READ)
        _assert_no_project_claim(session)
