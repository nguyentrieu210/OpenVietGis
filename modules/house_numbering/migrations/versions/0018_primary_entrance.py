"""Append-only choice of primary access, without changing an allocated address."""
from alembic import op

revision = "hn_0018_primary_entrance"
down_revision = "hn_0017_entrance_registry"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE house_numbering.primary_entrance_choices(
        organization_id uuid NOT NULL, project_id uuid NOT NULL, building_id uuid NOT NULL,
        revision bigint NOT NULL CHECK(revision>0), entrance_id uuid NOT NULL,
        evidence_digest char(64) NOT NULL CHECK(evidence_digest ~ '^[a-f0-9]{64}$'),
        reason varchar(500) NOT NULL CHECK(length(btrim(reason))>0),
        actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY(organization_id,project_id,building_id,revision),
        FOREIGN KEY(organization_id,project_id,building_id)
          REFERENCES house_numbering.buildings(organization_id,project_id,id),
        FOREIGN KEY(organization_id,project_id,entrance_id)
          REFERENCES house_numbering.building_entrances(organization_id,project_id,id)
      );
      ALTER TABLE house_numbering.primary_entrance_choices ENABLE ROW LEVEL SECURITY;
      ALTER TABLE house_numbering.primary_entrance_choices FORCE ROW LEVEL SECURITY;
      CREATE POLICY hn_primary_choice_scope ON house_numbering.primary_entrance_choices FOR ALL TO openvietgis_runtime
        USING(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid)
        WITH CHECK(organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
      REVOKE ALL ON house_numbering.primary_entrance_choices FROM PUBLIC,openvietgis_runtime;
      GRANT SELECT,INSERT ON house_numbering.primary_entrance_choices TO openvietgis_runtime;

      CREATE VIEW house_numbering.entrance_context_v1 WITH(security_invoker=true) AS
      SELECT e.organization_id,e.project_id,e.building_id,e.id,e.route_id,e.source_id,e.feature_id,
        r.state,r.revision,route.status AS route_status,route.revision AS route_revision,
        f.content_revision,encode(sha256(ST_AsEWKB(f.geometry)),'hex') AS geometry_digest,
        coalesce(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready'
          AND ST_GeometryType(f.geometry)='ST_Point' AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry),false) AS ready
      FROM house_numbering.building_entrances e
      LEFT JOIN LATERAL (SELECT state,revision FROM house_numbering.entrance_reviews r
        WHERE (r.organization_id,r.project_id,r.entrance_id)=(e.organization_id,e.project_id,e.id)
        ORDER BY revision DESC LIMIT 1) r ON true
      JOIN house_numbering.routes route ON (route.organization_id,route.project_id,route.id)=(e.organization_id,e.project_id,e.route_id)
      LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(e.organization_id,e.project_id,e.source_id,e.feature_id)
      LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id);

      CREATE VIEW house_numbering.entrance_decisions_v1 WITH(security_invoker=true) AS
      SELECT x.*,p.entrance_id AS selected_entrance_id,coalesce(p.revision,0) AS choice_revision,
        coalesce(p.evidence_digest=x.evidence_digest AND chosen.state='confirmed' AND chosen.ready
          AND chosen.route_status='confirmed' AND chosen.route_id=b.route_id,false) AS choice_valid
      FROM (
        SELECT organization_id,project_id,building_id,
          encode(sha256(convert_to(jsonb_agg(jsonb_build_array(id,route_id,source_id,feature_id,state,revision,
            route_status,route_revision,content_revision,geometry_digest,ready) ORDER BY id)::text,'UTF8')),'hex') AS evidence_digest,
          bool_or(state IS NULL OR state='observed') AS pending,
          bool_or(state='confirmed' AND (NOT ready OR route_status<>'confirmed')) AS invalid_confirmed,
          count(*) FILTER(WHERE state='confirmed') AS confirmed_count,
          count(DISTINCT route_id) FILTER(WHERE state='confirmed') AS confirmed_routes
        FROM house_numbering.entrance_context_v1 GROUP BY organization_id,project_id,building_id
      ) x
      JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(x.organization_id,x.project_id,x.building_id)
      LEFT JOIN LATERAL (SELECT entrance_id,revision,evidence_digest FROM house_numbering.primary_entrance_choices p
        WHERE (p.organization_id,p.project_id,p.building_id)=(x.organization_id,x.project_id,x.building_id)
        ORDER BY revision DESC LIMIT 1) p ON true
      LEFT JOIN house_numbering.entrance_context_v1 chosen
        ON (chosen.organization_id,chosen.project_id,chosen.building_id,chosen.id)=(x.organization_id,x.project_id,x.building_id,p.entrance_id);
      GRANT SELECT ON house_numbering.entrance_context_v1,house_numbering.entrance_decisions_v1 TO openvietgis_runtime;

      CREATE FUNCTION house_numbering.check_primary_choice() RETURNS trigger
      LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
      DECLARE d house_numbering.entrance_decisions_v1%ROWTYPE;
      BEGIN
        PERFORM pg_advisory_xact_lock(hashtextextended('hn_primary:'||NEW.building_id::text,0));
        SELECT * INTO d FROM house_numbering.entrance_decisions_v1
          WHERE (organization_id,project_id,building_id)=(NEW.organization_id,NEW.project_id,NEW.building_id);
        IF d.building_id IS NULL OR NEW.revision<>d.choice_revision+1 OR NEW.evidence_digest<>d.evidence_digest
          OR d.pending OR coalesce(d.invalid_confirmed,false)
          OR NOT EXISTS(SELECT 1 FROM house_numbering.entrance_context_v1 e
            JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(e.organization_id,e.project_id,e.building_id)
            JOIN house_numbering.building_entrances o ON (o.organization_id,o.project_id,o.id)=(e.organization_id,e.project_id,e.id)
            WHERE (e.organization_id,e.project_id,e.building_id,e.id)=(NEW.organization_id,NEW.project_id,NEW.building_id,NEW.entrance_id)
              AND e.state='confirmed' AND e.ready AND e.route_status='confirmed' AND e.route_id=b.route_id
              AND b.state='active' AND o.created_by<>NEW.actor_id)
          OR EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE
            (n.organization_id,n.project_id,n.building_id)=(NEW.organization_id,NEW.project_id,NEW.building_id)
              AND n.retired_at IS NULL AND n.kind IN ('submitted','approved'))
        THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE='Primary entrance requires current reviewed evidence and no submitted or approved number'; END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER trg_primary_choice BEFORE INSERT ON house_numbering.primary_entrance_choices
        FOR EACH ROW EXECUTE FUNCTION house_numbering.check_primary_choice();
    """)


def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS(SELECT 1 FROM house_numbering.primary_entrance_choices)
        THEN RAISE EXCEPTION 'Primary entrance history exists; retain before downgrade'; END IF; END $$;
      DROP TRIGGER trg_primary_choice ON house_numbering.primary_entrance_choices;
      DROP FUNCTION house_numbering.check_primary_choice();
      DROP VIEW house_numbering.entrance_decisions_v1;
      DROP VIEW house_numbering.entrance_context_v1;
      DROP TABLE house_numbering.primary_entrance_choices;
    """)
