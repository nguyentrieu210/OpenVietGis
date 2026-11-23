"""Converge the 3D tile feature-index and generated-tiles migration branches.

Both parent migrations remain intact; this revision only joins their histories.
"""

revision = "0038_consolidated_feature_index"
down_revision = ("0037_consolidated_generation", "0034_tile_feature_index")
branch_labels = None
depends_on = None


def upgrade() -> None:
    pass


def downgrade() -> None:
    pass
