"""Code-scoped reviewed address names; canonical route names stay unchanged."""

from alembic import op

revision = "hn_0008_reviewed_route_aliases"
down_revision = "hn_0007_route_lines_fast_path"
branch_labels = None
depends_on = None


def upgrade():
    op.execute(
        "ALTER TABLE house_numbering.routes ADD COLUMN reviewed_aliases jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(reviewed_aliases)='array')"
    )


def downgrade():
    op.execute("ALTER TABLE house_numbering.routes DROP COLUMN reviewed_aliases")
