"""Versioned, security-invoker spatial read contract for compiled modules."""
from alembic import op

revision = "0051_module_spatial_reads"
down_revision = "0050_module_change_stream"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE VIEW extensions.spatial_features_v1 WITH (security_invoker=true) AS
        SELECT organization_id,project_id,source_id,feature_id,attributes,geometry,
               geometry_state,content_revision,schema_revision
        FROM spatial.reference_features;
      CREATE VIEW extensions.spatial_sources_v1 WITH (security_invoker=true) AS
        SELECT organization_id,project_id,id,lifecycle,validation_state,geometry_family,
               stored_crs,content_revision,schema_revision
        FROM spatial.data_sources;
      REVOKE ALL ON extensions.spatial_features_v1,extensions.spatial_sources_v1 FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT ON extensions.spatial_features_v1,extensions.spatial_sources_v1 TO openvietgis_runtime;
    """)


def downgrade():
    op.execute("DROP VIEW extensions.spatial_features_v1,extensions.spatial_sources_v1")
