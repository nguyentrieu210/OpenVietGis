"""Extend durable processing algorithms; never rewrite existing job history."""
from alembic import op

revision = "0048_processing_extended"
down_revision = "0047_processing_overlay"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("ALTER TABLE processing.jobs DROP CONSTRAINT ck_processing_algo")
    op.execute(
        "ALTER TABLE processing.jobs ADD CONSTRAINT ck_processing_algo "
        "CHECK (algorithm_id IN ('buffer','clip','dissolve','difference',"
        "'intersection','union','reproject','spatial_join'))"
    )


def downgrade() -> None:
    op.execute("ALTER TABLE processing.jobs DROP CONSTRAINT ck_processing_algo")
    op.execute(
        "ALTER TABLE processing.jobs ADD CONSTRAINT ck_processing_algo "
        "CHECK (algorithm_id IN ('buffer','clip','dissolve','difference','intersection'))"
    )
