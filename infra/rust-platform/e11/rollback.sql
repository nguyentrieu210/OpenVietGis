-- Candidate staging rollback; removes only the E11 candidate objects.
BEGIN;
DROP TRIGGER e11_durable_spatial_change ON editing.changesets;
DROP FUNCTION ovg_e11.capture_spatial();
DROP FUNCTION ovg_e11.append_spatial(uuid, uuid);
DROP TABLE ovg_e11.events;
DROP TABLE ovg_e11.streams;
DROP SCHEMA ovg_e11;
COMMIT;
