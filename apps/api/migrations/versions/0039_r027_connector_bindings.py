"""R027 connector binding registry: disabled draft metadata only, never sync credentials.

New isolated migration, NOT applied to production by this change. No agent
authorization, external connection or write grant is introduced.
"""

from alembic import op

revision = "0039_r027_connector_bindings"
down_revision = "0038_consolidated_feature_index"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("CREATE SCHEMA IF NOT EXISTS integration_hub")
    op.execute("GRANT USAGE ON SCHEMA integration_hub TO openvietgis_runtime")
    op.execute("""
        CREATE TABLE integration_hub.connector_bindings (
            id uuid NOT NULL PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            layer_id uuid NOT NULL,
            adapter_key varchar(32) NOT NULL DEFAULT 'qgis'
                CHECK (adapter_key = 'qgis'),
            external_dataset_ref varchar(128) NOT NULL
                CHECK (external_dataset_ref = btrim(external_dataset_ref)
                       AND length(external_dataset_ref) BETWEEN 1 AND 128),
            direction varchar(32) NOT NULL DEFAULT 'bidirectional_manual'
                CHECK (direction = 'bidirectional_manual'),
            source_crs_code integer NOT NULL,
            target_crs_code integer NOT NULL
                CHECK (target_crs_code = source_crs_code),
            external_primary_key varchar(64) NOT NULL DEFAULT 'ovg_feature_id'
                CHECK (external_primary_key = 'ovg_feature_id'),
            allowed_field_ids jsonb NOT NULL DEFAULT '[]'::jsonb
                CHECK (jsonb_typeof(allowed_field_ids) = 'array'),
            base_content_revision varchar(64) NOT NULL
                CHECK (base_content_revision ~ '^[0-9a-f]{64}$'),
            base_schema_revision varchar(64) NOT NULL
                CHECK (base_schema_revision ~ '^[0-9a-f]{64}$'),
            state varchar(16) NOT NULL DEFAULT 'draft'
                CHECK (state IN ('draft', 'paused', 'revoked')),
            revision integer NOT NULL DEFAULT 1 CHECK (revision >= 1),
            created_by uuid NOT NULL REFERENCES identity.users (id),
            created_at timestamptz NOT NULL DEFAULT now(),
            updated_at timestamptz NOT NULL DEFAULT now(),
            CONSTRAINT uq_r027_binding_scope UNIQUE(organization_id, project_id, id),
            CONSTRAINT fk_r027_binding_layer FOREIGN KEY
                (organization_id, project_id, layer_id)
                REFERENCES spatial.layers (organization_id, project_id, id)
                ON DELETE CASCADE
        )
    """)
    op.execute("""
        CREATE INDEX ix_r027_bindings_layer
        ON integration_hub.connector_bindings
        (organization_id, project_id, layer_id, created_at DESC)
    """)
    op.execute("""
        CREATE UNIQUE INDEX uq_r027_active_binding_identity
        ON integration_hub.connector_bindings
          (organization_id, project_id, layer_id, adapter_key, external_dataset_ref)
        WHERE state <> 'revoked'
    """)
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        ENABLE ROW LEVEL SECURITY
    """)
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        FORCE ROW LEVEL SECURITY
    """)
    scope = """organization_id = NULLIF(
        current_setting('openvietgis.organization_id', true), '')::uuid
        AND project_id = NULLIF(
        current_setting('openvietgis.project_id', true), '')::uuid
        AND NOT gis_access.scoped(organization_id, project_id)"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id', true), '')::uuid"
    op.execute(f"""
        CREATE POLICY connector_bindings_read
        ON integration_hub.connector_bindings FOR SELECT TO openvietgis_runtime
        USING ({scope} AND {actor} IS NOT NULL)
    """)
    op.execute(f"""
        CREATE POLICY connector_bindings_insert
        ON integration_hub.connector_bindings FOR INSERT TO openvietgis_runtime
        WITH CHECK ({scope} AND created_by = {actor})
    """)
    op.execute(f"""
        CREATE POLICY connector_bindings_update
        ON integration_hub.connector_bindings FOR UPDATE TO openvietgis_runtime
        USING ({scope} AND {actor} IS NOT NULL)
        WITH CHECK ({scope} AND {actor} IS NOT NULL)
    """)
    op.execute("""
        GRANT SELECT, INSERT ON integration_hub.connector_bindings
        TO openvietgis_runtime
    """)
    # Narrow UPDATE only for lifecycle fields. No code path can mutate binding
    # scope, ACL mapping, credentials, revisions or identifiers via SQL role.
    op.execute("""
        GRANT UPDATE(state, revision, updated_at)
        ON integration_hub.connector_bindings TO openvietgis_runtime
    """)


def downgrade() -> None:
    # Fail closed if newer migrations have other integration_hub objects.
    op.execute("DROP TABLE integration_hub.connector_bindings")
    op.execute("DROP SCHEMA integration_hub")