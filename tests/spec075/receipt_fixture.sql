CREATE ROLE openvietgis_runtime NOLOGIN;
CREATE SCHEMA extensions;
CREATE SCHEMA gis_access;
GRANT USAGE ON SCHEMA extensions,gis_access TO openvietgis_runtime;
CREATE FUNCTION gis_access.scoped(uuid,uuid) RETURNS boolean LANGUAGE sql STABLE AS $$ SELECT coalesce(nullif(current_setting('test.gis_scoped',true),''),'false')::boolean $$;
CREATE TABLE extensions.manifests (organization_id uuid NOT NULL,id uuid NOT NULL,PRIMARY KEY(organization_id,id));
CREATE TABLE extensions.project_activations (organization_id uuid NOT NULL,project_id uuid NOT NULL,id uuid NOT NULL,PRIMARY KEY(organization_id,project_id,id));
