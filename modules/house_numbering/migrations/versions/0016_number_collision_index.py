"""Nonunique index for conservative collision checks; retain existing evidence."""
from alembic import op

revision = "hn_0016_collision_index"
down_revision = "hn_0015_issuance_replacement"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE INDEX ix_hn_active_collision_key ON house_numbering.address_numbers
      (organization_id,project_id,translate(btrim(number_text),'abcdefghijklmnopqrstuvwxyz','ABCDEFGHIJKLMNOPQRSTUVWXYZ'),route_id)
      WHERE retired_at IS NULL
    """)

def downgrade():
    op.execute("DROP INDEX house_numbering.ix_hn_active_collision_key")
