"""Permit native projected VN-2000 binary artifacts without datum conversion."""
import sqlalchemy as sa
from alembic import op

revision = "0051_native_vn2000_export"
down_revision = "0050_native_vn2000_processing"
branch_labels = None
depends_on = None

OLD = "4326,3857,4756,5899"
EXTRA = (
    "3405,3406,5896,5897,5898,9205,9206,9207,9208,9209,"
    "9210,9211,9212,9213,9214,9215,9216,9217,9218"
)

def upgrade():
    op.execute("ALTER TABLE transfers.derived_artifacts DROP CONSTRAINT ck_derived_artifact_crs")
    op.execute(
        f"""ALTER TABLE transfers.derived_artifacts ADD CONSTRAINT ck_derived_artifact_crs
        CHECK (output_crs IN ({OLD}) OR (output_crs IN ({EXTRA}) AND output_format IN (
            'geopackage', 'shapefile_zip')))"""
    )

def downgrade():
    has_native_artifacts = op.get_bind().execute(
        sa.text(
            f"SELECT EXISTS(SELECT 1 FROM transfers.derived_artifacts "
            f"WHERE output_crs IN ({EXTRA}))"
        )
    ).scalar_one()
    if has_native_artifacts:
        raise RuntimeError("Remove native VN-2000 artifacts before downgrade")
    op.execute("ALTER TABLE transfers.derived_artifacts DROP CONSTRAINT ck_derived_artifact_crs")
    op.execute(
        f"ALTER TABLE transfers.derived_artifacts ADD CONSTRAINT ck_derived_artifact_crs "
        f"CHECK (output_crs IN ({OLD}))"
    )
