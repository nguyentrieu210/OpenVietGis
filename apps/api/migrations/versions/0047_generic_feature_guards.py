"""Core-owned declarative feature guards. Official modules register policies, not triggers on GIS tables."""
from alembic import op

revision = "0047_generic_feature_guards"
down_revision = "0046_map_display_config"
branch_labels = None
depends_on = None


def upgrade():
    op.execute("""
      CREATE TABLE extensions.feature_guard_providers (
        extension_key varchar(100) PRIMARY KEY,
        lock_namespace varchar(100) NOT NULL UNIQUE CHECK (length(btrim(lock_namespace)) > 0),
        required_fields text[] NOT NULL DEFAULT '{}'::text[]
      );
      CREATE TABLE extensions.feature_guard_locks (
        extension_key varchar(100) NOT NULL REFERENCES extensions.feature_guard_providers(extension_key) ON DELETE RESTRICT,
        lock_id uuid NOT NULL, organization_id uuid NOT NULL, project_id uuid NOT NULL,
        source_id uuid NOT NULL, scope_values jsonb NOT NULL,
        message varchar(1000) NOT NULL,
        PRIMARY KEY (extension_key, lock_id),
        CHECK(jsonb_typeof(scope_values)='object' AND scope_values <> '{}'::jsonb),
        CHECK(length(btrim(message))>0)
      );
      CREATE INDEX ix_feature_guard_scope ON extensions.feature_guard_locks
        (organization_id,project_id,source_id,extension_key);
      -- These tables are a trusted internal adapter boundary, not a public write API.
      REVOKE ALL ON extensions.feature_guard_providers,extensions.feature_guard_locks FROM PUBLIC,openvietgis_runtime;
      CREATE FUNCTION extensions.guard_feature_write() RETURNS trigger
      LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
      DECLARE r record; provider record; blocked_message text;
      BEGIN
        -- Schema expansion with null fields and revision bumps are not business edits.
        IF TG_OP='UPDATE' AND OLD.organization_id=NEW.organization_id AND OLD.project_id=NEW.project_id
          AND OLD.source_id=NEW.source_id AND OLD.feature_id=NEW.feature_id
          AND OLD.geometry_state=NEW.geometry_state
          AND public.ST_AsEWKB(OLD.geometry) IS NOT DISTINCT FROM public.ST_AsEWKB(NEW.geometry)
          AND NEW.attributes @> OLD.attributes
          AND NOT EXISTS (SELECT 1 FROM jsonb_each(NEW.attributes) e WHERE NOT OLD.attributes ? e.key AND e.value<>'null'::jsonb)
        THEN RETURN NEW; END IF;
        FOR r IN SELECT OLD AS f WHERE TG_OP<>'INSERT' UNION ALL SELECT NEW WHERE TG_OP<>'DELETE' LOOP
          FOR provider IN SELECT p.extension_key,p.lock_namespace FROM extensions.feature_guard_providers p
            WHERE (r.f).attributes ?& p.required_fields
              AND EXISTS (SELECT 1 FROM extensions.project_activations a WHERE a.organization_id=(r.f).organization_id
                AND a.project_id=(r.f).project_id AND a.extension_key=p.extension_key AND a.status='active')
            ORDER BY p.lock_namespace LOOP
            -- The matching provider takes this key exclusively before signing/counting a source.
            -- Keep the existing namespace stable across deployments so older trusted writers serialize.
            PERFORM pg_advisory_xact_lock_shared(hashtextextended(provider.lock_namespace||(r.f).source_id::text,0));
            SELECT l.message INTO blocked_message FROM extensions.feature_guard_locks l
              WHERE l.organization_id=(r.f).organization_id AND l.project_id=(r.f).project_id
                AND l.source_id=(r.f).source_id AND l.extension_key=provider.extension_key
                AND NOT EXISTS (SELECT 1 FROM jsonb_each_text(l.scope_values) s
                  WHERE btrim((r.f).attributes->>s.key) IS DISTINCT FROM s.value)
              ORDER BY l.lock_id LIMIT 1;
            IF FOUND THEN RAISE EXCEPTION USING ERRCODE='OV409',MESSAGE=blocked_message; END IF;
          END LOOP;
        END LOOP;
        RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
      END $$;
      REVOKE ALL ON FUNCTION extensions.guard_feature_write() FROM PUBLIC;
      CREATE TRIGGER trg_core_feature_guard_insert BEFORE INSERT ON spatial.reference_features
        FOR EACH ROW EXECUTE FUNCTION extensions.guard_feature_write();
      CREATE TRIGGER trg_core_feature_guard_update BEFORE UPDATE OF attributes,geometry,geometry_state,organization_id,project_id,source_id,feature_id ON spatial.reference_features
        FOR EACH ROW WHEN (OLD.attributes IS DISTINCT FROM NEW.attributes OR public.ST_AsEWKB(OLD.geometry) IS DISTINCT FROM public.ST_AsEWKB(NEW.geometry)
          OR OLD.geometry_state IS DISTINCT FROM NEW.geometry_state OR OLD.organization_id IS DISTINCT FROM NEW.organization_id
          OR OLD.project_id IS DISTINCT FROM NEW.project_id OR OLD.source_id IS DISTINCT FROM NEW.source_id OR OLD.feature_id IS DISTINCT FROM NEW.feature_id)
        EXECUTE FUNCTION extensions.guard_feature_write();
      CREATE TRIGGER trg_core_feature_guard_delete BEFORE DELETE ON spatial.reference_features
        FOR EACH ROW EXECUTE FUNCTION extensions.guard_feature_write();
    """)


def downgrade():
    op.execute("""
      DO $$ BEGIN IF EXISTS (SELECT 1 FROM extensions.feature_guard_providers) THEN
        RAISE EXCEPTION 'Downgrade registered feature guard modules before Core'; END IF; END $$;
      DROP TRIGGER trg_core_feature_guard_delete ON spatial.reference_features;
      DROP TRIGGER trg_core_feature_guard_update ON spatial.reference_features;
      DROP TRIGGER trg_core_feature_guard_insert ON spatial.reference_features;
      DROP FUNCTION extensions.guard_feature_write();
      DROP TABLE extensions.feature_guard_locks;
      DROP TABLE extensions.feature_guard_providers;
    """)
