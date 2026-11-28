-- After the existing 0028/0058 fixture and exact 0060 upgrade.
\set ON_ERROR_STOP on
SET ROLE openvietgis_runtime;
BEGIN;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000001',true),set_config('openvietgis.marketplace_reviewer','no',true);
DO $$ BEGIN
 IF (SELECT signing_public_key FROM extensions.marketplace_submissions WHERE id='71000000-0000-0000-0000-000000000001')<>repeat('a',44) THEN RAISE EXCEPTION 'historical key not backfilled'; END IF;
END $$;
INSERT INTO extensions.marketplace_publisher_rotations(publisher_id,owner_user_id,revision,old_public_key,new_public_key,old_proof,new_proof)
VALUES ('dev_aaaaaaaaaaaaaaaaaaaaaaaa','51000000-0000-0000-0000-000000000001',2,repeat('a',44),repeat('d',44),repeat('e',88),repeat('f',88));
UPDATE extensions.marketplace_publishers SET public_key=repeat('d',44),key_revision=2 WHERE id='dev_aaaaaaaaaaaaaaaaaaaaaaaa' AND key_revision=1;
INSERT INTO extensions.marketplace_submissions(id,publisher_id,owner_user_id,extension_key,version,manifest,signature)
VALUES ('71000000-0000-0000-0000-000000000002','dev_aaaaaaaaaaaaaaaaaaaaaaaa','51000000-0000-0000-0000-000000000001','partner_dev_aaaaaaaaaaaaaaaaaaaaaaaa_test','2.0.0','{}',repeat('b',88));
DO $$ BEGIN
 IF (SELECT signing_public_key FROM extensions.marketplace_submissions WHERE id='71000000-0000-0000-0000-000000000001')<>repeat('a',44) OR
    (SELECT signing_public_key FROM extensions.marketplace_submissions WHERE id='71000000-0000-0000-0000-000000000002')<>repeat('d',44) THEN RAISE EXCEPTION 'signing key history changed'; END IF;
 BEGIN
 UPDATE extensions.marketplace_submissions SET signing_public_key=repeat('d',44);
 RAISE EXCEPTION 'historical signing key mutable';
 EXCEPTION WHEN insufficient_privilege THEN NULL; END;
END $$;
SELECT set_config('openvietgis.actor_user_id','51000000-0000-0000-0000-000000000003',true);
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM extensions.marketplace_publisher_rotations) THEN RAISE EXCEPTION 'rotation leak'; END IF;
 UPDATE extensions.marketplace_publishers SET public_key=repeat('z',44);
 IF FOUND THEN RAISE EXCEPTION 'cross-owner rotation'; END IF;
END $$;
COMMIT;
RESET ROLE;
SELECT 'SPEC075_PUBLISHER_ROTATION_PASS' AS result;
