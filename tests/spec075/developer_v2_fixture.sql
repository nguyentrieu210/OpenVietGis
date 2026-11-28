-- Minimal disposable schema for actual migrations 0028 and 0058.
CREATE ROLE openvietgis_runtime NOLOGIN;
CREATE SCHEMA extensions;
CREATE SCHEMA identity;
CREATE TABLE identity.users(id uuid PRIMARY KEY);
GRANT USAGE ON SCHEMA extensions,identity TO openvietgis_runtime;
INSERT INTO identity.users VALUES ('51000000-0000-0000-0000-000000000001'),('51000000-0000-0000-0000-000000000002'),('51000000-0000-0000-0000-000000000003');
