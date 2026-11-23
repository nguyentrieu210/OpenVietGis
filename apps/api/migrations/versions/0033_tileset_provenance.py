"""Retain the immutable source pins for generated tilesets."""

from alembic import op

revision = "0033_tileset_provenance"
down_revision = "0032_tilesets"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
        ALTER TABLE spatial.tilesets ADD COLUMN provenance jsonb NOT NULL DEFAULT '{}'::jsonb
          CHECK(jsonb_typeof(provenance)='object')
    """)


def downgrade():
    op.execute("ALTER TABLE spatial.tilesets DROP COLUMN provenance")
