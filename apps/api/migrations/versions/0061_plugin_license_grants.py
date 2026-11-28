"""Immutable commercial certificates; distinct from sandbox commerce and runtime grants."""
from alembic import op
revision = "0061_plugin_license_grants"
down_revision = "0060_plugin_publisher_rotation"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE extensions.plugin_license_grants (
        organization_id uuid NOT NULL REFERENCES org.organizations(id) ON DELETE RESTRICT,
        grant_id uuid NOT NULL, actor_id uuid NOT NULL, extension_key varchar(100) NOT NULL,
        release_digest varchar(64) NOT NULL CHECK(release_digest ~ '^[a-f0-9]{64}$'),
        sequence bigint NOT NULL CHECK(sequence BETWEEN 1 AND 9007199254740991),
        kind varchar(16) NOT NULL CHECK(kind IN ('trial','subscription','revoked')),
        grant_digest varchar(64) NOT NULL CHECK(grant_digest ~ '^[a-f0-9]{64}$'),
        certificate jsonb NOT NULL CHECK(jsonb_typeof(certificate)='object' AND octet_length(certificate::text)<=8192),
        signature varchar(88) NOT NULL CHECK(length(signature)=88),
        accepted_at timestamptz NOT NULL DEFAULT clock_timestamp(),
        PRIMARY KEY(organization_id,grant_id), UNIQUE(organization_id,release_digest,sequence)
      );
      CREATE UNIQUE INDEX plugin_license_one_trial ON extensions.plugin_license_grants(organization_id,extension_key) WHERE kind='trial';
      ALTER TABLE extensions.plugin_license_grants ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.plugin_license_grants FORCE ROW LEVEL SECURITY;
      CREATE POLICY license_read ON extensions.plugin_license_grants FOR SELECT TO openvietgis_runtime USING(
        organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND NULLIF(current_setting('openvietgis.actor_user_id',true),'') IS NOT NULL
        AND (NULLIF(current_setting('openvietgis.project_id',true),'') IS NULL
          OR NOT gis_access.scoped(organization_id,NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)));
      CREATE POLICY license_insert ON extensions.plugin_license_grants FOR INSERT TO openvietgis_runtime WITH CHECK(
        organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND actor_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
        AND NULLIF(current_setting('openvietgis.project_id',true),'') IS NULL);
      GRANT SELECT,INSERT ON extensions.plugin_license_grants TO openvietgis_runtime;
    """)

def downgrade():
    op.execute("ALTER TABLE extensions.plugin_license_grants NO FORCE ROW LEVEL SECURITY")
    if op.get_bind().exec_driver_sql("SELECT EXISTS(SELECT 1 FROM extensions.plugin_license_grants)").scalar():
        raise RuntimeError("Archive license history before downgrade")
    op.execute("DROP TABLE extensions.plugin_license_grants")
