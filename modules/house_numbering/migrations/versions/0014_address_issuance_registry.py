"""Record externally issued certificates and independent plate lifecycle."""
from alembic import op

revision = "hn_0014_issuance_registry"
down_revision = "hn_0013_proposal_pins"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.issuance_records(
        id uuid PRIMARY KEY, organization_id uuid NOT NULL, project_id uuid NOT NULL,
        proposal_id uuid NOT NULL, building_id uuid NOT NULL, number_id uuid NOT NULL, route_id uuid NOT NULL,
        number_text varchar(200) NOT NULL, route_name varchar(200) NOT NULL,
        authority_name varchar(200) NOT NULL CHECK(length(btrim(authority_name))>0),
        decision_ref varchar(200) NOT NULL CHECK(length(btrim(decision_ref))>0),
        certificate_ref varchar(200) NOT NULL CHECK(length(btrim(certificate_ref))>0),
        effective_date date NOT NULL, template_revision varchar(100) NOT NULL CHECK(length(btrim(template_revision))>0),
        recorded_by uuid NOT NULL, recorded_at timestamptz NOT NULL DEFAULT now(),
        UNIQUE(organization_id,project_id,id),
        UNIQUE(organization_id,project_id,number_id),
        UNIQUE(organization_id,project_id,authority_name,certificate_ref),
        FOREIGN KEY(organization_id,project_id,proposal_id) REFERENCES house_numbering.proposals(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,building_id) REFERENCES house_numbering.buildings(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,number_id) REFERENCES house_numbering.address_numbers(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,route_id) REFERENCES house_numbering.routes(organization_id,project_id,id)
      );
      CREATE INDEX ix_hn_issuance_building ON house_numbering.issuance_records(organization_id,project_id,building_id);
      CREATE TABLE house_numbering.issuance_events(
        organization_id uuid NOT NULL, project_id uuid NOT NULL, issuance_id uuid NOT NULL,
        revision bigint NOT NULL CHECK(revision>0),
        state varchar(20) NOT NULL CHECK(state IN ('recorded','printed','delivered','installed','void')),
        evidence_ref varchar(500) NOT NULL CHECK(length(btrim(evidence_ref))>0),
        actor_id uuid NOT NULL, occurred_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,issuance_id,revision),
        FOREIGN KEY(organization_id,project_id,issuance_id) REFERENCES house_numbering.issuance_records(organization_id,project_id,id)
      );
      ALTER TABLE house_numbering.issuance_records ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.issuance_records FORCE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.issuance_events ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.issuance_events FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_issuance_scope ON house_numbering.issuance_records FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      CREATE POLICY hn_issuance_events_scope ON house_numbering.issuance_events FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      REVOKE ALL ON house_numbering.issuance_records,house_numbering.issuance_events FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT,INSERT ON house_numbering.issuance_records,house_numbering.issuance_events TO openvietgis_runtime;
      -- Serialize appends and enforce the state machine even for other trusted SQL adapters.
      CREATE FUNCTION house_numbering.check_issuance_event() RETURNS trigger
      LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
      DECLARE previous_state text; previous_revision bigint;
      BEGIN
        -- Project write authorization already serializes normal HTTP writes.
        -- A separate transaction-scoped lock also protects direct concurrent appends.
        PERFORM pg_advisory_xact_lock(hashtextextended('hn_issuance:'||NEW.issuance_id::text,0));
        SELECT state,revision INTO previous_state,previous_revision
        FROM house_numbering.issuance_events WHERE organization_id=NEW.organization_id
          AND project_id=NEW.project_id AND issuance_id=NEW.issuance_id ORDER BY revision DESC LIMIT 1;
        IF NEW.revision<>coalesce(previous_revision,0)+1 OR NOT coalesce(
          (previous_state IS NULL AND NEW.state='recorded') OR
          (previous_state='recorded' AND NEW.state='printed') OR
          (previous_state='printed' AND NEW.state='delivered') OR
          (previous_state='delivered' AND NEW.state='installed') OR
          (previous_state IS NOT NULL AND previous_state<>'void' AND NEW.state='void'),false)
        THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Invalid or stale issuance transition'; END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER trg_issuance_event BEFORE INSERT ON house_numbering.issuance_events
        FOR EACH ROW EXECUTE FUNCTION house_numbering.check_issuance_event();
    """)

def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.issuance_records) THEN
        RAISE EXCEPTION 'Issuance history exists; retain before downgrade'; END IF; END $$;
      DROP TABLE house_numbering.issuance_events;
      DROP FUNCTION house_numbering.check_issuance_event();
      DROP TABLE house_numbering.issuance_records;
    """)
