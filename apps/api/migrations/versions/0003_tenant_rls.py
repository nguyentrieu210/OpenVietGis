"""Add runtime role and tenant row-level security."""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

revision: str = "0003_tenant_rls"
down_revision: str | Sequence[str] | None = "0002_identity_tenant_core"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

PROTECTED_TABLES = (
    ("org", "organization_memberships"),
    ("workspace", "workspaces"),
    ("workspace", "projects"),
    ("org", "resource_grants"),
    ("org", "invitations"),
    ("audit", "audit_events"),
)


def _tenant_expr(column: str = "organization_id") -> str:
    return f"{column} = NULLIF(current_setting('openvietgis.organization_id', true), '')::uuid"


def upgrade() -> None:
    bind = op.get_bind()
    role_exists = bool(
        bind.execute(
            sa.text("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'openvietgis_runtime')")
        ).scalar_one()
    )
    if not role_exists:
        op.execute(
            """
            CREATE ROLE openvietgis_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
                NOINHERIT NOBYPASSRLS
            """
        )
    op.execute(
        """
        ALTER ROLE openvietgis_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
            NOINHERIT NOBYPASSRLS
        """
    )

    op.execute(
        "GRANT USAGE ON SCHEMA identity, org, workspace, audit, platform, foundation "
        "TO openvietgis_runtime"
    )
    op.execute(
        "GRANT SELECT, INSERT, UPDATE ON identity.users, identity.identity_links, "
        "identity.web_sessions TO openvietgis_runtime"
    )
    op.execute(
        "GRANT SELECT, INSERT, UPDATE ON org.organizations, org.organization_memberships, "
        "org.resource_grants, org.invitations TO openvietgis_runtime"
    )
    op.execute(
        "GRANT SELECT, INSERT, UPDATE ON workspace.workspaces, workspace.projects "
        "TO openvietgis_runtime"
    )
    op.execute("GRANT SELECT, INSERT ON audit.audit_events TO openvietgis_runtime")
    op.execute("GRANT SELECT, UPDATE ON platform.installation_state TO openvietgis_runtime")
    op.execute("GRANT SELECT, INSERT, UPDATE ON foundation.sample_records TO openvietgis_runtime")

    for schema, table in PROTECTED_TABLES:
        op.execute(f'ALTER TABLE "{schema}"."{table}" ENABLE ROW LEVEL SECURITY')
        op.execute(f'ALTER TABLE "{schema}"."{table}" FORCE ROW LEVEL SECURITY')

    op.execute(
        f"""
        CREATE POLICY memberships_tenant_or_self ON org.organization_memberships
        FOR SELECT TO openvietgis_runtime
        USING (
            {_tenant_expr()}
            OR (
                NULLIF(current_setting('openvietgis.organization_id', true), '') IS NULL
                AND user_id = NULLIF(
                    current_setting('openvietgis.actor_user_id', true), ''
                )::uuid
            )
        )
        """
    )
    op.execute(
        f"""
        CREATE POLICY memberships_tenant_write ON org.organization_memberships
        FOR ALL TO openvietgis_runtime
        USING ({_tenant_expr()})
        WITH CHECK ({_tenant_expr()})
        """
    )

    for schema, table in (
        ("workspace", "workspaces"),
        ("workspace", "projects"),
        ("org", "resource_grants"),
        ("org", "invitations"),
    ):
        expression = _tenant_expr()
        policy = f"{table}_tenant"
        op.execute(
            f"""
            CREATE POLICY {policy} ON {schema}.{table}
            FOR ALL TO openvietgis_runtime
            USING ({expression})
            WITH CHECK ({expression})
            """
        )

    op.execute(
        f"""
        CREATE POLICY audit_events_tenant_select ON audit.audit_events
        FOR SELECT TO openvietgis_runtime
        USING ({_tenant_expr()})
        """
    )
    op.execute(
        f"""
        CREATE POLICY audit_events_insert ON audit.audit_events
        FOR INSERT TO openvietgis_runtime
        WITH CHECK (
            {_tenant_expr()}
            OR (
                organization_id IS NULL
                AND actor_type = 'human'
                AND actor_id = NULLIF(
                    current_setting('openvietgis.actor_user_id', true), ''
                )::uuid
            )
        )
        """
    )


def downgrade() -> None:
    op.execute("DROP POLICY IF EXISTS audit_events_insert ON audit.audit_events")
    op.execute("DROP POLICY IF EXISTS audit_events_tenant_select ON audit.audit_events")
    for schema, table in (
        ("org", "invitations"),
        ("org", "resource_grants"),
        ("workspace", "projects"),
        ("workspace", "workspaces"),
    ):
        op.execute(f"DROP POLICY IF EXISTS {table}_tenant ON {schema}.{table}")

    op.execute("DROP POLICY IF EXISTS memberships_tenant_write ON org.organization_memberships")
    op.execute("DROP POLICY IF EXISTS memberships_tenant_or_self ON org.organization_memberships")

    for schema, table in PROTECTED_TABLES:
        op.execute(f'ALTER TABLE "{schema}"."{table}" NO FORCE ROW LEVEL SECURITY')
        op.execute(f'ALTER TABLE "{schema}"."{table}" DISABLE ROW LEVEL SECURITY')

    op.execute("REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA identity FROM openvietgis_runtime")
    op.execute("REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA org FROM openvietgis_runtime")
    op.execute("REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA workspace FROM openvietgis_runtime")
    op.execute("REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA audit FROM openvietgis_runtime")
    op.execute("REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA platform FROM openvietgis_runtime")
    op.execute("REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA foundation FROM openvietgis_runtime")
    op.execute(
        "REVOKE USAGE ON SCHEMA identity, org, workspace, audit, platform, foundation "
        "FROM openvietgis_runtime"
    )
