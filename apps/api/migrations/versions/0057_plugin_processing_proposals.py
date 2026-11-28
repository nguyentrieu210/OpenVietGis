"""Immutable human-review proposal and approval receipt; uses canonical processing.jobs."""
from alembic import op
revision = "0057_plugin_processing_proposals"
down_revision = "0056_plugin_graph_operations"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE extensions.plugin_processing_proposals (
        id uuid PRIMARY KEY,
        organization_id uuid NOT NULL, project_id uuid NOT NULL, actor_id uuid NOT NULL,
        manifest_id uuid NOT NULL, activation_id uuid NOT NULL, activation_revision bigint NOT NULL CHECK(activation_revision>0),
        release_digest varchar(64) NOT NULL CHECK(release_digest ~ '^[a-f0-9]{64}$'),
        lock_digest varchar(64) NOT NULL CHECK(lock_digest ~ '^[a-f0-9]{64}$'),
        grant_revision bigint NOT NULL CHECK(grant_revision>0), catalog_epoch bigint NOT NULL CHECK(catalog_epoch>0),
        plan jsonb NOT NULL CHECK(jsonb_typeof(plan)='object' AND octet_length(plan::text)<=16384),
        plan_digest varchar(64) NOT NULL CHECK(plan_digest ~ '^[a-f0-9]{64}$'),
        created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
        UNIQUE(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,activation_id) REFERENCES extensions.project_activations(organization_id,project_id,id) ON DELETE RESTRICT,
        FOREIGN KEY(organization_id,manifest_id) REFERENCES extensions.manifests(organization_id,id) ON DELETE RESTRICT
      )
    """)
    op.execute("""
      CREATE TABLE extensions.plugin_processing_approvals (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, proposal_id uuid NOT NULL,
        actor_id uuid NOT NULL, job_id uuid NOT NULL, approved_at timestamptz NOT NULL DEFAULT clock_timestamp(),
        PRIMARY KEY(organization_id,project_id,proposal_id),
        FOREIGN KEY(organization_id,project_id,proposal_id) REFERENCES extensions.plugin_processing_proposals(organization_id,project_id,id) ON DELETE RESTRICT
      )
    """)
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND actor_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id)"""
    for table in ("plugin_processing_proposals","plugin_processing_approvals"):
        op.execute(f"ALTER TABLE extensions.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE extensions.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"CREATE POLICY processing_read ON extensions.{table} FOR SELECT TO openvietgis_runtime USING ({scope})")
        op.execute(f"CREATE POLICY processing_insert ON extensions.{table} FOR INSERT TO openvietgis_runtime WITH CHECK ({scope})")
        op.execute(f"GRANT SELECT,INSERT ON extensions.{table} TO openvietgis_runtime")

def downgrade():
    for table in ("plugin_processing_approvals","plugin_processing_proposals"):
        op.execute(f"ALTER TABLE extensions.{table} NO FORCE ROW LEVEL SECURITY")
        if op.get_bind().exec_driver_sql(f"SELECT EXISTS(SELECT 1 FROM extensions.{table})").scalar():
            raise RuntimeError("Archive plugin processing evidence before downgrade")
        op.execute(f"DROP TABLE extensions.{table}")
