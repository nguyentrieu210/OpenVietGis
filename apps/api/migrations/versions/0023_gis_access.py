"""Project GIS access scopes with deny-by-default membership policies."""
from alembic import op

revision = "0023_gis_access"
down_revision = "0023_user_avatar"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("CREATE SCHEMA gis_access")
    op.execute("GRANT USAGE ON SCHEMA gis_access TO openvietgis_runtime")
    op.execute("""
      CREATE TABLE gis_access.scoped_members (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, user_id uuid NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,user_id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE,
        FOREIGN KEY(organization_id,user_id) REFERENCES org.organization_memberships(organization_id,user_id) ON DELETE CASCADE
      );
      CREATE TABLE gis_access.groups (
        organization_id uuid NOT NULL, project_id uuid NOT NULL,id uuid NOT NULL,name varchar(160) NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,id), UNIQUE(organization_id,project_id,name),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
      );
      CREATE TABLE gis_access.group_members (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, group_id uuid NOT NULL,user_id uuid NOT NULL,
        PRIMARY KEY(organization_id,project_id,group_id,user_id),
        FOREIGN KEY(organization_id,project_id,group_id) REFERENCES gis_access.groups(organization_id,project_id,id) ON DELETE CASCADE,
        FOREIGN KEY(organization_id,project_id,user_id) REFERENCES gis_access.scoped_members(organization_id,project_id,user_id) ON DELETE CASCADE
      );
      CREATE TABLE gis_access.rules (
        organization_id uuid NOT NULL,project_id uuid NOT NULL,id uuid NOT NULL,
        user_id uuid,group_id uuid,layer_id uuid NOT NULL,source_id uuid NOT NULL,
        actions text[] NOT NULL, feature_ids text[], attribute_filter jsonb, region geometry(MultiPolygon,4326),
        created_at timestamptz NOT NULL DEFAULT now(), revoked_at timestamptz,
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,layer_id) REFERENCES spatial.layers(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,source_id) REFERENCES spatial.data_sources(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,user_id) REFERENCES gis_access.scoped_members(organization_id,project_id,user_id),
        FOREIGN KEY(organization_id,project_id,group_id) REFERENCES gis_access.groups(organization_id,project_id,id),
        CHECK((user_id IS NOT NULL) <> (group_id IS NOT NULL)),
        CHECK(actions <@ ARRAY['read','create','geometry','attributes','delete','export','approve']::text[] AND 'read'=ANY(actions)),
        CHECK(feature_ids IS NULL OR cardinality(feature_ids) BETWEEN 1 AND 50000),
        CHECK(attribute_filter IS NULL OR jsonb_typeof(attribute_filter)='object'),
        CHECK(region IS NULL OR (ST_IsValid(region) AND NOT ST_IsEmpty(region)))
      );
      CREATE INDEX ix_gis_access_source ON gis_access.rules(organization_id,project_id,source_id) WHERE revoked_at IS NULL;
      CREATE INDEX ix_gis_access_layer ON gis_access.rules(organization_id,project_id,layer_id) WHERE revoked_at IS NULL;
      CREATE INDEX ix_gis_access_group_members ON gis_access.group_members(organization_id,project_id,user_id);
      CREATE TABLE gis_access.reviews(
        organization_id uuid NOT NULL,project_id uuid NOT NULL,id uuid NOT NULL,changeset_id uuid NOT NULL,
        reviewer_id uuid NOT NULL,decision varchar(12) NOT NULL CHECK(decision IN ('approved','rejected')),
        note varchar(500) NOT NULL DEFAULT '',created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
      )
    """)
    for table in ["scoped_members","groups","group_members","rules","reviews"]:
        op.execute(f"ALTER TABLE gis_access.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE gis_access.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""CREATE POLICY {table}_tenant ON gis_access.{table} FOR ALL TO openvietgis_runtime
          USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid)
          WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid)""")
        op.execute(f"GRANT SELECT,INSERT,UPDATE,DELETE ON gis_access.{table} TO openvietgis_runtime")
    op.execute("""
      CREATE FUNCTION gis_access.scoped(o uuid,p uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
        SELECT EXISTS(SELECT 1 FROM gis_access.scoped_members m
        JOIN org.organization_memberships om ON om.organization_id=m.organization_id AND om.user_id=m.user_id
        WHERE m.organization_id=o AND m.project_id=p
         AND m.user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
         AND om.status='active' AND om.role NOT IN ('owner','admin'))
      $$;
      CREATE FUNCTION gis_access.subject_rules(o uuid,p uuid) RETURNS SETOF gis_access.rules LANGUAGE sql STABLE AS $$
        SELECT r.* FROM gis_access.rules r
        WHERE r.organization_id=o AND r.project_id=p AND r.revoked_at IS NULL
         AND (r.user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid
         OR EXISTS(SELECT 1 FROM gis_access.group_members gm WHERE gm.organization_id=o AND gm.project_id=p
           AND gm.group_id=r.group_id AND gm.user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid))
      $$;
      CREATE FUNCTION gis_access.layer_allowed(o uuid,p uuid,l uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
        SELECT NOT gis_access.scoped(o,p) OR EXISTS(SELECT 1 FROM gis_access.subject_rules(o,p) r WHERE r.layer_id=l AND 'read'=ANY(r.actions))
      $$;
      CREATE FUNCTION gis_access.source_allowed(o uuid,p uuid,s uuid,a text DEFAULT 'read') RETURNS boolean LANGUAGE sql STABLE AS $$
        SELECT NOT gis_access.scoped(o,p) OR EXISTS(SELECT 1 FROM gis_access.subject_rules(o,p) r WHERE r.source_id=s AND a=ANY(r.actions))
      $$;
      CREATE FUNCTION gis_access.feature_allowed(o uuid,p uuid,s uuid,f text,attrs jsonb,g geometry,a text DEFAULT 'read') RETURNS boolean LANGUAGE sql STABLE AS $$
        SELECT NOT gis_access.scoped(o,p) OR EXISTS(
          SELECT 1 FROM gis_access.subject_rules(o,p) r WHERE r.source_id=s AND a=ANY(r.actions)
          AND (r.feature_ids IS NULL OR f=ANY(r.feature_ids))
          AND (r.attribute_filter IS NULL OR attrs @> r.attribute_filter)
          AND (r.region IS NULL OR CASE WHEN g IS NOT NULL AND ST_SRID(g)>0 THEN ST_CoveredBy(ST_Transform(g,4326),r.region) ELSE FALSE END)
        )
      $$;
    """)
    # The verified repository can turn on internal mode ONLY for revision maintenance of an
    # already-authorized changeset. All public reads remain restricted.
    internal = "COALESCE(current_setting('openvietgis.acl_internal',true),'')='on'"
    conds = {
      "layers":"gis_access.layer_allowed(organization_id,project_id,id)",
      "data_sources":"gis_access.source_allowed(organization_id,project_id,id)",
      "source_fields":"gis_access.source_allowed(organization_id,project_id,source_id)",
      "reference_features":"gis_access.feature_allowed(organization_id,project_id,source_id,feature_id,attributes,geometry)",
      "catalog_changes":"NOT gis_access.scoped(organization_id,project_id)"
    }
    for table,expr in conds.items():
        op.execute(f"""CREATE POLICY {table}_gis_acl ON spatial.{table} AS RESTRICTIVE FOR ALL TO openvietgis_runtime
          USING (({internal}) OR ({expr})) WITH CHECK (({internal}) OR ({expr}))""")

def downgrade():
    for table in ["layers","data_sources","source_fields","reference_features","catalog_changes"]:
        op.execute(f"DROP POLICY {table}_gis_acl ON spatial.{table}")
    op.execute("DROP SCHEMA gis_access CASCADE")
