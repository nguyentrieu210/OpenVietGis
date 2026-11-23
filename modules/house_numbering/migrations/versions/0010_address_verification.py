"""Per-building address decisions scoped to a staged geometry candidate."""

from alembic import op

revision = "hn_0010_address_verification"
down_revision = "hn_0009_geometry_review"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      ALTER TABLE house_numbering.geometry_reviews ADD CONSTRAINT uq_hn_geometry_review_scope UNIQUE(organization_id,project_id,id);
      CREATE TABLE house_numbering.geometry_address_reviews(
        id uuid PRIMARY KEY, organization_id uuid NOT NULL, project_id uuid NOT NULL,
        geometry_review_id uuid NOT NULL, building_id uuid NOT NULL,
        evidence_token char(64) NOT NULL, decision varchar(16) NOT NULL CHECK(decision IN ('belongs','other_address')),
        verified_road varchar(200) NOT NULL, verified_prefix varchar(100) NOT NULL,
        evidence varchar(500) NOT NULL CHECK(length(btrim(evidence))>=10),
        revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
        created_by_actor_id uuid NOT NULL, updated_by_actor_id uuid NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
        UNIQUE(organization_id,project_id,geometry_review_id,building_id),
        FOREIGN KEY(organization_id,project_id,geometry_review_id) REFERENCES house_numbering.geometry_reviews(organization_id,project_id,id) ON DELETE RESTRICT,
        FOREIGN KEY(organization_id,project_id,building_id) REFERENCES house_numbering.buildings(organization_id,project_id,id) ON DELETE RESTRICT
      );
      ALTER TABLE house_numbering.geometry_address_reviews ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.geometry_address_reviews FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_geometry_address_reviews_scope ON house_numbering.geometry_address_reviews
      FOR ALL TO openvietgis_runtime
      USING (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
      WITH CHECK (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
        AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      GRANT SELECT,INSERT,UPDATE ON house_numbering.geometry_address_reviews TO openvietgis_runtime;
    """)


def downgrade():
    op.execute(
        "DROP TABLE house_numbering.geometry_address_reviews; ALTER TABLE house_numbering.geometry_reviews DROP CONSTRAINT uq_hn_geometry_review_scope"
    )
