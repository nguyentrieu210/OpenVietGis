"""Project chat and assigned tasks, scoped to full project members."""

from alembic import op

revision = "0029_team_chat_tasks"
down_revision = "0028_marketplace_developers"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE collaboration.messages (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, id uuid NOT NULL,
        author_id uuid NOT NULL REFERENCES identity.users(id),
        author_name varchar(200) NOT NULL,
        body varchar(4000) NOT NULL CHECK(length(trim(body))>0),
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id)
      );
      CREATE TABLE collaboration.tasks (
        organization_id uuid NOT NULL, project_id uuid NOT NULL, id uuid NOT NULL,
        creator_id uuid NOT NULL REFERENCES identity.users(id),
        creator_name varchar(200) NOT NULL,
        assignee_id uuid NOT NULL REFERENCES identity.users(id),
        assignee_name varchar(200) NOT NULL,
        title varchar(200) NOT NULL CHECK(length(trim(title))>0),
        description varchar(2000) NOT NULL DEFAULT '',
        status varchar(16) NOT NULL DEFAULT 'todo' CHECK(status IN ('todo','in_progress','done')),
        priority varchar(16) NOT NULL CHECK(priority IN ('low','normal','high')),
        due_date date, longitude double precision, latitude double precision,
        CHECK ((longitude IS NULL AND latitude IS NULL) OR
               (longitude IS NOT NULL AND latitude IS NOT NULL AND longitude BETWEEN -180 AND
                 180 AND latitude BETWEEN -90 AND 90)),
        revision integer NOT NULL DEFAULT 1 CHECK(revision>=1),
        created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT
          now(),
        PRIMARY KEY(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id)
      );
      CREATE INDEX messages_recent ON
        collaboration.messages(organization_id,project_id,created_at DESC,id);
      CREATE INDEX tasks_recent ON collaboration.tasks(organization_id,project_id,updated_at
        DESC,id);
      CREATE INDEX tasks_assignee ON
        collaboration.tasks(organization_id,project_id,assignee_id,status);
    """)
    scope = """organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
      AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid
      AND NOT gis_access.scoped(organization_id,project_id)"""
    actor = "NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid"
    for table, author in (("messages", "author_id"), ("tasks", "creator_id")):
        op.execute(f"ALTER TABLE collaboration.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE collaboration.{table} FORCE ROW LEVEL SECURITY")
        op.execute(
            f"CREATE POLICY {table}_read ON collaboration.{table} "
            f"FOR SELECT TO openvietgis_runtime USING ({scope} AND {actor} IS NOT NULL)"
        )
        op.execute(
            f"CREATE POLICY {table}_create ON collaboration.{table} FOR INSERT TO "
            f"openvietgis_runtime WITH CHECK ({scope} AND {author}={actor})"
        )
        op.execute(f"GRANT SELECT,INSERT ON collaboration.{table} TO openvietgis_runtime")
    op.execute(f"""CREATE POLICY tasks_update ON collaboration.tasks FOR UPDATE TO
      openvietgis_runtime
      USING ({scope} AND {actor} IN (creator_id,assignee_id))
      WITH CHECK ({scope} AND {actor} IN (creator_id,assignee_id))""")
    op.execute(
        "GRANT UPDATE(status,revision,updated_at) ON collaboration.tasks TO openvietgis_runtime"
    )


def downgrade():
    op.execute("DROP TABLE collaboration.tasks")
    op.execute("DROP TABLE collaboration.messages")
