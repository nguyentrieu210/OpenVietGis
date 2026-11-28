-- EMPTY disposable database only. Minimal canonical interfaces, not full schema.
CREATE ROLE openvietgis_runtime LOGIN PASSWORD 'spec075-fixture-only';
CREATE SCHEMA extensions;
CREATE SCHEMA org;
CREATE SCHEMA identity;
GRANT USAGE ON SCHEMA extensions,org,identity TO openvietgis_runtime;
CREATE TABLE org.organizations(id uuid PRIMARY KEY,status text NOT NULL);
CREATE TABLE org.organization_memberships(organization_id uuid,user_id uuid,role text,status text,PRIMARY KEY(organization_id,user_id));
CREATE TABLE identity.users(id uuid PRIMARY KEY,status text);
CREATE TABLE identity.identity_links(id uuid PRIMARY KEY,user_id uuid);
CREATE TABLE identity.web_sessions(id uuid PRIMARY KEY,user_id uuid,identity_link_id uuid,revoked_at timestamptz,expires_at timestamptz);
CREATE TABLE extensions.manifests(id uuid PRIMARY KEY);
ALTER TABLE extensions.manifests ENABLE ROW LEVEL SECURITY;
ALTER TABLE extensions.manifests FORCE ROW LEVEL SECURITY;
GRANT SELECT ON extensions.manifests TO openvietgis_runtime;
GRANT SELECT,UPDATE ON org.organizations,org.organization_memberships TO openvietgis_runtime;
GRANT SELECT ON ALL TABLES IN SCHEMA identity TO openvietgis_runtime;
INSERT INTO org.organizations VALUES ('11000000-0000-0000-0000-000000000001','active'),('11000000-0000-0000-0000-000000000002','active');
INSERT INTO org.organization_memberships VALUES
('11000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001','owner','active'),
('11000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000002','member','active'),
('11000000-0000-0000-0000-000000000002','51000000-0000-0000-0000-000000000001','owner','active');
INSERT INTO identity.users VALUES ('51000000-0000-0000-0000-000000000001','active'),('51000000-0000-0000-0000-000000000002','active');
INSERT INTO identity.identity_links VALUES ('61000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001'),('61000000-0000-0000-0000-000000000002','51000000-0000-0000-0000-000000000002');
INSERT INTO identity.web_sessions VALUES
('71000000-0000-0000-0000-000000000001','51000000-0000-0000-0000-000000000001','61000000-0000-0000-0000-000000000001',NULL,clock_timestamp()+interval '1 day'),
('71000000-0000-0000-0000-000000000002','51000000-0000-0000-0000-000000000002','61000000-0000-0000-0000-000000000002',NULL,clock_timestamp()+interval '1 day');
