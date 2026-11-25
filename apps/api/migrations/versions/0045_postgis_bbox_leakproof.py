"""Let the planner use GIST bbox probes on RLS-protected spatial tables.

Under row-level security PostgreSQL only pushes LEAKPROOF quals below the policy
checks, so `geometry && envelope` on spatial.reference_features (role
openvietgis_runtime) fell back to scanning every row of the layer. The bbox
overlap function only compares two boxes; its sole possible error is the mixed-SRID
message, which reveals no more than the layer's declared CRS. Requires superuser.
"""

from alembic import op

revision = "0045_postgis_bbox_leakproof"
down_revision = "0044_gis_acl_statement_scope"
branch_labels = None
depends_on = None

# Function behind the `geometry && geometry` operator.
FUNCTION = "geometry_overlaps(geometry, geometry)"


def upgrade() -> None:
    op.execute(f"ALTER FUNCTION {FUNCTION} LEAKPROOF")


def downgrade() -> None:
    op.execute(f"ALTER FUNCTION {FUNCTION} NOT LEAKPROOF")
