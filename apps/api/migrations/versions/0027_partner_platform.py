"""Persistent partner contributions and transactional integration event journal."""
from alembic import op

revision = "0027_partner_platform"
down_revision = "0026_layout_designer"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE extensions.partner_contributions (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, manifest_id uuid NOT NULL,
        document jsonb NOT NULL CHECK (octet_length(document::text)<=200000),
        revision integer NOT NULL DEFAULT 1 CHECK (revision>=1),
        updated_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,manifest_id),
        FOREIGN KEY(organization_id,project_id)
          REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE,
        FOREIGN KEY(manifest_id) REFERENCES extensions.manifests(id) ON DELETE CASCADE
      );
      CREATE TABLE extensions.integration_events (
        sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        id uuid NOT NULL UNIQUE, organization_id uuid NOT NULL, project_id uuid NOT NULL,
        event_kind varchar(64) NOT NULL, resource_type varchar(64) NOT NULL,
        resource_id uuid NOT NULL, revision_ref varchar(64), created_at timestamptz NOT NULL,
        FOREIGN KEY(organization_id,project_id)
          REFERENCES workspace.projects(organization_id,id) ON DELETE CASCADE
      );
      CREATE INDEX integration_event_scope ON extensions.integration_events
        (organization_id,project_id,sequence);
      ALTER TABLE extensions.partner_contributions ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.partner_contributions FORCE ROW LEVEL SECURITY;
      ALTER TABLE extensions.integration_events ENABLE ROW LEVEL SECURITY;
      ALTER TABLE extensions.integration_events FORCE ROW LEVEL SECURITY;
      CREATE POLICY partner_contribution_scope ON extensions.partner_contributions
        FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
          AND NOT gis_access.scoped(organization_id,project_id))
        WITH CHECK(organization_id=
          NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
          AND NOT gis_access.scoped(organization_id,project_id));
      CREATE POLICY integration_event_read ON extensions.integration_events
        FOR SELECT TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
          AND NOT gis_access.scoped(organization_id,project_id));
      CREATE POLICY integration_event_insert ON extensions.integration_events
        FOR INSERT TO openvietgis_runtime
        WITH CHECK(organization_id=
          NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      GRANT SELECT,INSERT,UPDATE,DELETE ON extensions.partner_contributions TO openvietgis_runtime;
      GRANT SELECT,INSERT ON extensions.integration_events TO openvietgis_runtime;
      GRANT USAGE ON SEQUENCE extensions.integration_events_sequence_seq TO openvietgis_runtime;
      CREATE FUNCTION extensions.capture_changeset_event() RETURNS trigger LANGUAGE plpgsql
        SET search_path=pg_catalog,extensions AS $$
      BEGIN
        INSERT INTO extensions.integration_events
          (id,organization_id,project_id,event_kind,resource_type,resource_id,revision_ref,created_at)
        VALUES(NEW.id,NEW.organization_id,NEW.project_id,
          CASE WHEN NEW.reverted_from_changeset_id IS NOT NULL OR NEW.status='reverted'
            THEN 'changeset.reverted' ELSE 'changeset.committed' END,
          'changeset',NEW.id,NEW.resulting_content_revision,NEW.created_at);
        RETURN NEW;
      END $$;
      CREATE FUNCTION extensions.capture_review_event() RETURNS trigger LANGUAGE plpgsql
        SET search_path=pg_catalog,extensions AS $$
      BEGIN
        IF NEW.event_kind IN ('review.requested','review.commented','review.approved',
                              'review.changes_requested') THEN
          INSERT INTO extensions.integration_events
          (id,organization_id,project_id,event_kind,resource_type,resource_id,revision_ref,created_at)
          VALUES(NEW.id,NEW.organization_id,NEW.project_id,NEW.event_kind,NEW.resource_type,NEW.resource_id,NEW.revision_ref,NEW.created_at);
        END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER integration_changeset_event AFTER INSERT ON editing.changesets
        FOR EACH ROW EXECUTE FUNCTION extensions.capture_changeset_event();
      CREATE TRIGGER integration_review_event AFTER INSERT ON collaboration.activities
        FOR EACH ROW EXECUTE FUNCTION extensions.capture_review_event();
    """)


def downgrade():
    op.execute("""
      DROP TRIGGER integration_review_event ON collaboration.activities;
      DROP TRIGGER integration_changeset_event ON editing.changesets;
      DROP FUNCTION extensions.capture_review_event();
      DROP FUNCTION extensions.capture_changeset_event();
      DROP TABLE extensions.integration_events;
      DROP TABLE extensions.partner_contributions;
    """)
