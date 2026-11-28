"""Separate parcel identity from buildings and observed parcel associations."""
from alembic import op
revision = "hn_0019_parcel_registry"
down_revision = "hn_0018_primary_entrance"
branch_labels = None
depends_on = None

def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.parcels(
        id uuid PRIMARY KEY,organization_id uuid NOT NULL,project_id uuid NOT NULL,
        source_id uuid NOT NULL,feature_id varchar(128) NOT NULL,
        reference varchar(200) NOT NULL CHECK(length(btrim(reference))>0),
        evidence_ref varchar(500) NOT NULL CHECK(length(btrim(evidence_ref))>0),
        created_by uuid NOT NULL,created_at timestamptz NOT NULL DEFAULT now(),
        UNIQUE(organization_id,project_id,id),UNIQUE(organization_id,project_id,source_id,feature_id),
        FOREIGN KEY(organization_id,project_id) REFERENCES workspace.projects(organization_id,id)
      );
      CREATE TABLE house_numbering.parcel_building_links(
        organization_id uuid NOT NULL,project_id uuid NOT NULL,parcel_id uuid NOT NULL,building_id uuid NOT NULL,
        revision bigint NOT NULL CHECK(revision>0),state text NOT NULL CHECK(state IN ('observed','confirmed','rejected','detached')),
        evidence_ref varchar(500) NOT NULL CHECK(length(btrim(evidence_ref))>0),
        geometry_digest text NOT NULL CHECK(geometry_digest ~ '^[a-f0-9]{64}$'),
        actor_id uuid NOT NULL,occurred_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,parcel_id,building_id,revision),
        FOREIGN KEY(organization_id,project_id,parcel_id) REFERENCES house_numbering.parcels(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,building_id) REFERENCES house_numbering.buildings(organization_id,project_id,id)
      );
      CREATE INDEX ix_hn_parcel_links_building ON house_numbering.parcel_building_links(organization_id,project_id,building_id,parcel_id,revision DESC);
      ALTER TABLE house_numbering.parcels ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.parcels FORCE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.parcel_building_links ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.parcel_building_links FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_parcel_scope ON house_numbering.parcels FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      CREATE POLICY hn_parcel_link_scope ON house_numbering.parcel_building_links FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      REVOKE ALL ON house_numbering.parcels,house_numbering.parcel_building_links FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT,INSERT ON house_numbering.parcels,house_numbering.parcel_building_links TO openvietgis_runtime;
      CREATE VIEW house_numbering.parcel_geometry_v1 WITH(security_invoker=true) AS
        SELECT p.*,CASE WHEN f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready'
          AND ST_GeometryType(f.geometry)='ST_Polygon' AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
          THEN encode(sha256(ST_AsEWKB(f.geometry)),'hex') END AS geometry_digest
        FROM house_numbering.parcels p
        LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(p.organization_id,p.project_id,p.source_id,p.feature_id)
        LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id);
      GRANT SELECT ON house_numbering.parcel_geometry_v1 TO openvietgis_runtime;
      CREATE FUNCTION house_numbering.check_parcel_link() RETURNS trigger
      LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
      DECLARE prior house_numbering.parcel_building_links%ROWTYPE; digest text;
      BEGIN
        PERFORM pg_advisory_xact_lock(hashtextextended('hn_parcel:'||NEW.parcel_id::text||':'||NEW.building_id::text,0));
        SELECT * INTO prior FROM house_numbering.parcel_building_links
          WHERE (organization_id,project_id,parcel_id,building_id)=(NEW.organization_id,NEW.project_id,NEW.parcel_id,NEW.building_id)
          ORDER BY revision DESC LIMIT 1;
        IF NEW.revision<>coalesce(prior.revision,0)+1 OR NOT coalesce(
          (prior.revision IS NULL AND NEW.state='observed') OR
          (prior.state IN ('rejected','detached') AND NEW.state='observed') OR
          (prior.state='observed' AND NEW.state IN ('confirmed','rejected') AND NEW.actor_id<>prior.actor_id) OR
          (prior.state='confirmed' AND NEW.state='detached'),false)
        THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Invalid parcel association transition'; END IF;
        IF NEW.state IN ('observed','confirmed') THEN
          SELECT p.geometry_digest INTO digest FROM house_numbering.parcel_geometry_v1 p
          JOIN house_numbering.buildings b ON (b.organization_id,b.project_id)=(p.organization_id,p.project_id)
          WHERE (p.organization_id,p.project_id,p.id,b.id)=(NEW.organization_id,NEW.project_id,NEW.parcel_id,NEW.building_id)
            AND b.state='active';
          IF digest IS NULL OR NEW.geometry_digest<>digest OR (NEW.state='confirmed' AND prior.geometry_digest<>digest)
          THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Parcel observation stale or geometry unavailable'; END IF;
        END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER trg_parcel_link BEFORE INSERT ON house_numbering.parcel_building_links
        FOR EACH ROW EXECUTE FUNCTION house_numbering.check_parcel_link();
    """)

def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.parcels) THEN
        RAISE EXCEPTION 'Parcel registry history exists; retain before downgrade'; END IF; END $$;
      DROP TABLE house_numbering.parcel_building_links;
      DROP FUNCTION house_numbering.check_parcel_link();
      DROP VIEW house_numbering.parcel_geometry_v1;
      DROP TABLE house_numbering.parcels;
    """)
