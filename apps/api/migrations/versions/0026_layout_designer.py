"""Versioned project-scoped print designer templates."""
from alembic import op
revision="0026_layout_designer"
down_revision="0025_processing_models"
branch_labels=None
depends_on=None

def upgrade():
    op.execute("""CREATE TABLE cartography.designer_layouts (
       organization_id uuid NOT NULL,project_id uuid NOT NULL,id uuid NOT NULL,
       name varchar(120) NOT NULL,document jsonb NOT NULL CHECK(octet_length(document::text)<=120000),
       revision integer NOT NULL DEFAULT 1 CHECK(revision>=1),
       created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),
       PRIMARY KEY(organization_id,project_id,id),
       FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
    )""")
    op.execute("ALTER TABLE cartography.designer_layouts ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE cartography.designer_layouts FORCE ROW LEVEL SECURITY")
    op.execute("""CREATE POLICY scoped_designer ON cartography.designer_layouts FOR ALL TO openvietgis_runtime
       USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
       AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
       AND NOT gis_access.scoped(organization_id,project_id))
       WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
       AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
       AND NOT gis_access.scoped(organization_id,project_id))""")
    op.execute("GRANT SELECT,INSERT,UPDATE,DELETE ON cartography.designer_layouts TO openvietgis_runtime")

def downgrade():
    op.execute("DROP TABLE cartography.designer_layouts")
