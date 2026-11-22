"""Notify project clients after a GIS changeset commits."""
from alembic import op

revision = "0031_spatial_presence_events"
down_revision = "0030_collaboration_realtime"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
    CREATE FUNCTION collaboration.publish_spatial_change() RETURNS trigger
    LANGUAGE plpgsql AS $$
    BEGIN
        PERFORM pg_notify('ovg_team_collaboration', json_build_object(
            'o', NEW.organization_id, 'p', NEW.project_id,
            'a', NULL, 'b', NULL, 'kind', 'spatial_change')::text);
        RETURN NEW;
    END $$;
    CREATE TRIGGER collaboration_spatial_changeset
    AFTER INSERT OR UPDATE OF status ON editing.changesets
    FOR EACH ROW EXECUTE FUNCTION collaboration.publish_spatial_change();
    """)


def downgrade():
    op.execute("DROP TRIGGER collaboration_spatial_changeset ON editing.changesets")
    op.execute("DROP FUNCTION collaboration.publish_spatial_change()")
