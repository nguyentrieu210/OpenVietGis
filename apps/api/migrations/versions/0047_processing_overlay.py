"""Add native polygon-overlay processing algorithms without rewriting historical jobs."""
from alembic import op

revision = "0047_processing_overlay"
down_revision = "0046_map_display_config"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("ALTER TABLE processing.jobs DROP CONSTRAINT ck_processing_algo")
    op.execute(
        "ALTER TABLE processing.jobs ADD CONSTRAINT ck_processing_algo "
        "CHECK (algorithm_id IN ('buffer','clip','dissolve','difference','intersection'))"
    )


def downgrade() -> None:
    # Fail rather than deleting or relabeling jobs if a new algorithm has been used.
    op.execute("ALTER TABLE processing.jobs DROP CONSTRAINT ck_processing_algo")
    op.execute(
        "ALTER TABLE processing.jobs ADD CONSTRAINT ck_processing_algo "
        "CHECK (algorithm_id IN ('buffer','clip','dissolve'))"
    )
