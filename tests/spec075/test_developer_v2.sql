\set ON_ERROR_STOP on
SET ROLE openvietgis_runtime;
BEGIN;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true),set_config('openvietgis.marketplace_reviewer','no',true);
INSERT INTO extensions.marketplace_publishers(id,owner_user_id,name,public_key)
VALUES ('dev_aaaaaaaaaaaaaaaaaaaaaaaa','51000000-0000-0000-0000-000000000001','Synthetic',repeat('a',44));
INSERT INTO extensions.marketplace_submissions(id,publisher_id,owner_user_id,extension_key,version,manifest,signature)
VALUES ('71000000-0000-0000-0000-000000000001','dev_aaaaaaaaaaaaaaaaaaaaaaaa','51000000-0000-0000-0000-000000000001','partner_dev_aaaaaaaaaaaaaaaaaaaaaaaa_test','1.0.0','{}',repeat('b',88));
INSERT INTO extensions.marketplace_v2_submissions(submission_id,owner_user_id,bundle,release_digest)
VALUES ('71000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001','{"synthetic":true}',repeat('c',64));
DO $$ BEGIN
 BEGIN
 UPDATE extensions.marketplace_v2_submissions SET bundle='{}';
 RAISE EXCEPTION 'mutable signed bundle';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
END $$;
SELECT set_config('openvietgis.marketplace_reviewer','yes',true);
DO $$ BEGIN
 BEGIN
 UPDATE extensions.marketplace_v2_submissions SET status='approved',review_note='self',reviewed_by='51000000-0000-0000-0000-000000000001',reviewed_at=now();
 IF FOUND THEN RAISE EXCEPTION 'self review allowed'; END IF;
 EXCEPTION WHEN insufficient_privilege OR check_violation THEN NULL; END;
END $$;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000002',true),set_config('openvietgis.marketplace_reviewer','no',true);
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM extensions.marketplace_v2_submissions) THEN RAISE EXCEPTION 'cross-owner leak'; END IF;
 BEGIN
 INSERT INTO extensions.marketplace_v2_submissions(submission_id,owner_user_id,bundle,release_digest)
 VALUES ('71000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000002','{}',repeat('d',64));
 RAISE EXCEPTION 'cross-owner insert allowed';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
END $$;
SELECT set_config('openvietgis.marketplace_reviewer','yes',true);
UPDATE extensions.marketplace_submissions SET status='approved',review_note='manifest ok',reviewed_by='51000000-0000-0000-0000-000000000002',reviewed_at=now();
UPDATE extensions.marketplace_v2_submissions SET status='approved',review_note='bundle ok',reviewed_by='51000000-0000-0000-0000-000000000002',reviewed_at=now();
DO $$ BEGIN
 IF (SELECT status FROM extensions.marketplace_v2_submissions)<>'approved' THEN RAISE EXCEPTION 'review not persisted'; END IF;
END $$;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true),set_config('openvietgis.marketplace_reviewer','no',true);
INSERT INTO extensions.marketplace_v2_withdrawals(submission_id,requested_by,reason)
VALUES ('71000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001','synthetic withdrawal request');
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000003',true);
DO $$ BEGIN IF EXISTS(SELECT 1 FROM extensions.marketplace_v2_withdrawals) THEN RAISE EXCEPTION 'withdrawal leak'; END IF; END $$;
COMMIT;
RESET ROLE;
SELECT 'SPEC075_DEVELOPER_V2_RLS_PASS' AS result;
