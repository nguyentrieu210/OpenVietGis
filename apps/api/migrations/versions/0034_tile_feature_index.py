"""Immutable per-tile triangle-to-GIS feature index."""

from alembic import op

revision = "0034_tile_feature_index"
down_revision = "0033_tileset_provenance"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
        ALTER TABLE spatial.tileset_contents ADD COLUMN feature_index jsonb NOT NULL
          DEFAULT '[]'::jsonb CHECK(jsonb_typeof(feature_index)='array')
    """)


def downgrade():
    op.execute("ALTER TABLE spatial.tileset_contents DROP COLUMN feature_index")
    # Assets with a removed index must not advertise picking on subsequent upgrade.
    op.execute("""
        UPDATE spatial.tilesets SET provenance=provenance-'feature_picking'
        WHERE provenance ? 'feature_picking'
    """)
