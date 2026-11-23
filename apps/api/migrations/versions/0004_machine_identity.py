"""Add tenant-scoped service principals and credentials."""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "0004_machine_identity"
down_revision: str | Sequence[str] | None = "0003a_identity_unlink_security"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

UUID = postgresql.UUID(as_uuid=True)

# Frozen Spec 002 vocabulary. New permissions require their own migration;
# importing a live enum would change the meaning of this historical revision.
PERMISSION_VALUES = (
    "organization.read",
    "organization.manage_members",
    "organization.manage_settings",
    "organization.lifecycle",
    "organization.owner.transfer",
    "organization.delete",
    "authorization.manage_grants",
    "invitation.manage",
    "service_principal.manage",
    "audit.read",
    "workspace.read",
    "workspace.manage",
    "project.read",
    "project.edit",
    "project.manage",
    "project.move_workspace",
    "project.archive",
    "project.delete",
    "project.share",
)


def _tenant_expr() -> str:
    return (
        "organization_id = NULLIF(current_setting('openvietgis.organization_id', true), '')::uuid"
    )


def upgrade() -> None:
    op.create_table(
        "service_principals",
        sa.Column("id", UUID, primary_key=True, nullable=False),
        sa.Column("organization_id", UUID, nullable=False),
        sa.Column("name", sa.String(length=200), nullable=False),
        sa.Column("description", sa.Text(), nullable=True),
        sa.Column("status", sa.String(length=32), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("created_by_actor_type", sa.String(length=32), nullable=False),
        sa.Column("created_by_actor_id", UUID, nullable=True),
        sa.Column("revoked_at", sa.DateTime(timezone=True), nullable=True),
        sa.ForeignKeyConstraint(
            ["organization_id"],
            ["org.organizations.id"],
            name="fk_service_principals_organization",
            ondelete="CASCADE",
        ),
        sa.UniqueConstraint(
            "organization_id", "id", name="uq_service_principals_organization_id_id"
        ),
        sa.UniqueConstraint(
            "organization_id", "name", name="uq_service_principals_organization_name"
        ),
        sa.CheckConstraint(
            "status IN ('active','revoked')",
            name="ck_service_principals_status",
        ),
        sa.CheckConstraint(
            "(status = 'revoked') = (revoked_at IS NOT NULL)",
            name="ck_service_principals_revoked_state",
        ),
        sa.CheckConstraint(
            "created_by_actor_type IN ('human','service_principal','system')",
            name="ck_service_principals_actor_type",
        ),
        sa.CheckConstraint(
            "created_by_actor_type = 'system' OR created_by_actor_id IS NOT NULL",
            name="ck_service_principals_actor_id",
        ),
        schema="org",
    )

    permission_values = ",".join("'" + permission + "'" for permission in PERMISSION_VALUES)
    op.create_table(
        "service_credentials",
        sa.Column("id", UUID, primary_key=True, nullable=False),
        sa.Column("organization_id", UUID, nullable=False),
        sa.Column("service_principal_id", UUID, nullable=False),
        sa.Column("name", sa.String(length=200), nullable=False),
        sa.Column("prefix", sa.String(length=64), nullable=False),
        sa.Column("secret_hash", sa.LargeBinary(length=32), nullable=False),
        sa.Column("permission_ceiling", postgresql.ARRAY(sa.Text()), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("expires_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("last_used_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("revoked_at", sa.DateTime(timezone=True), nullable=True),
        sa.ForeignKeyConstraint(
            ["organization_id", "service_principal_id"],
            [
                "org.service_principals.organization_id",
                "org.service_principals.id",
            ],
            name="fk_service_credentials_principal_same_org",
            ondelete="CASCADE",
        ),
        sa.UniqueConstraint(
            "organization_id", "id", name="uq_service_credentials_organization_id_id"
        ),
        sa.UniqueConstraint("prefix", name="uq_service_credentials_prefix"),
        sa.CheckConstraint(
            "octet_length(secret_hash) = 32",
            name="ck_service_credentials_hash_length",
        ),
        sa.CheckConstraint(
            "length(prefix) >= 8",
            name="ck_service_credentials_prefix_length",
        ),
        sa.CheckConstraint(
            "expires_at IS NULL OR expires_at > created_at",
            name="ck_service_credentials_expiry",
        ),
        sa.CheckConstraint(
            f"permission_ceiling IS NULL OR "
            f"permission_ceiling <@ ARRAY[{permission_values}]::text[]",
            name="ck_service_credentials_permission_ceiling",
        ),
        schema="org",
    )

    op.create_foreign_key(
        "fk_resource_grants_service_principal_same_org",
        "resource_grants",
        "service_principals",
        ["organization_id", "service_principal_id"],
        ["organization_id", "id"],
        source_schema="org",
        referent_schema="org",
        ondelete="CASCADE",
    )

    for table in ("service_principals", "service_credentials"):
        op.execute(f"GRANT SELECT, INSERT, UPDATE ON org.{table} TO openvietgis_runtime")
        op.execute(f"ALTER TABLE org.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE org.{table} FORCE ROW LEVEL SECURITY")
        op.execute(
            f"""
            CREATE POLICY {table}_tenant ON org.{table}
            FOR ALL TO openvietgis_runtime
            USING ({_tenant_expr()})
            WITH CHECK ({_tenant_expr()})
            """
        )


def downgrade() -> None:
    # This is a destructive rollback of P2 service identity. Retain all human
    # grants; remove service grants before dropping their referenced principals.
    op.execute("DELETE FROM org.resource_grants WHERE subject_kind = 'service_principal'")
    op.drop_constraint(
        "fk_resource_grants_service_principal_same_org",
        "resource_grants",
        schema="org",
        type_="foreignkey",
    )
    for table in ("service_credentials", "service_principals"):
        op.execute(f"DROP POLICY IF EXISTS {table}_tenant ON org.{table}")
        op.execute(f"ALTER TABLE org.{table} NO FORCE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE org.{table} DISABLE ROW LEVEL SECURITY")
        op.execute(f"REVOKE ALL PRIVILEGES ON org.{table} FROM openvietgis_runtime")
    op.drop_table("service_credentials", schema="org")
    op.drop_table("service_principals", schema="org")
