-- Disposable fixture only. Never apply to an existing database.
CREATE SCHEMA ovg066_fixture;
CREATE TABLE ovg066_fixture.features (
  organization_id uuid NOT NULL, project_id uuid NOT NULL, source_id uuid NOT NULL,
  feature_id varchar(128) COLLATE "C" NOT NULL,
  geometry geometry, geometry_state varchar(32) NOT NULL,
  content_revision varchar(64) NOT NULL, schema_revision varchar(64) NOT NULL,
  attributes jsonb NOT NULL DEFAULT '{}',
  PRIMARY KEY (organization_id,project_id,source_id,feature_id)
);
CREATE INDEX fixture_geometry ON ovg066_fixture.features USING gist (geometry)
 WHERE geometry_state='present';
INSERT INTO ovg066_fixture.features
SELECT '00000000-0000-0000-0000-000000000001'::uuid,
 '00000000-0000-0000-0000-000000000002'::uuid,
 ('00000000-0000-0000-0000-'||lpad(scale::text,12,'0'))::uuid,
 'p'||lpad(i::text,8,'0'),
 ST_SetSRID(ST_MakePoint(105.5+(i%1000)/1000.0,20.5+(i/1000%1000)/1000.0),4326),
 'present',repeat('a',64),repeat('b',64),jsonb_build_object('visible',i%3=0)
FROM (VALUES(30000),(100000),(1000000)) AS s(scale) CROSS JOIN LATERAL generate_series(1,scale) AS i;
-- Decoy tenant, same project/source and feature IDs.
INSERT INTO ovg066_fixture.features
SELECT '00000000-0000-0000-0000-000000000009'::uuid,project_id,source_id,feature_id,
 geometry,geometry_state,content_revision,schema_revision,attributes
FROM ovg066_fixture.features WHERE source_id='00000000-0000-0000-0000-000000100000';
GRANT USAGE ON SCHEMA ovg066_fixture TO openvietgis_runtime;
GRANT SELECT ON ovg066_fixture.features TO openvietgis_runtime;
ALTER TABLE ovg066_fixture.features ENABLE ROW LEVEL SECURITY;
ALTER TABLE ovg066_fixture.features FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_scope ON ovg066_fixture.features TO openvietgis_runtime
 USING (organization_id=NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid
 AND project_id=NULLIF(current_setting('openvietgis.project_id',true),'')::uuid);
-- Simplified restrictive object ACL; not a substitute for actual application policy.
CREATE POLICY object_scope ON ovg066_fixture.features AS RESTRICTIVE TO openvietgis_runtime
 USING ((SELECT current_setting('ovg066.scope',true))='full' OR attributes @> '{"visible":true}');
ANALYZE ovg066_fixture.features;
