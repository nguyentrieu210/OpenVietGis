-- REVIEW CANDIDATE ONLY: not an applied Alembic revision. Requires DB-owner
-- privilege/RLS, retention/rate-limit, browser consent and auth route review.
CREATE TABLE identity.mobile_grants (
 code_hash bytea PRIMARY KEY CHECK(octet_length(code_hash)=32),
 web_session_id uuid NOT NULL REFERENCES identity.web_sessions(id),
 user_id uuid NOT NULL REFERENCES identity.users(id),
 issuer text NOT NULL,audience text NOT NULL,redirect_uri text NOT NULL,
 challenge text NOT NULL,device_id uuid NOT NULL,expires_at timestamptz NOT NULL,used_at timestamptz
);
CREATE TABLE identity.mobile_sessions (
 id uuid PRIMARY KEY, web_session_id uuid NOT NULL REFERENCES identity.web_sessions(id),
 user_id uuid NOT NULL REFERENCES identity.users(id),issuer text NOT NULL,audience text NOT NULL,
 device_id uuid NOT NULL,access_hash bytea UNIQUE NOT NULL CHECK(octet_length(access_hash)=32),
 access_expires_at timestamptz NOT NULL,idle_expires_at timestamptz NOT NULL,
 absolute_expires_at timestamptz NOT NULL,revoked_at timestamptz
);
CREATE TABLE identity.mobile_refresh (
 hash bytea PRIMARY KEY CHECK(octet_length(hash)=32),session_id uuid NOT NULL REFERENCES identity.mobile_sessions(id) ON DELETE CASCADE,used_at timestamptz
);
CREATE INDEX mobile_refresh_session_idx ON identity.mobile_refresh(session_id);
REVOKE ALL ON identity.mobile_grants,identity.mobile_sessions,identity.mobile_refresh FROM PUBLIC;
