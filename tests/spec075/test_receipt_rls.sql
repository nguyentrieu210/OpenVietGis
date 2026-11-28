-- Run against isolated disposable database only. Prerequisite minimal fixture
-- represents the canonical composite FK and GIS ACL contracts, not full schema.
\set ON_ERROR_STOP on
INSERT INTO extensions.manifests VALUES ('10000000-0000-0000-0000-000000000001','20000000-0000-0000-0000-000000000001');
INSERT INTO extensions.project_activations VALUES ('10000000-0000-0000-0000-000000000001','30000000-0000-0000-0000-000000000001','40000000-0000-0000-0000-000000000001');
SET ROLE openvietgis_runtime;
BEGIN;
SELECT set_config('openvietgis.organization_id','10000000-0000-0000-0000-000000000001',true),
       set_config('openvietgis.project_id','30000000-0000-0000-0000-000000000001',true),
       set_config('openvietgis.actor_user_id','50000000-0000-0000-0000-000000000001',true);
INSERT INTO extensions.plugin_v2_receipts (id,organization_id,project_id,actor_id,request_id,activation_id,activation_revision,manifest_id,release_digest,request_digest,command_id)
VALUES ('60000000-0000-0000-0000-000000000001','10000000-0000-0000-0000-000000000001','30000000-0000-0000-0000-000000000001',
'50000000-0000-0000-0000-000000000001','70000000-0000-0000-0000-000000000001','40000000-0000-0000-0000-000000000001',1,
'20000000-0000-0000-0000-000000000001',repeat('a',64),repeat('b',64),'host_context');
DO $$
BEGIN
 IF (SELECT count(*) FROM extensions.plugin_v2_receipts)<>1 THEN RAISE EXCEPTION 'owner read denied'; END IF;
 BEGIN
  INSERT INTO extensions.plugin_v2_receipts SELECT '60000000-0000-0000-0000-000000000002',organization_id,project_id,actor_id,request_id,activation_id,activation_revision,manifest_id,release_digest,request_digest,command_id,created_at FROM extensions.plugin_v2_receipts;
  RAISE EXCEPTION 'duplicate accepted';
 EXCEPTION WHEN unique_violation THEN NULL;
 END;
 BEGIN
  UPDATE extensions.plugin_v2_receipts SET command_id='tampered';
  RAISE EXCEPTION 'update accepted';
 EXCEPTION WHEN insufficient_privilege THEN NULL;
 END;
 BEGIN
  DELETE FROM extensions.plugin_v2_receipts;
  RAISE EXCEPTION 'delete accepted';
 EXCEPTION WHEN insufficient_privilege THEN NULL;
 END;
END $$;
SELECT set_config('openvietgis.actor_user_id','50000000-0000-0000-0000-000000000002',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_receipts) THEN RAISE EXCEPTION 'cross actor leak'; END IF; END $$;
SELECT set_config('openvietgis.actor_user_id','50000000-0000-0000-0000-000000000001',true),set_config('openvietgis.project_id','30000000-0000-0000-0000-000000000002',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_receipts) THEN RAISE EXCEPTION 'cross project leak'; END IF; END $$;
SELECT set_config('openvietgis.organization_id','10000000-0000-0000-0000-000000000002',true),set_config('openvietgis.project_id','30000000-0000-0000-0000-000000000001',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_receipts) THEN RAISE EXCEPTION 'cross organization leak'; END IF; END $$;
SELECT set_config('openvietgis.organization_id','10000000-0000-0000-0000-000000000001',true);
SELECT set_config('test.gis_scoped','true',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_receipts) THEN RAISE EXCEPTION 'GIS-scoped actor leak'; END IF; END $$;
SELECT set_config('test.gis_scoped','false',true);
DO $$ BEGIN
 BEGIN
 INSERT INTO extensions.plugin_v2_receipts (id,organization_id,project_id,actor_id,request_id,activation_id,activation_revision,manifest_id,release_digest,request_digest,command_id)
 VALUES ('60000000-0000-0000-0000-000000000003','10000000-0000-0000-0000-000000000001','30000000-0000-0000-0000-000000000001',
 '50000000-0000-0000-0000-000000000002','70000000-0000-0000-0000-000000000003','40000000-0000-0000-0000-000000000001',1,
 '20000000-0000-0000-0000-000000000001',repeat('a',64),repeat('b',64),'host_context');
 RAISE EXCEPTION 'cross actor insert accepted';
 EXCEPTION WHEN insufficient_privilege THEN NULL;
 END;
END $$;
COMMIT;
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_v2_receipts) THEN RAISE EXCEPTION 'missing scope leak'; END IF; END $$;
RESET ROLE;
SELECT 'SPEC075_RECEIPT_RLS_PASS' AS result;
