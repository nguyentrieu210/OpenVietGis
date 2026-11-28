"""Global append-only marketplace trust checkpoints, not tenant data."""
from alembic import op
revision = "0059_plugin_global_trust"
down_revision = "0058_plugin_developer_v2"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE extensions.plugin_trust_checkpoints (
        sequence bigint PRIMARY KEY CHECK(sequence BETWEEN 1 AND 9007199254740991),
        catalog_epoch bigint NOT NULL CHECK(catalog_epoch BETWEEN 1 AND 9007199254740991),
        policy_digest varchar(64) NOT NULL CHECK(policy_digest ~ '^[a-f0-9]{64}$'),
        root_key_id varchar(64) NOT NULL CHECK(root_key_id ~ '^[a-f0-9]{64}$'),
        policy jsonb NOT NULL CHECK(jsonb_typeof(policy)='object' AND octet_length(policy::text)<=1048576),
        accepted_at timestamptz NOT NULL DEFAULT clock_timestamp()
      );
      ALTER TABLE extensions.plugin_trust_checkpoints ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.plugin_trust_checkpoints FORCE ROW LEVEL SECURITY;
      CREATE POLICY global_trust_read ON extensions.plugin_trust_checkpoints FOR SELECT TO openvietgis_runtime USING(true);
      CREATE POLICY global_trust_insert ON extensions.plugin_trust_checkpoints FOR INSERT TO openvietgis_runtime WITH CHECK(true);
      GRANT SELECT,INSERT ON extensions.plugin_trust_checkpoints TO openvietgis_runtime;
    """)

def downgrade():
    op.execute("ALTER TABLE extensions.plugin_trust_checkpoints NO FORCE ROW LEVEL SECURITY")
    if op.get_bind().exec_driver_sql("SELECT EXISTS(SELECT 1 FROM extensions.plugin_trust_checkpoints)").scalar():
        raise RuntimeError("Preserve trust checkpoint evidence before downgrade")
    op.execute("DROP TABLE extensions.plugin_trust_checkpoints")
