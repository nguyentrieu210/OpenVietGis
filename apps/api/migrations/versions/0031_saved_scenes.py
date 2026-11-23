"""Immutable project 3D scene definitions."""

from alembic import op

revision = "0031_saved_scenes"
down_revision = "0030_model_vault"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
        CREATE TABLE spatial.scene_versions (
            organization_id uuid NOT NULL, project_id uuid NOT NULL, scene_id uuid NOT NULL,
            version integer NOT NULL CHECK(version BETWEEN 1 AND 50),
            name varchar(160) NOT NULL CHECK(length(btrim(name))>0),
            profile jsonb NOT NULL CHECK(jsonb_typeof(profile)='object'),
            actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY(organization_id,project_id,scene_id,version),
            FOREIGN KEY(organization_id,project_id)
                REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
        )
    """)
    op.execute("ALTER TABLE spatial.scene_versions ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE spatial.scene_versions FORCE ROW LEVEL SECURITY")
    op.execute("""CREATE POLICY scene_scope ON spatial.scene_versions FOR ALL TO openvietgis_runtime
      USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
        AND NOT gis_access.scoped(organization_id,project_id))
      WITH CHECK(organization_id=
        NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
        AND NOT gis_access.scoped(organization_id,project_id))""")
    op.execute("GRANT SELECT,INSERT ON spatial.scene_versions TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE spatial.scene_versions")
