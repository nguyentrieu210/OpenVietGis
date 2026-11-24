"""R029 Managed Realtime GIS Client: sessions, commit-ordered cursor ledger, and AI command bridge.

Strictly additive migration (down_revision = 0040_r027_pairing_and_conflicts):
- integration_hub.managed_sessions: tracked managed desktop sessions with capability flags,
  permission_epoch, last_ack_cursor, expiry, and revocation. No raw tokens stored.
- integration_hub.project_event_counters: monotonic per-project cursor + permission_epoch +
  retention_floor serialized under transaction lock.
- integration_hub.project_events: commit-ordered durable invalidation ledger atomically
  appended inside the same DB transaction as every R006 changeset via AFTER INSERT trigger.
- integration_hub.desktop_commands: P2 allowlisted typed AI desktop command queue with
  explicit consent status and TTL.
"""

from alembic import op

revision = "0041_r029_managed_realtime_client"
down_revision = "0040_r027_pairing_and_conflicts"
branch_labels = None
depends_on = None


def upgrade() -> None:
    tenant_scope = """organization_id = NULLIF(
        current_setting('openvietgis.organization_id', true), '')::uuid
        AND project_id = NULLIF(
        current_setting('openvietgis.project_id', true), '')::uuid"""

    # 1. Managed sessions ledger.
    op.execute("""
        CREATE TABLE integration_hub.managed_sessions (
            id uuid NOT NULL PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            binding_id uuid NULL,
            actor_type varchar(32) NOT NULL,
            actor_id uuid NOT NULL,
            actor_display varchar(200) NOT NULL DEFAULT '',
            device_label varchar(120) NOT NULL DEFAULT 'QGIS Desktop',
            client_version varchar(40) NOT NULL DEFAULT '0.9.0',
            mode varchar(24) NOT NULL DEFAULT 'managed'
                CHECK (mode IN ('managed', 'legacy_snapshot')),
            capabilities jsonb NOT NULL DEFAULT '{}'::jsonb,
            permission_epoch bigint NOT NULL DEFAULT 1,
            last_ack_cursor bigint NOT NULL DEFAULT 0,
            state varchar(24) NOT NULL DEFAULT 'active'
                CHECK (state IN ('active', 'paused', 'revoked', 'expired')),
            expires_at timestamptz NOT NULL,
            revoked_at timestamptz NULL,
            last_seen_at timestamptz NOT NULL DEFAULT now(),
            created_at timestamptz NOT NULL DEFAULT now()
        )
    """)
    op.execute("""
        CREATE INDEX ix_r029_managed_sessions_project
        ON integration_hub.managed_sessions
        (organization_id, project_id, state, created_at DESC)
    """)
    op.execute(
        "ALTER TABLE integration_hub.managed_sessions ENABLE ROW LEVEL SECURITY"
    )
    op.execute(
        "ALTER TABLE integration_hub.managed_sessions FORCE ROW LEVEL SECURITY"
    )
    op.execute(f"""
        CREATE POLICY r029_managed_sessions_read
        ON integration_hub.managed_sessions FOR SELECT TO openvietgis_runtime
        USING ({tenant_scope})
    """)
    op.execute(f"""
        CREATE POLICY r029_managed_sessions_insert
        ON integration_hub.managed_sessions FOR INSERT TO openvietgis_runtime
        WITH CHECK ({tenant_scope})
    """)
    op.execute(f"""
        CREATE POLICY r029_managed_sessions_update
        ON integration_hub.managed_sessions FOR UPDATE TO openvietgis_runtime
        USING ({tenant_scope})
        WITH CHECK ({tenant_scope})
    """)
    op.execute("""
        GRANT SELECT, INSERT, UPDATE
        ON integration_hub.managed_sessions TO openvietgis_runtime
    """)

    # 2. Per-project commit-ordered cursor counter & permission epoch.
    op.execute("""
        CREATE TABLE integration_hub.project_event_counters (
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            last_cursor bigint NOT NULL DEFAULT 0,
            permission_epoch bigint NOT NULL DEFAULT 1,
            retention_floor bigint NOT NULL DEFAULT 0,
            updated_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (organization_id, project_id)
        )
    """)
    op.execute(
        "ALTER TABLE integration_hub.project_event_counters ENABLE ROW LEVEL SECURITY"
    )
    op.execute(
        "ALTER TABLE integration_hub.project_event_counters FORCE ROW LEVEL SECURITY"
    )
    op.execute(f"""
        CREATE POLICY r029_project_counters_all
        ON integration_hub.project_event_counters FOR ALL TO openvietgis_runtime
        USING ({tenant_scope})
        WITH CHECK ({tenant_scope})
    """)
    op.execute("""
        GRANT SELECT, INSERT, UPDATE
        ON integration_hub.project_event_counters TO openvietgis_runtime
    """)

    # 3. Commit-ordered durable project events.
    op.execute("""
        CREATE TABLE integration_hub.project_events (
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            cursor bigint NOT NULL,
            kind varchar(32) NOT NULL
                CHECK (
                    kind IN ('changeset', 'schema_change', 'catalog_change', 'visibility_reset')
                ),
            source_id uuid NULL,
            changeset_id uuid NULL,
            actor_id uuid NULL,
            content_revision varchar(64) NULL,
            committed_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (organization_id, project_id, cursor)
        )
    """)
    op.execute("""
        CREATE INDEX ix_r029_project_events_changeset
        ON integration_hub.project_events
        (organization_id, project_id, changeset_id)
        WHERE changeset_id IS NOT NULL
    """)
    op.execute(
        "ALTER TABLE integration_hub.project_events ENABLE ROW LEVEL SECURITY"
    )
    op.execute(
        "ALTER TABLE integration_hub.project_events FORCE ROW LEVEL SECURITY"
    )
    op.execute(f"""
        CREATE POLICY r029_project_events_read
        ON integration_hub.project_events FOR SELECT TO openvietgis_runtime
        USING ({tenant_scope})
    """)
    op.execute(f"""
        CREATE POLICY r029_project_events_insert
        ON integration_hub.project_events FOR INSERT TO openvietgis_runtime
        WITH CHECK ({tenant_scope})
    """)
    op.execute("""
        GRANT SELECT, INSERT
        ON integration_hub.project_events TO openvietgis_runtime
    """)

    # 4. Atomic trigger on editing.changesets so EVERY R006 commit (Web, QGIS, Field, AI MCP)
    # gets a durable commit-ordered cursor inside the exact same DB transaction.
    op.execute("""
        CREATE OR REPLACE FUNCTION integration_hub.r029_record_changeset_event()
        RETURNS trigger
        LANGUAGE plpgsql
        SECURITY DEFINER
        SET search_path = integration_hub, editing, pg_catalog
        AS $$
        DECLARE
            next_cur bigint;
        BEGIN
            INSERT INTO integration_hub.project_event_counters (
                organization_id, project_id, last_cursor,
                permission_epoch, retention_floor, updated_at
            ) VALUES (
                NEW.organization_id, NEW.project_id, 1, 1, 0, now()
            )
            ON CONFLICT (organization_id, project_id)
            DO UPDATE SET
                last_cursor = integration_hub.project_event_counters.last_cursor + 1,
                updated_at = now()
            RETURNING last_cursor INTO next_cur;

            INSERT INTO integration_hub.project_events (
                organization_id, project_id, cursor, kind,
                source_id, changeset_id, actor_id, content_revision, committed_at
            ) VALUES (
                NEW.organization_id, NEW.project_id, next_cur, 'changeset',
                NEW.source_id, NEW.id, NEW.actor_id, NEW.resulting_content_revision, now()
            );
            RETURN NEW;
        END;
        $$;
    """)
    op.execute("""
        DROP TRIGGER IF EXISTS trg_r029_record_changeset_event ON editing.changesets
    """)
    op.execute("""
        CREATE TRIGGER trg_r029_record_changeset_event
        AFTER INSERT ON editing.changesets
        FOR EACH ROW
        EXECUTE FUNCTION integration_hub.r029_record_changeset_event()
    """)

    # 5. P2 AI Desktop Bridge allowlisted command queue.
    op.execute("""
        CREATE TABLE integration_hub.desktop_commands (
            id uuid NOT NULL PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            session_id uuid NOT NULL REFERENCES integration_hub.managed_sessions (id)
                ON DELETE CASCADE,
            tool_name varchar(64) NOT NULL,
            arguments jsonb NOT NULL DEFAULT '{}'::jsonb,
            requires_consent boolean NOT NULL DEFAULT true,
            status varchar(24) NOT NULL DEFAULT 'pending'
                CHECK (
                    status IN ('pending', 'approved', 'rejected', 'completed', 'failed', 'expired')
                ),
            result jsonb NULL,
            requested_by uuid NOT NULL,
            expires_at timestamptz NOT NULL,
            created_at timestamptz NOT NULL DEFAULT now(),
            resolved_at timestamptz NULL
        )
    """)
    op.execute("""
        CREATE INDEX ix_r029_desktop_commands_session
        ON integration_hub.desktop_commands
        (organization_id, project_id, session_id, status, created_at DESC)
    """)
    op.execute(
        "ALTER TABLE integration_hub.desktop_commands ENABLE ROW LEVEL SECURITY"
    )
    op.execute(
        "ALTER TABLE integration_hub.desktop_commands FORCE ROW LEVEL SECURITY"
    )
    op.execute(f"""
        CREATE POLICY r029_desktop_commands_all
        ON integration_hub.desktop_commands FOR ALL TO openvietgis_runtime
        USING ({tenant_scope})
        WITH CHECK ({tenant_scope})
    """)
    op.execute("""
        GRANT SELECT, INSERT, UPDATE
        ON integration_hub.desktop_commands TO openvietgis_runtime
    """)


def downgrade() -> None:
    op.execute("DROP TRIGGER IF EXISTS trg_r029_record_changeset_event ON editing.changesets")
    op.execute("DROP FUNCTION IF EXISTS integration_hub.r029_record_changeset_event()")
    op.execute("DROP TABLE IF EXISTS integration_hub.desktop_commands")
    op.execute("DROP TABLE IF EXISTS integration_hub.project_events")
    op.execute("DROP TABLE IF EXISTS integration_hub.project_event_counters")
    op.execute("DROP TABLE IF EXISTS integration_hub.managed_sessions")
