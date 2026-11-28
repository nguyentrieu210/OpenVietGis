"""Record external retirement decisions and explicit successor address links."""
from alembic import op
revision = "hn_0021_address_changes"
down_revision = "hn_0020_reconcile_lookup"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.address_retirements(
        organization_id uuid NOT NULL,project_id uuid NOT NULL,number_id uuid NOT NULL,
        effective_date date NOT NULL,decision_ref varchar(200) NOT NULL CHECK(length(btrim(decision_ref))>0),
        reason varchar(500) NOT NULL CHECK(length(btrim(reason))>0),actor_id uuid NOT NULL,created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,number_id),
        FOREIGN KEY(organization_id,project_id,number_id) REFERENCES house_numbering.address_numbers(organization_id,project_id,id)
      );
      CREATE TABLE house_numbering.address_successors(
        organization_id uuid NOT NULL,project_id uuid NOT NULL,old_number_id uuid NOT NULL,new_number_id uuid NOT NULL,
        effective_date date NOT NULL,decision_ref varchar(200) NOT NULL CHECK(length(btrim(decision_ref))>0),
        actor_id uuid NOT NULL,created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,old_number_id),
        UNIQUE(organization_id,project_id,new_number_id),
        CHECK(old_number_id<>new_number_id),
        FOREIGN KEY(organization_id,project_id,old_number_id) REFERENCES house_numbering.address_retirements(organization_id,project_id,number_id),
        FOREIGN KEY(organization_id,project_id,new_number_id) REFERENCES house_numbering.address_numbers(organization_id,project_id,id)
      );
      ALTER TABLE house_numbering.address_retirements ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.address_retirements FORCE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.address_successors ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.address_successors FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_retire_scope ON house_numbering.address_retirements FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      CREATE POLICY hn_successor_scope ON house_numbering.address_successors FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      REVOKE ALL ON house_numbering.address_retirements,house_numbering.address_successors FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT,INSERT ON house_numbering.address_retirements,house_numbering.address_successors TO openvietgis_runtime;
      CREATE FUNCTION house_numbering.check_address_successor() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
      DECLARE valid boolean;
      BEGIN
        SELECT o.kind='approved' AND o.retired_at IS NOT NULL AND n.kind='approved' AND n.retired_at IS NULL
          AND o.building_id IS NOT NULL AND o.building_id=n.building_id
          AND n.created_at>=o.retired_at AND NEW.effective_date>=r.effective_date
          INTO valid
        FROM house_numbering.address_numbers o JOIN house_numbering.address_retirements r
          ON (r.organization_id,r.project_id,r.number_id)=(o.organization_id,o.project_id,o.id)
        JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(NEW.organization_id,NEW.project_id,NEW.new_number_id)
        WHERE (o.organization_id,o.project_id,o.id)=(NEW.organization_id,NEW.project_id,NEW.old_number_id);
        IF NOT coalesce(valid,false) THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Successor requires a new active approved number on the same building'; END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER trg_address_successor BEFORE INSERT ON house_numbering.address_successors
      FOR EACH ROW EXECUTE FUNCTION house_numbering.check_address_successor();
    """)

def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.address_retirements) THEN
        RAISE EXCEPTION 'Address change history exists; retain before downgrade'; END IF; END $$;
      DROP TABLE house_numbering.address_successors;
      DROP FUNCTION house_numbering.check_address_successor();
      DROP TABLE house_numbering.address_retirements;
    """)
