-- Disposable receipt fixture + exact 0057 DDL only.
\set ON_ERROR_STOP on
INSERT INTO extensions.manifests VALUES ('11000000-0000-0000-0000-000000000001','21000000-0000-0000-0000-000000000001');
INSERT INTO extensions.project_activations VALUES ('11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000001','41000000-0000-0000-0000-000000000001');
SET ROLE openvietgis_runtime;
BEGIN;
SELECT set_config('openvietgis.organization_id','11000000-0000-0000-0000-000000000001',true),set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000001',true),set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true);
INSERT INTO extensions.plugin_processing_proposals(id,organization_id,project_id,actor_id,manifest_id,activation_id,activation_revision,release_digest,lock_digest,grant_revision,catalog_epoch,plan,plan_digest)
VALUES ('71000000-0000-0000-0000-000000000001','11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001','21000000-0000-0000-0000-000000000001','41000000-0000-0000-0000-000000000001',1,repeat('a',64),repeat('b',64),1,2,'{"request":"synthetic"}',repeat('c',64));
INSERT INTO extensions.plugin_processing_approvals(organization_id,project_id,proposal_id,actor_id,job_id)
SELECT organization_id,project_id,id,actor_id,'81000000-0000-0000-0000-000000000001' FROM extensions.plugin_processing_proposals;
DO $$ BEGIN
 IF (SELECT count(*) FROM extensions.plugin_processing_approvals)<>1 THEN RAISE EXCEPTION 'missing approval'; END IF;
 BEGIN
 INSERT INTO extensions.plugin_processing_approvals(organization_id,project_id,proposal_id,actor_id,job_id)
 SELECT organization_id,project_id,id,actor_id,'81000000-0000-0000-0000-000000000002' FROM extensions.plugin_processing_proposals;
 RAISE EXCEPTION 'duplicate approval allowed';
 EXCEPTION WHEN unique_violation THEN NULL; END;
 BEGIN
 UPDATE extensions.plugin_processing_proposals SET plan='{}';
 RAISE EXCEPTION 'mutable proposal allowed';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
END $$;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000002',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_processing_proposals) OR EXISTS(SELECT 1 FROM extensions.plugin_processing_approvals) THEN RAISE EXCEPTION 'actor leak'; END IF; END $$;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true),set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000002',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_processing_proposals) THEN RAISE EXCEPTION 'project leak'; END IF; END $$;
SELECT set_config('openvietgis.project_id','31000000-0000-0000-0000-000000000001',true),set_config('test.gis_scoped','true',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.plugin_processing_proposals) THEN RAISE EXCEPTION 'scoped GIS leak'; END IF; END $$;
COMMIT;
RESET ROLE;
SELECT 'SPEC075_PROCESSING_PROPOSAL_RLS_PASS' AS result;
