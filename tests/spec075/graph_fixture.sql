-- Run ONLY after commerce_fixture.sql in a disposable DB; then 0017,0019,0053,0054,0056.
DROP TABLE extensions.manifests;
DROP SCHEMA extensions;
CREATE SCHEMA workspace;
CREATE SCHEMA spatial;
CREATE SCHEMA audit;
CREATE SCHEMA gis_access;
GRANT USAGE ON SCHEMA workspace,spatial,audit,gis_access TO openvietgis_runtime;
ALTER TABLE org.organization_memberships ADD COLUMN id uuid DEFAULT gen_random_uuid();
CREATE TABLE org.resource_grants(organization_id uuid,membership_id uuid,subject_kind text,revoked_at timestamptz,role text,scope_kind text,workspace_id uuid,project_id uuid);
GRANT SELECT ON org.resource_grants TO openvietgis_runtime;
CREATE TABLE workspace.projects(organization_id uuid,id uuid,status text,workspace_id uuid,PRIMARY KEY(organization_id,id));
GRANT SELECT,UPDATE ON workspace.projects TO openvietgis_runtime;
INSERT INTO workspace.projects VALUES ('11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000001','active',NULL),('11000000-0000-0000-0000-000000000001','31000000-0000-0000-0000-000000000002','active',NULL);
CREATE FUNCTION gis_access.scoped(uuid,uuid) RETURNS boolean LANGUAGE sql STABLE AS $$ SELECT false $$;
CREATE TABLE spatial.layers(id uuid);
CREATE TABLE spatial.data_sources(id uuid);
CREATE TABLE spatial.source_fields(id uuid);
CREATE TABLE spatial.reference_features(id uuid);
DO $$ DECLARE t text; BEGIN FOREACH t IN ARRAY ARRAY['layers','data_sources','source_fields','reference_features'] LOOP
 EXECUTE format('ALTER TABLE spatial.%I ENABLE ROW LEVEL SECURITY',t);
 EXECUTE format('ALTER TABLE spatial.%I FORCE ROW LEVEL SECURITY',t);
 EXECUTE format('GRANT SELECT ON spatial.%I TO openvietgis_runtime',t);
END LOOP; END $$;
CREATE TABLE audit.audit_events(id uuid PRIMARY KEY,organization_id uuid,actor_type text,actor_id uuid,actor_display_snapshot text,action text,resource_type text,resource_id uuid,outcome text,metadata jsonb,created_at timestamptz);
GRANT INSERT,SELECT ON audit.audit_events TO openvietgis_runtime;
