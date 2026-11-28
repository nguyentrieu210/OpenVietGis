"""Persistent synthetic commerce; no live payment or runtime entitlement authority."""
from alembic import op
revision = "0055_plugin_sandbox_commerce"
down_revision = "0054_plugin_v2_dependency_lock"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
    CREATE TABLE extensions.plugin_sandbox_orders (
      organization_id uuid NOT NULL REFERENCES org.organizations(id) ON DELETE RESTRICT,
      id uuid NOT NULL, actor_id uuid NOT NULL,
      release_digest varchar(64) NOT NULL CHECK(release_digest ~ '^[a-f0-9]{64}$'),
      quote jsonb NOT NULL CHECK(jsonb_typeof(quote)='object' AND octet_length(quote::text)<=4096),
      created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      PRIMARY KEY(organization_id,id)
    )
    """)
    op.execute("""
    CREATE TABLE extensions.plugin_sandbox_events (
      organization_id uuid NOT NULL, order_id uuid NOT NULL,
      sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
      provider_event_id varchar(128) NOT NULL,
      payload jsonb NOT NULL CHECK(jsonb_typeof(payload)='object' AND octet_length(payload::text)<=4096),
      signature varchar(88) NOT NULL CHECK(length(signature)=88),
      receipt jsonb NOT NULL CHECK(jsonb_typeof(receipt)='object' AND octet_length(receipt::text)<=4096),
      recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      UNIQUE(organization_id,provider_event_id),
      FOREIGN KEY(organization_id,order_id) REFERENCES extensions.plugin_sandbox_orders(organization_id,id) ON DELETE RESTRICT
    )
    """)
    op.execute("CREATE INDEX plugin_sandbox_events_order ON extensions.plugin_sandbox_events(organization_id,order_id,sequence)")
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid IS NOT NULL
      AND NULLIF(current_setting('openvietgis.project_id',true),'') IS NULL"""
    for table in ("plugin_sandbox_orders","plugin_sandbox_events"):
        op.execute(f"ALTER TABLE extensions.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE extensions.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"CREATE POLICY sandbox_read ON extensions.{table} FOR SELECT TO openvietgis_runtime USING ({scope})")
        actor = " AND actor_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid" if table.endswith("orders") else ""
        op.execute(f"CREATE POLICY sandbox_insert ON extensions.{table} FOR INSERT TO openvietgis_runtime WITH CHECK ({scope}{actor})")
        op.execute(f"GRANT SELECT,INSERT ON extensions.{table} TO openvietgis_runtime")
    op.execute("GRANT USAGE ON SEQUENCE extensions.plugin_sandbox_events_sequence_seq TO openvietgis_runtime")

def downgrade():
    for table in ("plugin_sandbox_orders","plugin_sandbox_events"):
        op.execute(f"ALTER TABLE extensions.{table} NO FORCE ROW LEVEL SECURITY")
        if op.get_bind().exec_driver_sql(f"SELECT EXISTS(SELECT 1 FROM extensions.{table})").scalar():
            raise RuntimeError("Archive sandbox accounting records before downgrade")
    op.execute("DROP TABLE extensions.plugin_sandbox_events")
    op.execute("DROP TABLE extensions.plugin_sandbox_orders")
