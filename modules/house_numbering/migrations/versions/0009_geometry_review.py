"""Separate review-stage geometry from active numbering routes."""
from alembic import op

revision="hn_0009_geometry_review"
down_revision="hn_0008_reviewed_route_aliases"
branch_labels=None
depends_on=None

def upgrade():
    op.execute("""
        CREATE TABLE house_numbering.geometry_reviews (
            id uuid PRIMARY KEY,
            organization_id uuid NOT NULL,
            project_id uuid NOT NULL,
            route_code varchar(160) NOT NULL,
            snapshot jsonb NOT NULL CHECK (jsonb_typeof(snapshot)='object'),
            snapshot_token char(64) NOT NULL,
            reason varchar(500) NOT NULL CHECK(length(btrim(reason))>=10),
            revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
            created_by_actor_id uuid NOT NULL,
            updated_by_actor_id uuid NOT NULL,
            created_at timestamptz NOT NULL DEFAULT now(),
            updated_at timestamptz NOT NULL DEFAULT now(),
            UNIQUE (organization_id, project_id, route_code),
            FOREIGN KEY (organization_id, project_id) REFERENCES workspace.projects(organization_id,id) ON DELETE RESTRICT
        );
        ALTER TABLE house_numbering.geometry_reviews ENABLE ROW LEVEL SECURITY;
        ALTER TABLE house_numbering.geometry_reviews FORCE ROW LEVEL SECURITY;
        CREATE POLICY hn_geometry_reviews_project_scope ON house_numbering.geometry_reviews
          FOR ALL TO openvietgis_runtime
          USING (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
             AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
          WITH CHECK (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
             AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
        GRANT SELECT, INSERT, UPDATE ON house_numbering.geometry_reviews TO openvietgis_runtime;
    """)

def downgrade():
    op.execute("DROP TABLE house_numbering.geometry_reviews")
