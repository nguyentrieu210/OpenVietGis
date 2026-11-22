"""Persistent project-scoped raster assets and bounded XYZ tile pyramids."""
from alembic import op

revision = "0024_raster_layers"
down_revision = "0023_gis_access"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("CREATE SCHEMA raster")
    op.execute("GRANT USAGE ON SCHEMA raster TO openvietgis_runtime")
    op.execute("""
      CREATE TABLE raster.layers (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, id uuid NOT NULL,
        name varchar(160) NOT NULL, metadata jsonb NOT NULL,
        cog bytea NOT NULL CHECK(octet_length(cog) BETWEEN 1 AND 67108864),
        byte_size bigint NOT NULL CHECK(byte_size BETWEEN 1 AND 134217728),
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
      );
      CREATE TABLE raster.tiles (
        organization_id uuid NOT NULL,project_id uuid NOT NULL,layer_id uuid NOT NULL,
        z integer NOT NULL CHECK(z BETWEEN 0 AND 20),x integer NOT NULL,y integer NOT NULL,
        png bytea NOT NULL CHECK(octet_length(png) BETWEEN 1 AND 400000),
        PRIMARY KEY(organization_id,project_id,layer_id,z,x,y),
        CHECK(x>=0 AND y>=0 AND x<(1<<z) AND y<(1<<z)),
        FOREIGN KEY(organization_id,project_id,layer_id) REFERENCES raster.layers(organization_id,project_id,id) ON DELETE CASCADE
      )
    """)
    for table in ("layers","tiles"):
        op.execute(f"ALTER TABLE raster.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE raster.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""CREATE POLICY scoped_raster ON raster.{table} FOR ALL TO openvietgis_runtime
            USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
            AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
            AND NOT gis_access.scoped(organization_id,project_id))
            WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
            AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
            AND NOT gis_access.scoped(organization_id,project_id))""")
        op.execute(f"GRANT SELECT,INSERT,DELETE ON raster.{table} TO openvietgis_runtime")

def downgrade():
    op.execute("DROP TABLE raster.tiles")
    op.execute("DROP TABLE raster.layers")
    op.execute("DROP SCHEMA raster")
