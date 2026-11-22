"""Project-scoped short-lived feature edit leases."""
from alembic import op

revision = "0032_feature_edit_leases"
down_revision = "0031_spatial_presence_events"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
    CREATE TABLE collaboration.feature_edit_leases (
      organization_id uuid NOT NULL, project_id uuid NOT NULL, source_id uuid NOT NULL,
      feature_id varchar(128) NOT NULL, actor_id uuid NOT NULL, actor_name varchar(200) NOT NULL,
      token uuid NOT NULL, expires_at timestamptz NOT NULL,
      PRIMARY KEY (organization_id,project_id,source_id,feature_id),
      FOREIGN KEY (organization_id,project_id,source_id)
        REFERENCES spatial.data_sources(organization_id,project_id,id) ON DELETE CASCADE
    );
    CREATE INDEX feature_edit_leases_expiry ON collaboration.feature_edit_leases(organization_id,project_id,expires_at);
    ALTER TABLE collaboration.feature_edit_leases ENABLE ROW LEVEL SECURITY;
    ALTER TABLE collaboration.feature_edit_leases FORCE ROW LEVEL SECURITY;
    """)
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
    # All authenticated project writers need to see conflicts, including scoped
    # and service writers. The human-facing list endpoint separately denies scoped access.
    op.execute(f"CREATE POLICY lease_read ON collaboration.feature_edit_leases FOR SELECT TO openvietgis_runtime USING ({scope})")
    op.execute(f"CREATE POLICY lease_insert ON collaboration.feature_edit_leases FOR INSERT TO openvietgis_runtime WITH CHECK ({scope} AND actor_id={actor} AND NOT gis_access.scoped(organization_id,project_id))")
    op.execute(f"CREATE POLICY lease_update ON collaboration.feature_edit_leases FOR UPDATE TO openvietgis_runtime USING ({scope} AND (actor_id={actor} OR expires_at<=clock_timestamp())) WITH CHECK ({scope} AND actor_id={actor} AND NOT gis_access.scoped(organization_id,project_id))")
    op.execute(f"CREATE POLICY lease_delete ON collaboration.feature_edit_leases FOR DELETE TO openvietgis_runtime USING ({scope} AND (actor_id={actor} OR expires_at<=clock_timestamp()))")
    op.execute("GRANT SELECT,INSERT,UPDATE,DELETE ON collaboration.feature_edit_leases TO openvietgis_runtime")
    op.execute("""
    CREATE FUNCTION collaboration.publish_lease_change() RETURNS trigger LANGUAGE plpgsql AS $$
    DECLARE o uuid; p uuid;
    BEGIN
      IF TG_OP='DELETE' THEN o:=OLD.organization_id; p:=OLD.project_id;
      ELSE o:=NEW.organization_id; p:=NEW.project_id; END IF;
      PERFORM pg_notify('ovg_team_collaboration',json_build_object('o',o,'p',p,'a',NULL,'b',NULL,'kind','lock_change')::text);
      RETURN NULL;
    END $$;
    CREATE TRIGGER feature_edit_leases_change AFTER INSERT OR UPDATE OR DELETE ON collaboration.feature_edit_leases
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_lease_change();
    """)

def downgrade():
    op.execute("DROP TABLE collaboration.feature_edit_leases")
    op.execute("DROP FUNCTION collaboration.publish_lease_change()")
