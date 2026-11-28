"""Immutable approval inputs for every newly submitted proposal revision."""
from alembic import op

revision = "hn_0013_proposal_pins"
down_revision = "hn_0012_generic_edit_guards"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.proposal_submission_pins(
        organization_id uuid NOT NULL, project_id uuid NOT NULL,
        proposal_id uuid NOT NULL, submitted_revision bigint NOT NULL CHECK(submitted_revision>1),
        submitted_by uuid NOT NULL, input_digest char(64) NOT NULL CHECK(input_digest ~ '^[a-f0-9]{64}$'),
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,proposal_id,submitted_revision),
        FOREIGN KEY(organization_id,project_id,proposal_id)
          REFERENCES house_numbering.proposals(organization_id,project_id,id) ON DELETE RESTRICT
      );
      ALTER TABLE house_numbering.proposal_submission_pins ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.proposal_submission_pins FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_proposal_pins_scope ON house_numbering.proposal_submission_pins
        FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
          AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      GRANT SELECT,INSERT ON house_numbering.proposal_submission_pins TO openvietgis_runtime;
    """)


def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.proposal_submission_pins) THEN
        RAISE EXCEPTION 'Submission evidence exists; retain history before downgrade'; END IF; END $$;
      DROP TABLE house_numbering.proposal_submission_pins;
    """)
