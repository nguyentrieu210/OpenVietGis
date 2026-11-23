"""Versioned project-scoped print designer templates."""

from alembic import op

revision = "0026_layout_designer"
down_revision = "0025_processing_models"
branch_labels = None
depends_on = None


def upgrade():
    op.execute(
        "CREATE TABLE cartography.designer_layouts (\n       "
        "organization_id uuid NOT NULL,project_id uuid NOT NULL,id "
        "uuid NOT NULL,\n       name varchar(120) NOT NULL,document "
        "jsonb NOT NULL CHECK(octet_length(document::text)<=120000),\n"
        "       revision integer NOT NULL DEFAULT 1 "
        "CHECK(revision>=1),\n       created_at timestamptz NOT NULL "
        "DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT "
        "now(),\n       PRIMARY KEY(organization_id,project_id,id),\n  "
        "     FOREIGN KEY(organization_id,project_id) REFERENCES "
        "workspace.projects(organization_id,id) ON DELETE CASCADE\n   "
        " )"
    )
    op.execute("ALTER TABLE cartography.designer_layouts ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE cartography.designer_layouts FORCE ROW LEVEL SECURITY")
    op.execute(
        "CREATE POLICY scoped_designer ON cartography.designer_layout"
        "s FOR ALL TO openvietgis_runtime\n       USING(organization_i"
        "d=NULLIF(current_setting('openvietgis.organization_id',true)"
        ",'')::uuid\n       AND project_id=NULLIF(current_setting('ope"
        "nvietgis.project_id',true),'')::uuid\n       AND NOT "
        "gis_access.scoped(organization_id,project_id))\n       WITH "
        "CHECK(organization_id=NULLIF(current_setting('openvietgis.or"
        "ganization_id',true),'')::uuid\n       AND "
        "project_id=NULLIF(current_setting('openvietgis.project_id',t"
        "rue),'')::uuid\n       AND NOT gis_access.scoped(organization"
        "_id,project_id))"
    )
    op.execute(
        "GRANT SELECT,INSERT,UPDATE,DELETE ON cartography.designer_layouts TO openvietgis_runtime"
    )


def downgrade():
    op.execute("DROP TABLE cartography.designer_layouts")
