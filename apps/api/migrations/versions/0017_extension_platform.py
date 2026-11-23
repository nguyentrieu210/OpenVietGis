"""R014 Internal Extension / Module Platform schema (generic, zero vertical entities)."""

from __future__ import annotations

from alembic import op

revision = "0017_extension_platform"
down_revision = "0016_field_offline"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("CREATE SCHEMA extensions")
    op.execute("GRANT USAGE ON SCHEMA extensions TO openvietgis_runtime")

    op.execute("""
        CREATE TABLE extensions.manifests (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            extension_key varchar(100) NOT NULL,
            version varchar(32) NOT NULL,
            api_compatibility varchar(32) NOT NULL,
            display_name varchar(200) NOT NULL,
            description varchar(500) NOT NULL DEFAULT '',
            permissions jsonb NOT NULL,
            commands jsonb NOT NULL,
            capabilities jsonb NOT NULL,
            entity_schemas jsonb NOT NULL,
            enabled boolean NOT NULL DEFAULT true,
            registered_by_actor_id uuid NOT NULL,
            created_at timestamptz NOT NULL,
            CONSTRAINT uq_ext_manifest_ver UNIQUE (organization_id, extension_key, version),
            CONSTRAINT ck_ext_key CHECK (extension_key ~ '^[a-z][a-z0-9_-]{1,99}$'),
            CONSTRAINT fk_ext_manifest_org
                FOREIGN KEY (organization_id)
                REFERENCES org.organizations(id)
                ON DELETE RESTRICT
        )
    """)

    op.execute("""
        CREATE TABLE extensions.project_activations (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            manifest_id uuid NOT NULL,
            extension_key varchar(100) NOT NULL,
            status varchar(32) NOT NULL DEFAULT 'active',
            activated_by_actor_id uuid NOT NULL,
            activated_at timestamptz NOT NULL,
            updated_at timestamptz NOT NULL,
            CONSTRAINT uq_ext_proj_act UNIQUE (organization_id, project_id, extension_key),
            CONSTRAINT ck_ext_act_status CHECK (status IN ('active', 'disabled')),
            CONSTRAINT fk_ext_act_manifest
                FOREIGN KEY (manifest_id)
                REFERENCES extensions.manifests(id)
                ON DELETE RESTRICT,
            CONSTRAINT fk_ext_act_project
                FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects(organization_id, id)
                ON DELETE RESTRICT
        )
    """)

    op.execute("""
        CREATE TABLE extensions.records (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            extension_key varchar(100) NOT NULL,
            entity_name varchar(64) NOT NULL,
            payload jsonb NOT NULL,
            linked_layer_id uuid NULL,
            linked_feature_id varchar(128) NULL,
            created_by_actor_id uuid NOT NULL,
            created_at timestamptz NOT NULL,
            updated_at timestamptz NOT NULL,
            CONSTRAINT fk_ext_rec_project
                FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects(organization_id, id)
                ON DELETE RESTRICT
        )
    """)

    op.execute("ALTER TABLE extensions.manifests ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE extensions.manifests FORCE ROW LEVEL SECURITY")
    op.execute("""
        CREATE POLICY ext_manifests_org_scope ON extensions.manifests
        FOR ALL TO openvietgis_runtime
        USING (
            organization_id = NULLIF(
                current_setting('openvietgis.organization_id', true), ''
            )::uuid
        )
        WITH CHECK (
            organization_id = NULLIF(
                current_setting('openvietgis.organization_id', true), ''
            )::uuid
        )
    """)

    for table in ("project_activations", "records"):
        op.execute(f"ALTER TABLE extensions.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE extensions.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""
            CREATE POLICY ext_{table}_project_scope ON extensions.{table}
            FOR ALL TO openvietgis_runtime
            USING (
                organization_id = NULLIF(
                    current_setting('openvietgis.organization_id', true), ''
                )::uuid
                AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
            )
            WITH CHECK (
                organization_id = NULLIF(
                    current_setting('openvietgis.organization_id', true), ''
                )::uuid
                AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
            )
        """)

    op.execute("GRANT SELECT, INSERT, UPDATE ON extensions.manifests TO openvietgis_runtime")
    op.execute(
        "GRANT SELECT, INSERT, UPDATE ON extensions.project_activations TO openvietgis_runtime"
    )
    op.execute("GRANT SELECT, INSERT, UPDATE, DELETE ON extensions.records TO openvietgis_runtime")


def downgrade() -> None:
    op.execute("DROP TABLE extensions.records")
    op.execute("DROP TABLE extensions.project_activations")
    op.execute("DROP TABLE extensions.manifests")
    op.execute("DROP SCHEMA extensions")
