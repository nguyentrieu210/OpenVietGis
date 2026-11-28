"""Append-only same-address certificate replacement chain."""
from alembic import op

revision = "hn_0015_issuance_replacement"
down_revision = "hn_0014_issuance_registry"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      ALTER TABLE house_numbering.issuance_records
        ADD COLUMN replaces_issuance_id uuid,
        ADD COLUMN replacement_reason varchar(500),
        ADD CONSTRAINT fk_hn_issuance_predecessor FOREIGN KEY(organization_id,project_id,replaces_issuance_id)
          REFERENCES house_numbering.issuance_records(organization_id,project_id,id),
        ADD CONSTRAINT ck_hn_issuance_replacement CHECK(
          (replaces_issuance_id IS NULL AND replacement_reason IS NULL) OR
          (replaces_issuance_id IS NOT NULL AND replaces_issuance_id<>id AND replacement_reason IS NOT NULL AND length(btrim(replacement_reason))>0)
        );
      ALTER TABLE house_numbering.issuance_records DROP CONSTRAINT issuance_records_organization_id_project_id_number_id_key;
      CREATE UNIQUE INDEX uq_hn_issuance_root ON house_numbering.issuance_records(organization_id,project_id,number_id)
        WHERE replaces_issuance_id IS NULL;
      CREATE UNIQUE INDEX uq_hn_issuance_successor ON house_numbering.issuance_records(organization_id,project_id,replaces_issuance_id)
        WHERE replaces_issuance_id IS NOT NULL;
      CREATE FUNCTION house_numbering.check_issuance_replacement() RETURNS trigger
      LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
      DECLARE old_record house_numbering.issuance_records%ROWTYPE; old_state text;
      BEGIN
        IF NEW.replaces_issuance_id IS NULL THEN RETURN NEW; END IF;
        PERFORM pg_advisory_xact_lock(hashtextextended('hn_issuance:'||NEW.replaces_issuance_id::text,0));
        SELECT * INTO old_record FROM house_numbering.issuance_records
          WHERE organization_id=NEW.organization_id AND project_id=NEW.project_id AND id=NEW.replaces_issuance_id;
        SELECT state INTO old_state FROM house_numbering.issuance_events
          WHERE organization_id=NEW.organization_id AND project_id=NEW.project_id AND issuance_id=NEW.replaces_issuance_id
          ORDER BY revision DESC LIMIT 1;
        IF old_record.id IS NULL OR old_state IS DISTINCT FROM 'void'
          OR (NEW.number_id,NEW.building_id,NEW.route_id,NEW.proposal_id,NEW.number_text,NEW.route_name)
            IS DISTINCT FROM (old_record.number_id,old_record.building_id,old_record.route_id,old_record.proposal_id,old_record.number_text,old_record.route_name)
          OR NEW.effective_date<old_record.effective_date
        THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Replacement requires a void predecessor and unchanged approved address'; END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER trg_issuance_replacement BEFORE INSERT ON house_numbering.issuance_records
        FOR EACH ROW EXECUTE FUNCTION house_numbering.check_issuance_replacement();
    """)

def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.issuance_records WHERE replaces_issuance_id IS NOT NULL)
        THEN RAISE EXCEPTION 'Replacement history exists; retain before downgrade'; END IF; END $$;
      DROP TRIGGER trg_issuance_replacement ON house_numbering.issuance_records;
      DROP FUNCTION house_numbering.check_issuance_replacement();
      DROP INDEX house_numbering.uq_hn_issuance_root;
      DROP INDEX house_numbering.uq_hn_issuance_successor;
      ALTER TABLE house_numbering.issuance_records
        DROP COLUMN replaces_issuance_id, DROP COLUMN replacement_reason,
        ADD UNIQUE(organization_id,project_id,number_id);
    """)
