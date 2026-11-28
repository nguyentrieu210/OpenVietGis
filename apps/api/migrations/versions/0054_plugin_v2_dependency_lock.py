"""Pin approved dependency graph and minimum reviewed epoch to each grant."""
from alembic import op

revision = "0054_plugin_v2_dependency_lock"
down_revision = "0053_plugin_v2_grants"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
        ALTER TABLE extensions.plugin_v2_grant_events
          ADD COLUMN dependency_lock jsonb,
          ADD COLUMN lock_digest varchar(64),
          ADD COLUMN catalog_epoch bigint,
          ADD CONSTRAINT plugin_v2_lock_shape CHECK (
            (dependency_lock IS NULL AND lock_digest IS NULL AND catalog_epoch IS NULL)
            OR (dependency_lock IS NOT NULL AND lock_digest IS NOT NULL AND catalog_epoch IS NOT NULL
                AND jsonb_typeof(dependency_lock)='array'
                AND jsonb_array_length(dependency_lock) BETWEEN 1 AND 100
                AND octet_length(dependency_lock::text)<=262144
                AND lock_digest ~ '^[a-f0-9]{64}$'
                AND catalog_epoch BETWEEN 1 AND 9007199254740991))
    """)
    op.execute("CREATE INDEX plugin_v2_grants_epoch ON extensions.plugin_v2_grant_events (organization_id,project_id,catalog_epoch DESC) WHERE catalog_epoch IS NOT NULL")

def downgrade():
    # A downgrade would discard immutable approval evidence, including on old grants.
    op.execute("ALTER TABLE extensions.plugin_v2_grant_events NO FORCE ROW LEVEL SECURITY")
    if op.get_bind().exec_driver_sql("SELECT EXISTS(SELECT 1 FROM extensions.plugin_v2_grant_events WHERE dependency_lock IS NOT NULL)").scalar():
        raise RuntimeError("Archive dependency approval evidence before downgrade")
    op.execute("DROP INDEX extensions.plugin_v2_grants_epoch")
    op.execute("ALTER TABLE extensions.plugin_v2_grant_events DROP CONSTRAINT plugin_v2_lock_shape, DROP COLUMN dependency_lock, DROP COLUMN lock_digest, DROP COLUMN catalog_epoch")
    op.execute("ALTER TABLE extensions.plugin_v2_grant_events FORCE ROW LEVEL SECURITY")
