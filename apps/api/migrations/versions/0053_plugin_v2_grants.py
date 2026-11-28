"""Append-only manager approval of exact signed runtime release and activation revision."""
from alembic import op

revision = "0053_plugin_v2_grants"
down_revision = "0052_plugin_v2_receipts"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
        CREATE TABLE extensions.plugin_v2_grant_events (
          sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
          organization_id uuid NOT NULL, project_id uuid NOT NULL,
          activation_id uuid NOT NULL, activation_revision bigint NOT NULL CHECK (activation_revision > 0),
          manifest_id uuid NOT NULL, actor_id uuid NOT NULL,
          release_digest varchar(64) NOT NULL CHECK (release_digest ~ '^[a-f0-9]{64}$'),
          enabled boolean NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
          FOREIGN KEY (organization_id,project_id,activation_id)
            REFERENCES extensions.project_activations(organization_id,project_id,id) ON DELETE RESTRICT,
          FOREIGN KEY (organization_id,manifest_id)
            REFERENCES extensions.manifests(organization_id,id) ON DELETE RESTRICT
        )
    """)
    op.execute("CREATE INDEX plugin_v2_grants_latest ON extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,sequence DESC)")
    op.execute("ALTER TABLE extensions.plugin_v2_grant_events ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE extensions.plugin_v2_grant_events FORCE ROW LEVEL SECURITY")
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid IS NOT NULL
      AND NOT gis_access.scoped(organization_id,project_id)"""
    op.execute(f"CREATE POLICY plugin_v2_grant_read ON extensions.plugin_v2_grant_events FOR SELECT TO openvietgis_runtime USING ({scope})")
    op.execute(f"""CREATE POLICY plugin_v2_grant_insert ON extensions.plugin_v2_grant_events FOR INSERT TO openvietgis_runtime
      WITH CHECK ({scope} AND actor_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid)""")
    op.execute("GRANT SELECT,INSERT ON extensions.plugin_v2_grant_events TO openvietgis_runtime")
    op.execute("GRANT USAGE ON SEQUENCE extensions.plugin_v2_grant_events_sequence_seq TO openvietgis_runtime")

def downgrade():
    op.execute("ALTER TABLE extensions.plugin_v2_grant_events NO FORCE ROW LEVEL SECURITY")
    if op.get_bind().exec_driver_sql("SELECT EXISTS(SELECT 1 FROM extensions.plugin_v2_grant_events)").scalar():
        raise RuntimeError("Archive plugin grant audit before downgrade")
    op.execute("DROP TABLE extensions.plugin_v2_grant_events")
