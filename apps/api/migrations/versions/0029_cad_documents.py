"""Immutable project CAD document versions."""

from alembic import op

revision = "0029_cad_documents"
down_revision = "0028_marketplace_developers"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""CREATE TABLE cad.document_versions (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, document_id uuid NOT NULL,
        version integer NOT NULL CHECK(version BETWEEN 1 AND 20),
        filename varchar(180) NOT NULL, format varchar(3) NOT NULL CHECK(format IN ('dwg','dxf')),
        storage_key varchar(200) NOT NULL UNIQUE, sha256 char(64) NOT NULL,
        byte_size integer NOT NULL CHECK(byte_size BETWEEN 1 AND 26214400),
        actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,document_id,version),
        FOREIGN KEY(organization_id,project_id)
            REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
    )""")
    op.execute("ALTER TABLE cad.document_versions ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE cad.document_versions FORCE ROW LEVEL SECURITY")
    op.execute("""CREATE POLICY cad_document_scope ON cad.document_versions
      FOR ALL TO openvietgis_runtime
      USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id))
      WITH CHECK(organization_id=
        NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id))""")
    op.execute("GRANT SELECT,INSERT ON cad.document_versions TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE cad.document_versions")
