"""Immutable scoped 3D Tiles resources."""

from alembic import op

revision = "0032_tilesets"
down_revision = "0031_saved_scenes"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE spatial.tilesets (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, id uuid NOT NULL,
        name varchar(160) NOT NULL CHECK(length(btrim(name))>0),
        manifest jsonb NOT NULL, byte_size bigint NOT NULL CHECK(byte_size>0),
        tile_count integer NOT NULL CHECK(tile_count BETWEEN 1 AND 128),
        actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id)
          REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
      );
      CREATE TABLE spatial.tileset_contents (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, tileset_id uuid NOT NULL,
        uri varchar(180) NOT NULL, sha256 char(64) NOT NULL, payload bytea NOT NULL,
        PRIMARY KEY(organization_id,project_id,tileset_id,uri),
        FOREIGN KEY(organization_id,project_id,tileset_id)
          REFERENCES spatial.tilesets(organization_id,project_id,id) ON DELETE CASCADE
      )
    """)
    for table in ("tilesets", "tileset_contents"):
        op.execute(f"ALTER TABLE spatial.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE spatial.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""CREATE POLICY tileset_scope ON spatial.{table} FOR ALL TO openvietgis_runtime
          USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
            AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
            AND NOT gis_access.scoped(organization_id,project_id))
          WITH CHECK(organization_id=
            NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
            AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
            AND NOT gis_access.scoped(organization_id,project_id))""")
        op.execute(f"GRANT SELECT,INSERT ON spatial.{table} TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE spatial.tileset_contents")
    op.execute("DROP TABLE spatial.tilesets")
