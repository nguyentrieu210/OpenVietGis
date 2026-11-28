-- Review candidate, not mounted migration or payment integration.
-- Existing extension registry remains the authority for installation/activation.
-- Before adoption: tenant RLS/role design, FK against exact registry schema,
-- audited operator mutation, revision CAS, revocation event and retention.
CREATE TABLE extensions.mobile_entitlements (
 id uuid PRIMARY KEY, organization_id uuid NOT NULL, manifest_id uuid NOT NULL,
 version text NOT NULL, release_digest text NOT NULL CHECK(release_digest ~ '^[a-f0-9]{64}$'),
 state text NOT NULL CHECK(state IN ('active','unpaid','revoked','expired')),
 not_before timestamptz NOT NULL, expires_at timestamptz NOT NULL,
 revision bigint NOT NULL CHECK(revision>0),
 CHECK(expires_at>not_before), UNIQUE(organization_id,manifest_id,release_digest)
);
REVOKE ALL ON extensions.mobile_entitlements FROM PUBLIC;
ALTER TABLE extensions.mobile_entitlements ENABLE ROW LEVEL SECURITY;
ALTER TABLE extensions.mobile_entitlements FORCE ROW LEVEL SECURITY;
-- No permissive policies/grants until reviewed.
