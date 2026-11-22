"""Project-scoped reusable GIS processing models."""
from alembic import op
revision = "0025_processing_models"
down_revision = "0024_raster_layers"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE processing.models (
        organization_id uuid NOT NULL,project_id uuid NOT NULL,id uuid NOT NULL,
        name varchar(120) NOT NULL, graph jsonb NOT NULL CHECK(octet_length(graph::text)<=120000),
        revision integer NOT NULL DEFAULT 1 CHECK(revision>=1),
        created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
      )
    """)
    op.execute("ALTER TABLE processing.models ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE processing.models FORCE ROW LEVEL SECURITY")
    op.execute("""CREATE POLICY scoped_models ON processing.models FOR ALL TO openvietgis_runtime
      USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id))
      WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id))""")
    op.execute("GRANT SELECT,INSERT,UPDATE,DELETE ON processing.models TO openvietgis_runtime")

def downgrade():
    op.execute("DROP TABLE processing.models")
