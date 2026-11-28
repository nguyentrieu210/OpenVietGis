-- Disposable PostgreSQL fixture mirroring identity columns used by candidate.
-- NOT full platform migration or production RLS acceptance.
CREATE ROLE openvietgis_runtime LOGIN PASSWORD 'fixture-runtime' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS;
CREATE SCHEMA identity;
CREATE TABLE identity.users(id uuid PRIMARY KEY,display_name varchar(200) NOT NULL,email_snapshot varchar(320),status varchar(32) NOT NULL CHECK(status IN('active','disabled')),created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE identity.identity_links(id uuid PRIMARY KEY,user_id uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,issuer text NOT NULL,subject text NOT NULL,email_snapshot varchar(320),email_verified boolean NOT NULL DEFAULT false,display_name_snapshot varchar(200),created_at timestamptz NOT NULL DEFAULT now(),last_login_at timestamptz,UNIQUE(issuer,subject));
CREATE TABLE identity.web_sessions(id uuid PRIMARY KEY,user_id uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,identity_link_id uuid NOT NULL REFERENCES identity.identity_links(id) ON DELETE CASCADE,secret_hash bytea NOT NULL,issued_at timestamptz NOT NULL DEFAULT now(),expires_at timestamptz NOT NULL,last_seen_at timestamptz,revoked_at timestamptz);
INSERT INTO identity.users(id,display_name,status) VALUES('22222222-2222-4222-8222-222222222222','Fixture','active');
INSERT INTO identity.identity_links(id,user_id,issuer,subject) VALUES('33333333-3333-4333-8333-333333333333','22222222-2222-4222-8222-222222222222','fixture','test');
INSERT INTO identity.web_sessions(id,user_id,identity_link_id,secret_hash,expires_at) VALUES('11111111-1111-4111-8111-111111111111','22222222-2222-4222-8222-222222222222','33333333-3333-4333-8333-333333333333',sha256(convert_to('fixture-secret','UTF8')),now()+interval '1 day');
