"""Persist account avatars with self-only database access."""

from alembic import op

revision = "0023_user_avatar"
down_revision = "0022_schema_change_evidence"
branch_labels = None
depends_on = None


def upgrade():
    op.execute(
        "CREATE TABLE identity.user_avatars (\n      user_id "
        "uuid PRIMARY KEY REFERENCES identity.users(id) ON "
        "DELETE CASCADE,\n      data_url text NOT NULL "
        "CHECK(length(data_url)<=150000 AND data_url LIKE "
        "'data:image/jpeg;base64,%'),\n      updated_at "
        "timestamptz NOT NULL DEFAULT now()\n    )"
    )
    op.execute("ALTER TABLE identity.user_avatars ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE identity.user_avatars FORCE ROW LEVEL SECURITY")
    op.execute("""CREATE POLICY self_avatar ON identity.user_avatars FOR ALL TO openvietgis_runtime
      USING(user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid)
      WITH CHECK(user_id=NULLIF(current_setting('openvietgis.actor_user_id',true),'')::uuid)""")
    op.execute("GRANT SELECT,INSERT,UPDATE,DELETE ON identity.user_avatars TO openvietgis_runtime")


def downgrade():
    op.execute("DROP TABLE identity.user_avatars")
