"""Allow manually created, empty vector sources."""

from alembic import op

revision = "0020_manual_layers"
down_revision = "0019_extension_history"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("ALTER TABLE spatial.data_sources DROP CONSTRAINT ck_source_kind")
    op.execute("""ALTER TABLE spatial.data_sources ADD CONSTRAINT ck_source_kind CHECK (
      (kind IN ('reference_fixture','derived_processing','manual') AND source_asset_id IS NULL)
      OR (kind = 'imported_file' AND source_asset_id IS NOT NULL))""")


def downgrade():
    op.execute("ALTER TABLE spatial.data_sources DROP CONSTRAINT ck_source_kind")
    op.execute("""ALTER TABLE spatial.data_sources ADD CONSTRAINT ck_source_kind CHECK (
      (kind IN ('reference_fixture','derived_processing') AND source_asset_id IS NULL)
      OR (kind = 'imported_file' AND source_asset_id IS NOT NULL))""")
