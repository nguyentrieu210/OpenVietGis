"""Index immutable evidence-set case lookup without rewriting old decisions."""
from alembic import op
revision = "hn_0020_reconcile_lookup"
down_revision = "hn_0019_parcel_registry"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("CREATE INDEX ix_hn_reconcile_snapshot_lookup ON house_numbering.reconcile_cases(organization_id,project_id,building_id,case_class,estimated)")
def downgrade():
    op.execute("DROP INDEX house_numbering.ix_hn_reconcile_snapshot_lookup")
