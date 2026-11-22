"""Private messages, task receipts, and commit-driven collaboration events."""
from alembic import op

revision = "0030_collaboration_realtime"
down_revision = "0029_team_chat_tasks"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      ALTER TABLE collaboration.messages ADD COLUMN recipient_id uuid REFERENCES identity.users(id);
      ALTER TABLE collaboration.messages ADD CONSTRAINT distinct_dm_participants CHECK(recipient_id IS NULL OR recipient_id<>author_id);
      CREATE INDEX direct_messages_recent ON collaboration.messages(organization_id,project_id,recipient_id,author_id,created_at DESC);
      ALTER TABLE collaboration.tasks ADD COLUMN source_message_id uuid;
      ALTER TABLE collaboration.tasks ADD CONSTRAINT task_source_message_fk FOREIGN KEY(organization_id,project_id,source_message_id)
        REFERENCES collaboration.messages(organization_id,project_id,id);
      CREATE TABLE collaboration.task_notification_reads (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, task_id uuid NOT NULL, user_id uuid NOT NULL,
        read_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,task_id,user_id),
        FOREIGN KEY(organization_id,project_id,task_id) REFERENCES collaboration.tasks(organization_id,project_id,id)
      );
    """)
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id)"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
    op.execute("DROP POLICY messages_read ON collaboration.messages")
    op.execute(f"""CREATE POLICY messages_read ON collaboration.messages FOR SELECT TO openvietgis_runtime
      USING ({scope} AND {actor} IS NOT NULL AND (recipient_id IS NULL OR {actor} IN (author_id,recipient_id)))""")
    op.execute("DROP POLICY tasks_create ON collaboration.tasks")
    op.execute(f"""CREATE POLICY tasks_create ON collaboration.tasks FOR INSERT TO openvietgis_runtime WITH CHECK
      ({scope} AND creator_id={actor} AND (source_message_id IS NULL OR EXISTS(
        SELECT 1 FROM collaboration.messages m WHERE m.organization_id=tasks.organization_id AND m.project_id=tasks.project_id
        AND m.id=tasks.source_message_id AND m.recipient_id IS NULL)))""")
    op.execute("ALTER TABLE collaboration.task_notification_reads ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE collaboration.task_notification_reads FORCE ROW LEVEL SECURITY")
    op.execute(f"""CREATE POLICY notification_own_read ON collaboration.task_notification_reads FOR SELECT TO openvietgis_runtime
      USING ({scope} AND user_id={actor})""")
    op.execute(f"""CREATE POLICY notification_own_create ON collaboration.task_notification_reads FOR INSERT TO openvietgis_runtime
      WITH CHECK ({scope} AND user_id={actor} AND EXISTS (
        SELECT 1 FROM collaboration.tasks t WHERE t.organization_id=task_notification_reads.organization_id
        AND t.project_id=task_notification_reads.project_id AND t.id=task_notification_reads.task_id
        AND t.assignee_id={actor} AND t.creator_id<>{actor}))""")
    op.execute("GRANT SELECT,INSERT ON collaboration.task_notification_reads TO openvietgis_runtime")
    op.execute("""
      CREATE FUNCTION collaboration.publish_change() RETURNS trigger LANGUAGE plpgsql AS $$
      DECLARE a uuid; b uuid;
      BEGIN
        IF TG_TABLE_NAME='messages' THEN
          IF NEW.recipient_id IS NOT NULL THEN a:=NEW.author_id; b:=NEW.recipient_id; END IF;
        ELSIF TG_TABLE_NAME='task_notification_reads' THEN
          a:=NEW.user_id;
        END IF;
        PERFORM pg_notify('ovg_team_collaboration',json_build_object(
          'o',NEW.organization_id,'p',NEW.project_id,'a',a,'b',b)::text);
        RETURN NEW;
      END;
      $$;
      CREATE TRIGGER messages_realtime AFTER INSERT ON collaboration.messages FOR EACH ROW EXECUTE FUNCTION collaboration.publish_change();
      CREATE TRIGGER tasks_realtime AFTER INSERT OR UPDATE ON collaboration.tasks FOR EACH ROW EXECUTE FUNCTION collaboration.publish_change();
      CREATE TRIGGER reads_realtime AFTER INSERT ON collaboration.task_notification_reads FOR EACH ROW EXECUTE FUNCTION collaboration.publish_change();
    """)


def downgrade():
    # Refuse silent conversion of private chats into public messages.
    # A migration owner may be subject to FORCE RLS; temporarily remove FORCE
    # inside this transactional migration before inspecting for private rows.
    op.execute("ALTER TABLE collaboration.messages NO FORCE ROW LEVEL SECURITY")
    op.execute("""DO $ BEGIN IF EXISTS(SELECT 1 FROM collaboration.messages WHERE recipient_id IS NOT NULL) THEN
      RAISE EXCEPTION 'Private messages exist; backup and explicit migration plan required'; END IF; END $""")
    op.execute("ALTER TABLE collaboration.messages FORCE ROW LEVEL SECURITY")
    op.execute("DROP POLICY tasks_create ON collaboration.tasks")
    op.execute("DROP TRIGGER reads_realtime ON collaboration.task_notification_reads")
    op.execute("DROP TRIGGER tasks_realtime ON collaboration.tasks")
    op.execute("DROP TRIGGER messages_realtime ON collaboration.messages")
    op.execute("DROP FUNCTION collaboration.publish_change()")
    op.execute("DROP TABLE collaboration.task_notification_reads")
    op.execute("ALTER TABLE collaboration.tasks DROP COLUMN source_message_id")
    op.execute("DROP POLICY messages_read ON collaboration.messages")
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id)"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
    op.execute(f"CREATE POLICY messages_read ON collaboration.messages FOR SELECT TO openvietgis_runtime USING ({scope} AND {actor} IS NOT NULL)")
    op.execute(f"CREATE POLICY tasks_create ON collaboration.tasks FOR INSERT TO openvietgis_runtime WITH CHECK ({scope} AND creator_id={actor})")
    op.execute("ALTER TABLE collaboration.messages DROP COLUMN recipient_id")
