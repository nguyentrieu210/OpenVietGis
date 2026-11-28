"""Immutable graph installation and project switch receipts, scoped to existing registry."""
from alembic import op
revision = "0056_plugin_graph_operations"
down_revision = "0055_plugin_sandbox_commerce"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
    CREATE TABLE extensions.plugin_graph_installs (
      organization_id uuid NOT NULL REFERENCES org.organizations(id) ON DELETE RESTRICT,
      request_id uuid NOT NULL, actor_id uuid NOT NULL,
      plan_digest varchar(64) NOT NULL CHECK(plan_digest ~ '^[a-f0-9]{64}$'),
      catalog_epoch bigint NOT NULL CHECK(catalog_epoch BETWEEN 1 AND 9007199254740991),
      receipt jsonb NOT NULL CHECK(jsonb_typeof(receipt)='object' AND octet_length(receipt::text)<=262144),
      created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      PRIMARY KEY(organization_id,request_id)
    )
    """)
    op.execute("""
    CREATE TABLE extensions.plugin_graph_switches (
      organization_id uuid NOT NULL, project_id uuid NOT NULL, request_id uuid NOT NULL,
      actor_id uuid NOT NULL, plan_digest varchar(64) NOT NULL CHECK(plan_digest ~ '^[a-f0-9]{64}$'),
      catalog_epoch bigint NOT NULL CHECK(catalog_epoch BETWEEN 1 AND 9007199254740991),
      receipt jsonb NOT NULL CHECK(jsonb_typeof(receipt)='object' AND octet_length(receipt::text)<=262144),
      created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      PRIMARY KEY(organization_id,project_id,request_id),
      FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE RESTRICT
    )
    """)
    org = "organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid"
    human = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid IS NOT NULL"
    actor = "actor_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
    for table, project in (
        ("plugin_graph_installs", "NULLIF(current_setting('openvietgis.project_id',true),'') IS NULL"),
        ("plugin_graph_switches", "project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid AND NOT gis_access.scoped(organization_id,project_id)"),
    ):
        scope = f"{org} AND {human} AND {project}"
        op.execute(f"ALTER TABLE extensions.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE extensions.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"CREATE POLICY graph_read ON extensions.{table} FOR SELECT TO openvietgis_runtime USING ({scope})")
        op.execute(f"CREATE POLICY graph_insert ON extensions.{table} FOR INSERT TO openvietgis_runtime WITH CHECK ({scope} AND {actor})")
        op.execute(f"GRANT SELECT,INSERT ON extensions.{table} TO openvietgis_runtime")
    op.execute("CREATE INDEX plugin_graph_installs_epoch ON extensions.plugin_graph_installs(organization_id,catalog_epoch DESC)")
    op.execute("CREATE INDEX plugin_graph_switches_epoch ON extensions.plugin_graph_switches(organization_id,project_id,catalog_epoch DESC)")

def downgrade():
    for table in ("plugin_graph_switches","plugin_graph_installs"):
        op.execute(f"ALTER TABLE extensions.{table} NO FORCE ROW LEVEL SECURITY")
        if op.get_bind().exec_driver_sql(f"SELECT EXISTS(SELECT 1 FROM extensions.{table})").scalar():
            raise RuntimeError("Archive plugin graph operation receipts before downgrade")
        op.execute(f"DROP TABLE extensions.{table}")
