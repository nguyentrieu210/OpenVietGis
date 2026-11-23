"""R013 Field & Offline packages and idempotent outbox sync batches with RLS."""

from __future__ import annotations

from alembic import op

revision = "0016_field_offline"
down_revision = "0015_cad_precision"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("CREATE SCHEMA field")
    op.execute("GRANT USAGE ON SCHEMA field TO openvietgis_runtime")

    op.execute("""
        CREATE TABLE field.offline_packages (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            name varchar(200) NOT NULL,
            profile varchar(32) NOT NULL DEFAULT 'standard_pwa',
            aoi_bbox jsonb NOT NULL,
            crs_code integer NOT NULL,
            layer_pins jsonb NOT NULL,
            feature_count integer NOT NULL,
            storage_key varchar(500) NOT NULL,
            sha256 varchar(64) NOT NULL,
            byte_size bigint NOT NULL,
            expires_at timestamptz NOT NULL,
            revoked_at timestamptz NULL,
            created_by_actor_id uuid NOT NULL,
            created_at timestamptz NOT NULL,
            CONSTRAINT ck_field_pkg_profile CHECK (profile IN ('standard_pwa')),
            CONSTRAINT ck_field_pkg_sha CHECK (sha256 ~ '^[a-f0-9]{64}$'),
            CONSTRAINT fk_field_pkg_project
                FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects(organization_id, id)
                ON DELETE RESTRICT
        )
    """)

    op.execute("""
        CREATE TABLE field.sync_batches (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            package_id uuid NOT NULL,
            operation_id varchar(128) NOT NULL,
            status varchar(32) NOT NULL,
            submitted_ops_count integer NOT NULL,
            applied_changeset_ids jsonb NOT NULL,
            conflicts jsonb NOT NULL,
            actor_id uuid NOT NULL,
            created_at timestamptz NOT NULL,
            CONSTRAINT uq_field_sync_op UNIQUE (organization_id, project_id, operation_id),
            CONSTRAINT ck_field_sync_status CHECK (status IN ('applied', 'conflict', 'rejected')),
            CONSTRAINT fk_field_sync_pkg
                FOREIGN KEY (package_id)
                REFERENCES field.offline_packages(id)
                ON DELETE RESTRICT
        )
    """)

    for table in ("offline_packages", "sync_batches"):
        op.execute(f"ALTER TABLE field.{table} ENABLE ROW LEVEL SECURITY")
        op.execute(f"ALTER TABLE field.{table} FORCE ROW LEVEL SECURITY")
        op.execute(f"""
            CREATE POLICY field_{table}_project_scope ON field.{table}
            FOR ALL TO openvietgis_runtime
            USING (
                organization_id = NULLIF(
                    current_setting('openvietgis.organization_id', true), ''
                )::uuid
                AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
            )
            WITH CHECK (
                organization_id = NULLIF(
                    current_setting('openvietgis.organization_id', true), ''
                )::uuid
                AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
            )
        """)

    op.execute("GRANT SELECT, INSERT, UPDATE ON field.offline_packages TO openvietgis_runtime")
    op.execute("GRANT SELECT, INSERT ON field.sync_batches TO openvietgis_runtime")


def downgrade() -> None:
    op.execute("DROP TABLE field.sync_batches")
    op.execute("DROP TABLE field.offline_packages")
    op.execute("DROP SCHEMA field")
