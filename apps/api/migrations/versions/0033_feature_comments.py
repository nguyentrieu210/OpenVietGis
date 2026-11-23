"""Feature discussion threads and mention notifications."""

from alembic import op

revision = "0033_feature_comments"
down_revision = "0032_feature_edit_leases"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
    ALTER TABLE collaboration.messages ADD COLUMN source_id uuid, ADD COLUMN feature_id
    varchar(128),
      ADD COLUMN longitude double precision, ADD COLUMN latitude double precision,
      ADD COLUMN mentions jsonb NOT NULL DEFAULT '[]'::jsonb;
    ALTER TABLE collaboration.messages ADD CONSTRAINT message_feature_pair CHECK (
      (source_id IS NULL AND feature_id IS NULL) OR (source_id IS NOT NULL AND feature_id IS NOT
      NULL AND recipient_id IS NULL));
    ALTER TABLE collaboration.messages ADD CONSTRAINT message_source_fk FOREIGN
    KEY(organization_id,project_id,source_id)
      REFERENCES spatial.data_sources(organization_id,project_id,id);
    CREATE INDEX feature_comments_recent ON collaboration.messages(organization_id,project_id,
    source_id,feature_id,created_at DESC);
    ALTER TABLE collaboration.tasks ADD COLUMN source_id uuid, ADD COLUMN feature_id varchar(128);
    ALTER TABLE collaboration.tasks ADD CONSTRAINT task_feature_pair CHECK ((source_id IS
    NULL)=(feature_id IS NULL));
    ALTER TABLE collaboration.tasks ADD CONSTRAINT task_spatial_source_fk FOREIGN
    KEY(organization_id,project_id,source_id)
      REFERENCES spatial.data_sources(organization_id,project_id,id);
    CREATE TABLE collaboration.comment_mentions (
      organization_id uuid NOT NULL, project_id uuid NOT NULL, message_id uuid NOT NULL,
      user_id uuid NOT NULL REFERENCES identity.users(id), read_at timestamptz,
      PRIMARY KEY(organization_id,project_id,message_id,user_id),
      FOREIGN KEY(organization_id,project_id,message_id) REFERENCES
      collaboration.messages(organization_id,project_id,id)
    );
    CREATE INDEX comment_mentions_inbox ON collaboration.comment_mentions(organization_id,
    project_id,user_id,read_at);
    ALTER TABLE collaboration.comment_mentions ENABLE ROW LEVEL SECURITY;
    ALTER TABLE collaboration.comment_mentions FORCE ROW LEVEL SECURITY;
    """)
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id)"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
    op.execute(
        f"CREATE POLICY mentions_read ON collaboration.comment_mentions FOR SELECT TO "
        f"openvietgis_runtime USING ({scope} AND user_id={actor})"
    )
    op.execute(f"""CREATE POLICY mentions_create ON collaboration.comment_mentions FOR INSERT TO
    openvietgis_runtime WITH CHECK ({scope}
      AND user_id<>{actor} AND EXISTS(SELECT 1 FROM collaboration.messages m WHERE
      m.organization_id=comment_mentions.organization_id
      AND m.project_id=comment_mentions.project_id AND m.id=comment_mentions.message_id AND
      m.source_id IS NOT NULL AND m.author_id={actor}))""")
    op.execute(
        f"CREATE POLICY mentions_update ON collaboration.comment_mentions FOR UPDATE TO "
        f"openvietgis_runtime USING ({scope} AND user_id={actor}) WITH CHECK ({scope} AND "
        f"user_id={actor})"
    )
    op.execute(
        "GRANT SELECT,INSERT,UPDATE(read_at) ON collaboration.comment_mentions TO "
        "openvietgis_runtime"
    )
    op.execute("""
    CREATE FUNCTION collaboration.publish_mention_read() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN
      PERFORM pg_notify('ovg_team_collaboration',json_build_object('o',NEW.organization_id,'p',
      NEW.project_id,'a',NEW.user_id,'b',NULL)::text);
      RETURN NEW;
    END $$;
    CREATE TRIGGER mention_read_event AFTER UPDATE OF read_at ON collaboration.comment_mentions
      FOR EACH ROW EXECUTE FUNCTION collaboration.publish_mention_read();
    """)


def downgrade():
    # Never silently turn saved feature comments into regular chat messages.
    op.execute("ALTER TABLE collaboration.messages NO FORCE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE collaboration.tasks NO FORCE ROW LEVEL SECURITY")
    op.execute("""DO $$ BEGIN IF EXISTS(SELECT 1 FROM collaboration.messages WHERE source_id IS
    NOT NULL)
      OR EXISTS(SELECT 1 FROM collaboration.tasks WHERE source_id IS NOT NULL) THEN
      RAISE EXCEPTION 'Feature-linked collaboration data exists; explicit migration required';
      END IF; END $$;""")
    op.execute("ALTER TABLE collaboration.messages FORCE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE collaboration.tasks FORCE ROW LEVEL SECURITY")
    op.execute("DROP TABLE collaboration.comment_mentions")
    op.execute("DROP FUNCTION collaboration.publish_mention_read()")
    op.execute("ALTER TABLE collaboration.tasks DROP COLUMN source_id, DROP COLUMN feature_id")
    op.execute(
        "ALTER TABLE collaboration.messages DROP COLUMN source_id, DROP COLUMN feature_id, DROP "
        "COLUMN longitude, DROP COLUMN latitude, DROP COLUMN mentions"
    )
