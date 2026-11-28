"""Transactional, project-scoped module invalidation log with commit-ordered cursor."""
from alembic import op

revision = "0050_module_change_stream"
down_revision = "0049_module_layer_bindings"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE extensions.module_change_streams (
        organization_id uuid NOT NULL, project_id uuid NOT NULL,
        epoch uuid NOT NULL DEFAULT gen_random_uuid(),
        head bigint NOT NULL DEFAULT 0 CHECK(head>=0),
        PRIMARY KEY(organization_id,project_id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE RESTRICT
      );
      CREATE TABLE extensions.module_change_events (
        organization_id uuid NOT NULL, project_id uuid NOT NULL,
        sequence bigint NOT NULL CHECK(sequence>0),
        extension_key varchar(100) NOT NULL,
        kind text NOT NULL CHECK(kind IN ('activation','permissions','bindings')),
        occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
        PRIMARY KEY(organization_id,project_id,sequence),
        FOREIGN KEY(organization_id,project_id) REFERENCES extensions.module_change_streams(organization_id,project_id) ON DELETE RESTRICT
      );
      ALTER TABLE extensions.module_change_streams ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.module_change_events ENABLE ROW LEVEL SECURITY;
      CREATE POLICY module_stream_read ON extensions.module_change_streams FOR SELECT TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      CREATE POLICY module_events_read ON extensions.module_change_events FOR SELECT TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      REVOKE ALL ON extensions.module_change_streams,extensions.module_change_events FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT ON extensions.module_change_streams,extensions.module_change_events TO openvietgis_runtime;
      -- A project counter is incremented under a row lock held until commit.
      -- A sequence/identity alone is insufficient: transactions can commit out of order.
      CREATE FUNCTION extensions.capture_module_change() RETURNS trigger
      LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
      DECLARE org uuid; project uuid; module_key text; seq bigint; event_kind text;
      BEGIN
        IF TG_OP='UPDATE' AND OLD IS NOT DISTINCT FROM NEW THEN RETURN NEW; END IF;
        IF TG_OP='DELETE' THEN
          org=OLD.organization_id;project=OLD.project_id;module_key=OLD.extension_key;
        ELSE
          org=NEW.organization_id;project=NEW.project_id;module_key=NEW.extension_key;
        END IF;
        IF TG_OP='UPDATE' AND (OLD.organization_id,OLD.project_id,OLD.extension_key)
          IS DISTINCT FROM (NEW.organization_id,NEW.project_id,NEW.extension_key) THEN
          RAISE EXCEPTION 'Module configuration scope is immutable';
        END IF;
        event_kind=TG_ARGV[0];
        INSERT INTO extensions.module_change_streams(organization_id,project_id,head)
          VALUES(org,project,1)
          ON CONFLICT(organization_id,project_id) DO UPDATE
          SET head=extensions.module_change_streams.head+1 RETURNING head INTO seq;
        INSERT INTO extensions.module_change_events(organization_id,project_id,sequence,extension_key,kind)
          VALUES(org,project,seq,module_key,event_kind);
        RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
      END $$;
      REVOKE ALL ON FUNCTION extensions.capture_module_change() FROM PUBLIC;
      CREATE TRIGGER trg_module_activation_notice AFTER INSERT OR UPDATE OR DELETE ON extensions.project_activations
        FOR EACH ROW EXECUTE FUNCTION extensions.capture_module_change('activation');
      CREATE TRIGGER trg_module_permission_notice AFTER INSERT OR UPDATE OR DELETE ON extensions.module_permission_policies
        FOR EACH ROW EXECUTE FUNCTION extensions.capture_module_change('permissions');
      CREATE TRIGGER trg_module_binding_notice AFTER INSERT OR UPDATE OR DELETE ON extensions.module_binding_sets
        FOR EACH ROW EXECUTE FUNCTION extensions.capture_module_change('bindings');
    """)


def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.module_change_events) THEN
        RAISE EXCEPTION 'Module change history exists; export and clear before downgrade'; END IF; END $$;
      DROP TRIGGER trg_module_binding_notice ON extensions.module_binding_sets;
      DROP TRIGGER trg_module_permission_notice ON extensions.module_permission_policies;
      DROP TRIGGER trg_module_activation_notice ON extensions.project_activations;
      DROP FUNCTION extensions.capture_module_change();
      DROP TABLE extensions.module_change_events;
      DROP TABLE extensions.module_change_streams;
    """)
