"""Project-local module layer bindings, keyed by immutable spatial source IDs."""
from alembic import op

revision = "0049_module_layer_bindings"
down_revision = "0048_module_permission_policies"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE extensions.module_binding_sets (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, extension_key varchar(100) NOT NULL,
        revision bigint NOT NULL CHECK(revision>0), updated_by_actor_id uuid NOT NULL,
        updated_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,extension_key),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE RESTRICT
      );
      CREATE TABLE extensions.module_layer_bindings (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, extension_key varchar(100) NOT NULL,
        source_id uuid NOT NULL, role varchar(64) NOT NULL CHECK(role ~ '^[a-z][a-z0-9_]{0,63}$'),
        schema_revision varchar(64) NOT NULL CHECK(schema_revision ~ '^[a-f0-9]{64}$'),
        PRIMARY KEY(organization_id,project_id,extension_key,source_id),
        FOREIGN KEY(organization_id,project_id,extension_key) REFERENCES extensions.module_binding_sets(organization_id,project_id,extension_key) ON DELETE CASCADE,
        FOREIGN KEY(organization_id,project_id,source_id) REFERENCES spatial.data_sources(organization_id,project_id,id) ON DELETE RESTRICT
      );
      ALTER TABLE extensions.module_binding_sets ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.module_binding_sets FORCE ROW LEVEL SECURITY;
      ALTER TABLE extensions.module_layer_bindings ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.module_layer_bindings FORCE ROW LEVEL SECURITY;
      CREATE POLICY module_binding_set_scope ON extensions.module_binding_sets FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      CREATE POLICY module_binding_scope ON extensions.module_layer_bindings FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      GRANT SELECT,INSERT,UPDATE,DELETE ON extensions.module_binding_sets,extensions.module_layer_bindings TO openvietgis_runtime;
    """)


def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.module_binding_sets) THEN
        RAISE EXCEPTION 'Explicit module bindings exist; export and clear before downgrade'; END IF; END $$;
      DROP TABLE extensions.module_layer_bindings;
      DROP TABLE extensions.module_binding_sets;
    """)
