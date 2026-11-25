-- Staging candidate only; promote through a reviewed Alembic revision.
-- Project-wide spatial invalidations only: never store private chat or feature payloads.
BEGIN;
CREATE SCHEMA ovg_e11;
REVOKE ALL ON SCHEMA ovg_e11 FROM PUBLIC;
CREATE TABLE ovg_e11.streams (
    organization_id uuid NOT NULL,
    project_id uuid NOT NULL,
    epoch uuid NOT NULL DEFAULT gen_random_uuid(),
    sequence bigint NOT NULL CHECK (sequence >= 0),
    PRIMARY KEY (organization_id, project_id)
);
CREATE TABLE ovg_e11.events (
    organization_id uuid NOT NULL,
    project_id uuid NOT NULL,
    sequence bigint NOT NULL CHECK (sequence > 0),
    kind text NOT NULL CHECK (kind = 'spatial_change'),
    PRIMARY KEY (organization_id, project_id, sequence),
    FOREIGN KEY (organization_id, project_id)
      REFERENCES ovg_e11.streams(organization_id, project_id) ON DELETE CASCADE
);
ALTER TABLE ovg_e11.streams ENABLE ROW LEVEL SECURITY;
ALTER TABLE ovg_e11.events ENABLE ROW LEVEL SECURITY;
-- Owner writes through a fixed-search-path trigger; runtime only reads.
CREATE POLICY stream_read ON ovg_e11.streams FOR SELECT TO openvietgis_runtime
USING (
    organization_id = NULLIF(current_setting('openvietgis.organization_id', true), '')::uuid
    AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
    AND NULLIF(current_setting('openvietgis.actor_user_id', true), '')::uuid IS NOT NULL
    AND NOT gis_access.scoped(organization_id, project_id)
);
CREATE POLICY event_read ON ovg_e11.events FOR SELECT TO openvietgis_runtime
USING (
    organization_id = NULLIF(current_setting('openvietgis.organization_id', true), '')::uuid
    AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
    AND NULLIF(current_setting('openvietgis.actor_user_id', true), '')::uuid IS NOT NULL
    AND NOT gis_access.scoped(organization_id, project_id)
);
GRANT USAGE ON SCHEMA ovg_e11 TO openvietgis_runtime;
GRANT SELECT ON ovg_e11.streams, ovg_e11.events TO openvietgis_runtime;

CREATE FUNCTION ovg_e11.append_spatial(o uuid, p uuid) RETURNS bigint
LANGUAGE plpgsql SET search_path = pg_catalog, pg_temp AS $$
DECLARE n bigint;
BEGIN
    -- The row lock serializes writes within a project until commit. Unlike a
    -- global sequence, a later committed cursor cannot skip an earlier open tx.
    INSERT INTO ovg_e11.streams AS s (organization_id, project_id, sequence)
    VALUES (o, p, 1)
    ON CONFLICT (organization_id, project_id)
    DO UPDATE SET sequence = s.sequence + 1
    RETURNING sequence INTO n;

    INSERT INTO ovg_e11.events VALUES (o, p, n, 'spatial_change');
    DELETE FROM ovg_e11.events
    WHERE organization_id = o AND project_id = p AND sequence <= n - 4096;
    RETURN n;
END $$;
REVOKE ALL ON FUNCTION ovg_e11.append_spatial(uuid, uuid) FROM PUBLIC;

CREATE FUNCTION ovg_e11.capture_spatial() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF TG_TABLE_SCHEMA <> 'editing' OR TG_TABLE_NAME <> 'changesets' THEN
        RAISE EXCEPTION 'unexpected outbox trigger source';
    END IF;
    IF TG_OP = 'UPDATE' THEN
        IF OLD.status IS NOT DISTINCT FROM NEW.status THEN
            RETURN NEW;
        END IF;
    END IF;
    PERFORM ovg_e11.append_spatial(NEW.organization_id, NEW.project_id);
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION ovg_e11.capture_spatial() FROM PUBLIC;
CREATE TRIGGER e11_durable_spatial_change
AFTER INSERT OR UPDATE OF status ON editing.changesets
FOR EACH ROW EXECUTE FUNCTION ovg_e11.capture_spatial();
COMMIT;
