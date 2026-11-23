"""R027 Pha A & Pha B: binding activation, pairing token ledger, and conflict resolution ledger.

Strictly additive migration (backward-compatible with 0039):
- Allows 'active' state on integration_hub.connector_bindings + activated_at / last_paired_at.
- Creates integration_hub.binding_pairing_tokens (stores SHA-256 hash + service_credential_id,
  never raw secret tokens) with strict tenant/project RLS.
- Creates integration_hub.sync_conflicts (per-feature baseline/web/qgis diff & resolution)
  with strict tenant/project RLS.
"""

from alembic import op

revision = "0040_r027_pairing_and_conflicts"
down_revision = "0039_r027_connector_bindings"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # 1. Extend connector_bindings state check to allow 'active' and add audit timestamps.
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        DROP CONSTRAINT IF EXISTS connector_bindings_state_check
    """)
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        ADD CONSTRAINT connector_bindings_state_check
        CHECK (state IN ('draft', 'active', 'paused', 'revoked'))
    """)
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        ADD COLUMN IF NOT EXISTS activated_at timestamptz,
        ADD COLUMN IF NOT EXISTS last_paired_at timestamptz,
        ADD COLUMN IF NOT EXISTS service_principal_id uuid
    """)
    op.execute("""
        GRANT UPDATE(
            state, revision, updated_at, activated_at, last_paired_at, service_principal_id
        )
        ON integration_hub.connector_bindings TO openvietgis_runtime
    """)

    scope = """organization_id = NULLIF(
        current_setting('openvietgis.organization_id', true), '')::uuid
        AND project_id = NULLIF(
        current_setting('openvietgis.project_id', true), '')::uuid
        AND NOT gis_access.scoped(organization_id, project_id)"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id', true), '')::uuid"

    # 2. Pairing tokens table (metadata + SHA-256 hash only; raw secret is never stored).
    op.execute("""
        CREATE TABLE integration_hub.binding_pairing_tokens (
            id uuid NOT NULL PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            binding_id uuid NOT NULL,
            label varchar(120) NOT NULL DEFAULT 'QGIS Desktop',
            token_prefix varchar(24) NOT NULL,
            token_hash varchar(64) NOT NULL,
            service_principal_id uuid NOT NULL,
            service_credential_id uuid NOT NULL,
            expires_at timestamptz NOT NULL,
            last_used_at timestamptz,
            revoked_at timestamptz,
            created_by uuid NOT NULL REFERENCES identity.users (id),
            created_at timestamptz NOT NULL DEFAULT now(),
            CONSTRAINT fk_r027_pairing_binding FOREIGN KEY
                (organization_id, project_id, binding_id)
                REFERENCES integration_hub.connector_bindings (organization_id, project_id, id)
                ON DELETE CASCADE
        )
    """)
    op.execute("""
        CREATE INDEX ix_r027_pairing_tokens_binding
        ON integration_hub.binding_pairing_tokens
        (organization_id, project_id, binding_id, created_at DESC)
    """)
    op.execute("""
        ALTER TABLE integration_hub.binding_pairing_tokens
        ENABLE ROW LEVEL SECURITY
    """)
    op.execute("""
        ALTER TABLE integration_hub.binding_pairing_tokens
        FORCE ROW LEVEL SECURITY
    """)
    op.execute(f"""
        CREATE POLICY pairing_tokens_read
        ON integration_hub.binding_pairing_tokens FOR SELECT TO openvietgis_runtime
        USING ({scope} AND {actor} IS NOT NULL)
    """)
    op.execute(f"""
        CREATE POLICY pairing_tokens_insert
        ON integration_hub.binding_pairing_tokens FOR INSERT TO openvietgis_runtime
        WITH CHECK ({scope} AND created_by = {actor})
    """)
    op.execute(f"""
        CREATE POLICY pairing_tokens_update
        ON integration_hub.binding_pairing_tokens FOR UPDATE TO openvietgis_runtime
        USING ({scope} AND {actor} IS NOT NULL)
        WITH CHECK ({scope} AND {actor} IS NOT NULL)
    """)
    op.execute("""
        GRANT SELECT, INSERT ON integration_hub.binding_pairing_tokens
        TO openvietgis_runtime
    """)
    op.execute("""
        GRANT UPDATE(last_used_at, revoked_at)
        ON integration_hub.binding_pairing_tokens TO openvietgis_runtime
    """)

    # 3. Conflict resolution ledger (Pha B).
    op.execute("""
        CREATE TABLE integration_hub.sync_conflicts (
            id uuid NOT NULL PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            binding_id uuid NOT NULL,
            layer_id uuid NOT NULL,
            feature_id varchar(128) NOT NULL,
            conflict_kind varchar(32) NOT NULL
                CHECK (conflict_kind IN ('attribute', 'geometry', 'delete_candidate')),
            field_name varchar(128),
            baseline_value jsonb,
            web_value jsonb,
            qgis_value jsonb,
            status varchar(24) NOT NULL DEFAULT 'open'
                CHECK (status IN (
                    'open', 'resolved_web', 'resolved_qgis', 'resolved_merged', 'dismissed'
                )),
            resolved_value jsonb,
            resolution_note text,
            created_by uuid,
            resolved_by uuid REFERENCES identity.users (id),
            created_at timestamptz NOT NULL DEFAULT now(),
            resolved_at timestamptz,
            CONSTRAINT fk_r027_conflict_binding FOREIGN KEY
                (organization_id, project_id, binding_id)
                REFERENCES integration_hub.connector_bindings (organization_id, project_id, id)
                ON DELETE CASCADE
        )
    """)
    op.execute("""
        CREATE INDEX ix_r027_conflicts_binding
        ON integration_hub.sync_conflicts
        (organization_id, project_id, binding_id, status, created_at DESC)
    """)
    op.execute("""
        ALTER TABLE integration_hub.sync_conflicts
        ENABLE ROW LEVEL SECURITY
    """)
    op.execute("""
        ALTER TABLE integration_hub.sync_conflicts
        FORCE ROW LEVEL SECURITY
    """)
    op.execute(f"""
        CREATE POLICY sync_conflicts_read
        ON integration_hub.sync_conflicts FOR SELECT TO openvietgis_runtime
        USING ({scope})
    """)
    op.execute(f"""
        CREATE POLICY sync_conflicts_insert
        ON integration_hub.sync_conflicts FOR INSERT TO openvietgis_runtime
        WITH CHECK ({scope})
    """)
    op.execute(f"""
        CREATE POLICY sync_conflicts_update
        ON integration_hub.sync_conflicts FOR UPDATE TO openvietgis_runtime
        USING ({scope} AND {actor} IS NOT NULL)
        WITH CHECK ({scope} AND {actor} IS NOT NULL)
    """)
    op.execute("""
        GRANT SELECT, INSERT ON integration_hub.sync_conflicts
        TO openvietgis_runtime
    """)
    op.execute("""
        GRANT UPDATE(status, resolved_value, resolution_note, resolved_by, resolved_at)
        ON integration_hub.sync_conflicts TO openvietgis_runtime
    """)


def downgrade() -> None:
    op.execute("DROP TABLE IF EXISTS integration_hub.sync_conflicts")
    op.execute("DROP TABLE IF EXISTS integration_hub.binding_pairing_tokens")
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        DROP COLUMN IF EXISTS service_principal_id,
        DROP COLUMN IF EXISTS last_paired_at,
        DROP COLUMN IF EXISTS activated_at
    """)
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        DROP CONSTRAINT IF EXISTS connector_bindings_state_check
    """)
    op.execute("""
        ALTER TABLE integration_hub.connector_bindings
        ADD CONSTRAINT connector_bindings_state_check
        CHECK (state IN ('draft', 'paused', 'revoked'))
    """)
