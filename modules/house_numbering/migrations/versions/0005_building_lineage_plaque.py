"""Track plaque lifecycle and immutable building split/merge lineage."""

from __future__ import annotations

from alembic import op

revision = "hn_0005_building_lineage_plaque"
down_revision = "hn_0004_survey_gps_delegation"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("""
        ALTER TABLE house_numbering.buildings
        ADD COLUMN plaque_status varchar(20) NOT NULL DEFAULT 'not_planned',
        ADD COLUMN plaque_updated_at timestamptz NULL,
        ADD CONSTRAINT ck_hn_building_plaque_status
            CHECK (plaque_status IN (
                'not_planned','planned','installed','replaced','removed'
            ))
    """)
    op.execute("""
        CREATE TABLE house_numbering.building_lineage (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            operation_id uuid NOT NULL,
            relation_kind varchar(16) NOT NULL,
            predecessor_building_id uuid NOT NULL,
            successor_building_id uuid NOT NULL,
            created_by_actor_id uuid NOT NULL,
            created_at timestamptz NOT NULL,
            CONSTRAINT uq_hn_lineage_scope
                UNIQUE (organization_id, project_id, id),
            CONSTRAINT uq_hn_lineage_operation_edge
                UNIQUE (
                    organization_id, project_id, operation_id,
                    predecessor_building_id, successor_building_id
                ),
            CONSTRAINT fk_hn_lineage_project
                FOREIGN KEY (organization_id, project_id)
                REFERENCES workspace.projects(organization_id, id)
                ON DELETE RESTRICT,
            CONSTRAINT fk_hn_lineage_predecessor
                FOREIGN KEY (
                    organization_id, project_id, predecessor_building_id
                )
                REFERENCES house_numbering.buildings(
                    organization_id, project_id, id
                )
                ON DELETE RESTRICT,
            CONSTRAINT fk_hn_lineage_successor
                FOREIGN KEY (
                    organization_id, project_id, successor_building_id
                )
                REFERENCES house_numbering.buildings(
                    organization_id, project_id, id
                )
                ON DELETE RESTRICT,
            CONSTRAINT ck_hn_lineage_kind
                CHECK (relation_kind IN ('split','merge')),
            CONSTRAINT ck_hn_lineage_distinct
                CHECK (predecessor_building_id <> successor_building_id)
        )
    """)
    op.execute("""
        CREATE INDEX ix_hn_lineage_predecessor
        ON house_numbering.building_lineage (
            organization_id, project_id, predecessor_building_id, created_at
        )
    """)
    op.execute("""
        CREATE INDEX ix_hn_lineage_successor
        ON house_numbering.building_lineage (
            organization_id, project_id, successor_building_id, created_at
        )
    """)
    op.execute("ALTER TABLE house_numbering.building_lineage ENABLE ROW LEVEL SECURITY")
    op.execute("ALTER TABLE house_numbering.building_lineage FORCE ROW LEVEL SECURITY")
    op.execute("""
        CREATE POLICY hn_building_lineage_project_scope
        ON house_numbering.building_lineage
        FOR ALL TO openvietgis_runtime
        USING (
            organization_id = NULLIF(
                current_setting('openvietgis.organization_id', true), ''
            )::uuid
            AND project_id = NULLIF(
                current_setting('openvietgis.project_id', true), ''
            )::uuid
        )
        WITH CHECK (
            organization_id = NULLIF(
                current_setting('openvietgis.organization_id', true), ''
            )::uuid
            AND project_id = NULLIF(
                current_setting('openvietgis.project_id', true), ''
            )::uuid
        )
    """)
    op.execute(
        "GRANT SELECT, INSERT ON house_numbering.building_lineage "
        "TO openvietgis_runtime"
    )


def downgrade() -> None:
    op.execute("DROP TABLE IF EXISTS house_numbering.building_lineage")
    op.execute("""
        ALTER TABLE house_numbering.buildings
        DROP CONSTRAINT IF EXISTS ck_hn_building_plaque_status,
        DROP COLUMN plaque_updated_at,
        DROP COLUMN plaque_status
    """)
