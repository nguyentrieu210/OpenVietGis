"""Project-scoped reusable GIS processing models."""

from alembic import op

revision = "0025_processing_models"
down_revision = "0024_raster_layers"
branch_labels = None
depends_on = None


def upgrade():
    op.execute(
        "\n      CREATE TABLE processing.models (\n        "
        "organization_id uuid NOT NULL,project_id uuid NOT NULL,id "
        "uuid NOT NULL,\n        name varchar(120) NOT NULL, graph "
        "jsonb NOT NULL CHECK(octet_length(graph::text)<=120000),\n   "
        "     revision integer NOT NULL DEFAULT 1 CHECK(revision>=1),"
        "\n        created_at timestamptz NOT NULL DEFAULT now(), "
        "updated_at timestamptz NOT NULL DEFAULT now(),\n        "
        "PRIMARY KEY(organization_id,project_id,id),\n        FOREIGN "
        "KEY(organization_id,project_id) REFERENCES "
        "workspace.projects(organization_id,id) ON DELETE CASCADE\n   "
        "   )\n    "
    )
    op.execute("ALTER TABLE processing.models ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE processing.models FORCE ROW LEVEL SECURITY")
    op.execute(
        "CREATE POLICY scoped_models ON processing.models FOR ALL TO "
        "openvietgis_runtime\n      USING(organization_id=NULLIF(curre"
        "nt_setting('openvietgis.organization_id',true),'')::uuid\n   "
        "   AND project_id=NULLIF(current_setting('openvietgis.projec"
        "t_id',true),'')::uuid\n      AND NOT gis_access.scoped(organi"
        "zation_id,project_id))\n      WITH CHECK(organization_id=NULL"
        "IF(current_setting('openvietgis.organization_id',true),'')::"
        "uuid\n      AND project_id=NULLIF(current_setting('openvietgi"
        "s.project_id',true),'')::uuid\n      AND NOT "
        "gis_access.scoped(organization_id,project_id))"
    )
    op.execute("GRANT SELECT,INSERT,UPDATE,DELETE ON processing.models TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE processing.models")
