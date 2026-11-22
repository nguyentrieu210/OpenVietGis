"""Spatial lookup index and ISO A0–A4 cartography."""
from alembic import op

revision="0021_professional_tools"
down_revision="0020_manual_layers"
branch_labels=None
depends_on=None

def upgrade():
    op.execute("CREATE INDEX ix_reference_features_geometry ON spatial.reference_features USING gist (geometry) WHERE geometry_state='present'")
    op.execute("ALTER TABLE cartography.print_layouts DROP CONSTRAINT ck_layout_paper")
    op.execute("ALTER TABLE cartography.print_layouts ADD CONSTRAINT ck_layout_paper CHECK (paper_size IN ('A0','A1','A2','A3','A4'))")

def downgrade():
    op.execute("ALTER TABLE cartography.print_layouts DROP CONSTRAINT ck_layout_paper")
    op.execute("ALTER TABLE cartography.print_layouts ADD CONSTRAINT ck_layout_paper CHECK (paper_size IN ('A3','A4'))")
    op.execute("DROP INDEX spatial.ix_reference_features_geometry")
