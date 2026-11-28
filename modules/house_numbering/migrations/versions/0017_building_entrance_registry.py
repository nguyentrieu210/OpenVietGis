"""Record observed entrances separately from the official building address."""
from alembic import op

revision = "hn_0017_entrance_registry"
down_revision = "hn_0016_collision_index"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.building_entrances(
        id uuid PRIMARY KEY, organization_id uuid NOT NULL, project_id uuid NOT NULL,
        building_id uuid NOT NULL, route_id uuid NOT NULL,
        source_id uuid NOT NULL, feature_id varchar(128) NOT NULL,
        evidence_ref varchar(500) NOT NULL CHECK(length(btrim(evidence_ref))>0),
        observed_number varchar(200),
        created_by uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
        UNIQUE(organization_id,project_id,id),
        UNIQUE(organization_id,project_id,building_id,source_id,feature_id,route_id),
        FOREIGN KEY(organization_id,project_id,building_id)
          REFERENCES house_numbering.buildings(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,route_id)
          REFERENCES house_numbering.routes(organization_id,project_id,id)
      );
      CREATE TABLE house_numbering.entrance_reviews(
        organization_id uuid NOT NULL,project_id uuid NOT NULL,entrance_id uuid NOT NULL,
        revision bigint NOT NULL CHECK(revision>0),
        state text NOT NULL CHECK(state IN ('observed','confirmed','rejected','retired')),
        reason varchar(500) NOT NULL CHECK(length(btrim(reason))>0),
        actor_id uuid NOT NULL,occurred_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,entrance_id,revision),
        FOREIGN KEY(organization_id,project_id,entrance_id)
          REFERENCES house_numbering.building_entrances(organization_id,project_id,id)
      );
      ALTER TABLE house_numbering.building_entrances ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.building_entrances FORCE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.entrance_reviews ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.entrance_reviews FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_entrance_scope ON house_numbering.building_entrances FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      CREATE POLICY hn_entrance_reviews_scope ON house_numbering.entrance_reviews FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      REVOKE ALL ON house_numbering.building_entrances,house_numbering.entrance_reviews FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT,INSERT ON house_numbering.building_entrances,house_numbering.entrance_reviews TO openvietgis_runtime;
      CREATE FUNCTION house_numbering.check_entrance_review() RETURNS trigger
      LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
      DECLARE prior text; rev bigint; creator uuid;
      BEGIN
        PERFORM pg_advisory_xact_lock(hashtextextended('hn_entrance:'||NEW.entrance_id::text,0));
        SELECT created_by INTO creator FROM house_numbering.building_entrances
          WHERE (organization_id,project_id,id)=(NEW.organization_id,NEW.project_id,NEW.entrance_id);
        SELECT state,revision INTO prior,rev FROM house_numbering.entrance_reviews
          WHERE (organization_id,project_id,entrance_id)=(NEW.organization_id,NEW.project_id,NEW.entrance_id)
          ORDER BY revision DESC LIMIT 1;
        IF NEW.revision<>coalesce(rev,0)+1 OR NOT coalesce(
          (prior IS NULL AND NEW.state='observed' AND NEW.actor_id=creator) OR
          (prior='observed' AND NEW.state IN ('confirmed','rejected') AND NEW.actor_id<>creator) OR
          (prior='confirmed' AND NEW.state='retired'),false)
        THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Invalid entrance review or independent reviewer required'; END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER trg_entrance_review BEFORE INSERT ON house_numbering.entrance_reviews
        FOR EACH ROW EXECUTE FUNCTION house_numbering.check_entrance_review();
    """)


def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.building_entrances)
        THEN RAISE EXCEPTION 'Entrance history exists; retain before downgrade'; END IF; END $$;
      DROP TABLE house_numbering.entrance_reviews;
      DROP FUNCTION house_numbering.check_entrance_review();
      DROP TABLE house_numbering.building_entrances;
    """)
