-- Run after receipt_fixture.sql and exact migrations 0052/0053 on a disposable DB.
\set ON_ERROR_STOP on
INSERT INTO extensions.manifests VALUES ('11000000-0000-0000-0000-000000000001','21000000-0000-0000-0000-000000000001');
INSERT INTO extensions.project_activations VALUES ('11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000001','41000000-0000-0000-0000-000000000001');
SET ROLE openvietgis_runtime;
BEGIN;
SELECT set_config('openvietgis.organization_id','11000000-0000-0000-0000-000000000001',true),
       set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000001',true),
       set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true);
INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled)
VALUES ('11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000001',
'41000000-0000-0000-0000-000000000001',1,'21000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001',repeat('a',64),true);
INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled)
SELECT organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,false FROM extensions.plugin_v2_grant_events;
DO $$ BEGIN
 IF (SELECT count(*) FROM extensions.plugin_v2_grant_events)<>2 THEN RAISE EXCEPTION 'grant audit missing'; END IF;
 IF (SELECT enabled FROM extensions.plugin_v2_grant_events ORDER BY sequence DESC LIMIT 1) THEN RAISE EXCEPTION 'revocation missing'; END IF;
 BEGIN
  UPDATE extensions.plugin_v2_grant_events SET enabled=true;
  RAISE EXCEPTION 'update allowed';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
 BEGIN
  DELETE FROM extensions.plugin_v2_grant_events;
  RAISE EXCEPTION 'delete allowed';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
 BEGIN
  INSERT INTO extensions.plugin_v2_grant_events (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled)
  SELECT organization_id,project_id,activation_id,activation_revision,manifest_id,'51000000-0000-0000-0000-000000000002',release_digest,true FROM extensions.plugin_v2_grant_events LIMIT 1;
  RAISE EXCEPTION 'forged actor allowed';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
END $$;
SELECT set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000002',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_grant_events) THEN RAISE EXCEPTION 'cross project leak'; END IF; END $$;
SELECT set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000001',true),set_config('openvietgis.organization_id','11000000-0000-0000-0000-000000000002',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_grant_events) THEN RAISE EXCEPTION 'cross organization leak'; END IF; END $$;
SELECT set_config('openvietgis.organization_id','11000000-0000-0000-0000-000000000001',true),set_config('test.gis_scoped','true',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_grant_events) THEN RAISE EXCEPTION 'restricted GIS leak'; END IF; END $$;
COMMIT;
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_grant_events) THEN RAISE EXCEPTION 'missing scope leak'; END IF; END $$;
RESET ROLE;
SELECT 'SPEC075_GRANT_RLS_PASS' AS result;
