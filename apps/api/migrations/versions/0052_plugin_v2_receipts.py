"""Scoped append-only receipts for the declarative read-only plugin runtime."""
from alembic import op

revision = "0052_plugin_v2_receipts"
down_revision = "0051_native_vn2000_export"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
        CREATE TABLE extensions.plugin_v2_receipts (
          id uuid PRIMARY KEY, organization_id uuid NOT NULL, project_id uuid NOT NULL,
          actor_id uuid NOT NULL, request_id uuid NOT NULL, activation_id uuid NOT NULL,
          activation_revision bigint NOT NULL CHECK (activation_revision > 0),
          manifest_id uuid NOT NULL, release_digest varchar(64) NOT NULL CHECK (release_digest ~ '^[a-f0-9]{64}$'),
          request_digest varchar(64) NOT NULL CHECK (request_digest ~ '^[a-f0-9]{64}$'),
          command_id varchar(64) NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
          UNIQUE (organization_id,project_id,actor_id,request_id),
          FOREIGN KEY (organization_id,project_id,activation_id)
            REFERENCES extensions.project_activations(organization_id,project_id,id) ON DELETE RESTRICT,
          FOREIGN KEY (organization_id,manifest_id)
            REFERENCES extensions.manifests(organization_id,id) ON DELETE RESTRICT
        )
    """)
    op.execute("CREATE INDEX plugin_v2_receipts_quota ON extensions.plugin_v2_receipts (organization_id,project_id,actor_id,created_at)")
    op.execute("ALTER TABLE extensions.plugin_v2_receipts ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE extensions.plugin_v2_receipts FORCE ROW LEVEL SECURITY")
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND actor_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id)"""
    op.execute(f"CREATE POLICY plugin_v2_receipt_read ON extensions.plugin_v2_receipts FOR SELECT TO openvietgis_runtime USING ({scope})")
    op.execute(f"CREATE POLICY plugin_v2_receipt_insert ON extensions.plugin_v2_receipts FOR INSERT TO openvietgis_runtime WITH CHECK ({scope})")
    op.execute("GRANT SELECT,INSERT ON extensions.plugin_v2_receipts TO openvietgis_runtime")

def downgrade():
    # Never silently delete an audit trail. The runtime can be disabled without downgrade.
    op.execute("ALTER TABLE extensions.plugin_v2_receipts NO FORCE ROW LEVEL SECURITY")
    bind=op.get_bind()
    if bind.exec_driver_sql("SELECT EXISTS(SELECT 1 FROM extensions.plugin_v2_receipts)").scalar():
        raise RuntimeError("Export and explicitly archive plugin receipts before downgrade")
    op.execute("DROP TABLE extensions.plugin_v2_receipts")
