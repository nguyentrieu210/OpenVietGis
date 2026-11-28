-- Run on the same disposable fixture after 0052, 0053, 0054 and grant tests.
\set ON_ERROR_STOP on
SET ROLE openvietgis_runtime;
BEGIN;
SELECT set_config('openvietgis.organization_id','11000000-0000-0000-0000-000000000001',true),
       set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000001',true),
       set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true);
DO $$ BEGIN
 IF (SELECT count(*) FROM extensions.plugin_v2_grant_events WHERE dependency_lock IS NULL)<>2 THEN RAISE EXCEPTION 'legacy grant history lost'; END IF;
END $$;
INSERT INTO extensions.plugin_v2_grant_events
  (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled,dependency_lock,lock_digest,catalog_epoch)
VALUES ('11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000001',
'41000000-0000-0000-0000-000000000001',1,'21000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001',
repeat('a',64),true,'[{"key":"partner_fixture_context","version":"1.0.0"}]',repeat('b',64),4);
DO $$ BEGIN
 IF (SELECT max(catalog_epoch) FROM extensions.plugin_v2_grant_events)<>4 THEN RAISE EXCEPTION 'epoch missing'; END IF;
 IF (SELECT lock_digest FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1)<>repeat('b',64) THEN RAISE EXCEPTION 'lock missing'; END IF;
 BEGIN
  INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled,dependency_lock,lock_digest,catalog_epoch)
  SELECT organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,true,'[]',lock_digest,catalog_epoch FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1;
  RAISE EXCEPTION 'empty lock accepted';
 EXCEPTION WHEN check_violation THEN NULL; END;
 BEGIN
  INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled,dependency_lock,lock_digest,catalog_epoch)
  SELECT organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,true,dependency_lock,NULL,catalog_epoch FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1;
  RAISE EXCEPTION 'partial lock accepted';
 EXCEPTION WHEN check_violation THEN NULL; END;
 BEGIN
  INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled,dependency_lock,lock_digest,catalog_epoch)
  SELECT organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,true,dependency_lock,lock_digest,0 FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1;
  RAISE EXCEPTION 'zero epoch accepted';
 EXCEPTION WHEN check_violation THEN NULL; END;
END $$;
INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled)
SELECT organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,false FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1;
DO $$ BEGIN
 IF (SELECT enabled FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1) THEN RAISE EXCEPTION 'revocation lost'; END IF;
 IF (SELECT max(catalog_epoch) FROM extensions.plugin_v2_grant_events)<>4 THEN RAISE EXCEPTION 'historical epoch floor lost'; END IF;
END $$;
COMMIT;
RESET ROLE;
SELECT 'SPEC075_DEPENDENCY_LOCK_PASS' AS result;
