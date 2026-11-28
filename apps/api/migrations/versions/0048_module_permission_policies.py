"""Project-scoped official module permission allowlists (bounded by Core role)."""
from alembic import op

revision = "0048_module_permission_policies"
down_revision = "0047_generic_feature_guards"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE extensions.module_permission_policies (
        organization_id uuid NOT NULL,project_id uuid NOT NULL,extension_key varchar(100) NOT NULL,
        actor_type text NOT NULL CHECK(actor_type IN ('human','service_principal')),actor_id uuid NOT NULL,
        permissions text[] CHECK(cardinality(permissions)<=64 AND array_position(permissions,NULL) IS NULL),
        revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),updated_by_actor_id uuid NOT NULL,
        updated_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,extension_key,actor_type,actor_id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE RESTRICT
      );
      ALTER TABLE extensions.module_permission_policies ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.module_permission_policies FORCE ROW LEVEL SECURITY;
      CREATE POLICY module_permission_project_scope ON extensions.module_permission_policies
        FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      GRANT SELECT,INSERT,UPDATE,DELETE ON extensions.module_permission_policies TO openvietgis_runtime;
    """)


def downgrade():
    # Refuse rollback that would silently widen permissions; managers must export
    # and explicitly clear policies before downgrading to the role-only engine.
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.module_permission_policies WHERE permissions IS NOT NULL) THEN
        RAISE EXCEPTION 'Explicit module permission policies exist; refusing security downgrade'; END IF; END $$;
      DROP TABLE extensions.module_permission_policies;
    """)
