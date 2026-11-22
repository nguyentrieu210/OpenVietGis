"""Immutable project 3D assets and placement revisions."""

from alembic import op

revision = "0030_model_vault"
down_revision = "0029_cad_documents"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
        CREATE TABLE spatial.model_assets (
            organization_id uuid NOT NULL, project_id uuid NOT NULL, id uuid NOT NULL,
            filename varchar(180) NOT NULL,
            format varchar(4) NOT NULL CHECK(format IN ('glb','gltf')),
            sha256 char(64) NOT NULL CHECK(sha256 ~ '^[0-9a-f]{64}$'),
            byte_size integer NOT NULL CHECK(byte_size BETWEEN 1 AND 10485760),
            payload bytea NOT NULL CHECK(octet_length(payload)=byte_size),
            actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY(organization_id,project_id,id),
            FOREIGN KEY(organization_id,project_id)
                REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
        );
        CREATE TABLE spatial.model_placements (
            organization_id uuid NOT NULL, project_id uuid NOT NULL, model_id uuid NOT NULL,
            version integer NOT NULL CHECK(version BETWEEN 1 AND 50),
            profile jsonb NOT NULL CHECK(jsonb_typeof(profile)='object'),
            actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY(organization_id,project_id,model_id,version),
            FOREIGN KEY(organization_id,project_id,model_id)
                REFERENCES spatial.model_assets(organization_id,project_id,id) ON DELETE CASCADE
        );
    """)
    for table in ("model_assets", "model_placements"):
        op.execute(f"ALTER TABLE spatial.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE spatial.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""CREATE POLICY model_scope ON spatial.{table} FOR ALL TO openvietgis_runtime
          USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
            AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
            AND NOT gis_access.scoped(organization_id,project_id))
          WITH CHECK(organization_id=
            NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
            AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
            AND NOT gis_access.scoped(organization_id,project_id))""")
        op.execute(f"GRANT SELECT,INSERT ON spatial.{table} TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE spatial.model_placements")
    op.execute("DROP TABLE spatial.model_assets")
