"""Persistent project-scoped raster assets and bounded XYZ tile pyramids."""

from alembic import op

revision = "0024_raster_layers"
down_revision = "0023_gis_access"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("CREATE SCHEMA raster")
    op.execute("GRANT USAGE ON SCHEMA raster TO openvietgis_runtime")
    op.execute(
        "\n      CREATE TABLE raster.layers (\n        "
        "organization_id uuid NOT NULL, project_id uuid NOT "
        "NULL, id uuid NOT NULL,\n        name varchar(160) NOT "
        "NULL, metadata jsonb NOT NULL,\n        cog bytea NOT "
        "NULL CHECK(octet_length(cog) BETWEEN 1 AND 67108864),\n "
        "       byte_size bigint NOT NULL CHECK(byte_size "
        "BETWEEN 1 AND 134217728),\n        created_at "
        "timestamptz NOT NULL DEFAULT now(),\n        PRIMARY "
        "KEY(organization_id,project_id,id),\n        FOREIGN "
        "KEY(organization_id,project_id) REFERENCES "
        "workspace.projects(organization_id,id) ON DELETE "
        "CASCADE\n      );\n      CREATE TABLE raster.tiles (\n    "
        "    organization_id uuid NOT NULL,project_id uuid NOT "
        "NULL,layer_id uuid NOT NULL,\n        z integer NOT "
        "NULL CHECK(z BETWEEN 0 AND 20),x integer NOT NULL,y "
        "integer NOT NULL,\n        png bytea NOT NULL "
        "CHECK(octet_length(png) BETWEEN 1 AND 400000),\n        "
        "PRIMARY KEY(organization_id,project_id,layer_id,z,x,y),"
        "\n        CHECK(x>=0 AND y>=0 AND x<(1<<z) AND "
        "y<(1<<z)),\n        FOREIGN KEY(organization_id,project_"
        "id,layer_id) REFERENCES raster.layers(organization_id,p"
        "roject_id,id) ON DELETE CASCADE\n      )\n    "
    )
    for table in ("layers", "tiles"):
        op.execute(f"ALTER TABLE raster.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE raster.{table} FORCE ROW LEVEL SECURITY")
        op.execute(
            "CREATE POLICY scoped_raster ON raster."
            f"{table}"
            " FOR ALL TO openvietgis_runtime\n            "
            "USING(organization_id=NULLIF(current_setting('openvietg"
            "is.organization_id',true),'')::uuid\n            AND "
            "project_id=NULLIF(current_setting('openvietgis.project_"
            "id',true),'')::uuid\n            AND NOT "
            "gis_access.scoped(organization_id,project_id))\n        "
            "    WITH CHECK(organization_id=NULLIF(current_setting('"
            "openvietgis.organization_id',true),'')::uuid\n          "
            "  AND project_id=NULLIF(current_setting('openvietgis.pr"
            "oject_id',true),'')::uuid\n            AND NOT "
            "gis_access.scoped(organization_id,project_id))"
        )
        op.execute(f"GRANT SELECT,INSERT,DELETE ON raster.{table} TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE raster.tiles")
    op.execute("DROP TABLE raster.layers")
    op.execute("DROP SCHEMA raster")
