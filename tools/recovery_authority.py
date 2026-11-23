"""Fail-closed recovery authority comparison and job quarantine.

Only an independently attested *current* authority database may preserve
historical human access. This is a conservative intersection, not an import of
new permissions. All restored bearer secrets, service identities, invitations,
offline packages and incomplete job attempts are revoked/quarantined regardless.
"""

from __future__ import annotations

from typing import Any
from uuid import UUID

RECOVERY_QUARANTINED = "RECOVERY_QUARANTINED"

# No credential hashes, token material, email or identity subjects leave the
# authority database. Link fingerprints include ID, issuer and subject so a
# removed/reassigned link makes the entire user's historical identity unsafe.
# Organization envelopes include the complete resource lifecycle, because
# implicit owner/admin permissions also depend on scope state.
AUTHORITY_QUERY = """
SELECT json_build_object(
  'users', COALESCE((
    SELECT json_agg(row_to_json(u) ORDER BY id) FROM (
      SELECT id::text AS id, COALESCE((
        SELECT json_agg(encode(sha256(convert_to(
          jsonb_build_array(l.id, l.issuer, l.subject)::text, 'UTF8')), 'hex')
                        ORDER BY l.id)
        FROM identity.identity_links l WHERE l.user_id = identity.users.id
      ), '[]'::json) AS links FROM identity.users WHERE status = 'active'
    ) u
  ), '[]'::json),
  'organizations', COALESCE((
    SELECT json_agg(row_to_json(o) ORDER BY id) FROM (
      SELECT id::text AS id, COALESCE((
        SELECT json_agg(jsonb_build_array(w.id, w.status) ORDER BY w.id)
        FROM workspace.workspaces w WHERE w.organization_id = org.organizations.id
      ), '[]'::json) AS workspaces, COALESCE((
        SELECT json_agg(jsonb_build_array(p.id, p.workspace_id, p.status) ORDER BY p.id)
        FROM workspace.projects p WHERE p.organization_id = org.organizations.id
      ), '[]'::json) AS projects FROM org.organizations WHERE status = 'active'
    ) o
  ), '[]'::json),
  'memberships', COALESCE((
    SELECT json_agg(row_to_json(m) ORDER BY id) FROM (
      SELECT id::text AS id, user_id::text, organization_id::text, role,
        COALESCE((
          SELECT json_agg(
            jsonb_build_array(g.id, g.scope_kind, g.workspace_id, g.project_id, g.role)
            ORDER BY g.id
          ) FROM org.resource_grants g
          WHERE g.membership_id = org.organization_memberships.id
            AND g.organization_id = org.organization_memberships.organization_id
            AND g.subject_kind = 'membership' AND g.revoked_at IS NULL
        ), '[]'::json) AS grants
      FROM org.organization_memberships WHERE status = 'active' AND revoked_at IS NULL
    ) m
  ), '[]'::json)
);
"""


# Hash the complete policy envelope, not just organization-level grants.
# A changed fine-grained rule, group or scoped membership must never be
# restored as historical authority. Scope values remain inside PostgreSQL.
GIS_AUTHORITY_QUERY = """
SELECT encode(sha256(convert_to(COALESCE(string_agg(
  encode(sha256(convert_to(jsonb_build_array(kind, row_data)::text, 'UTF8')), 'hex'),
  '' ORDER BY kind, row_data::text), ''), 'UTF8')), 'hex')
FROM (
  SELECT 'scoped' AS kind, to_jsonb(s) AS row_data FROM gis_access.scoped_members s
  UNION ALL SELECT 'group', to_jsonb(g) FROM gis_access.groups g
  UNION ALL SELECT 'group_member', to_jsonb(m) FROM gis_access.group_members m
  UNION ALL SELECT 'rule', to_jsonb(r) FROM gis_access.rules r
) policy;
"""


def _indexed(snapshot: dict[str, Any], kind: str) -> dict[str, Any]:
    rows = snapshot[kind]
    result = {}
    for row in rows:
        key = str(UUID(row["id"]))
        if key in result:
            raise RuntimeError("Duplicate authority identifier")
        result[key] = row
    return result


def safe_human_authority(
    restored: dict[str, Any], current: dict[str, Any] | None
) -> tuple[list[str], list[str]]:
    """Return only unchanged human identity and membership envelopes.

    A missing, changed, revoked or newly added scope, permission or identity
    link never restores access to that historical membership. No current source
    means no retained authority. An authoritative snapshot is obtained securely
    by the operator, never inferred from the historical dump.
    """
    if current is None:
        return [], []
    # A differing or missing fine-grained policy fingerprint means the
    # historical permission graph cannot be proven safe. Quarantine all
    # human authority until it has been independently re-established.
    if restored.get("gis_access") != current.get("gis_access"):
        return [], []
    old_users, new_users = (_indexed(x, "users") for x in (restored, current))
    old_orgs, new_orgs = (_indexed(x, "organizations") for x in (restored, current))
    old_members, new_members = (_indexed(x, "memberships") for x in (restored, current))
    # Full link envelope equality also excludes a user with a removed link.
    users = sorted(
        key
        for key, row in old_users.items()
        if row["links"] and new_users.get(key) == row
    )
    orgs = {key for key, row in old_orgs.items() if new_orgs.get(key) == row}
    members = sorted(
        key
        for key, row in old_members.items()
        if new_members.get(key) == row
        and row["user_id"] in users
        and row["organization_id"] in orgs
    )
    return users, members


def _uuid_array(ids: list[str]) -> str:
    # Strict UUID parsing protects interpolation. No user-supplied SQL.
    return "ARRAY[" + ",".join("'" + str(UUID(x)) + "'" for x in ids) + "]::uuid[]"


def reconciliation_sql(users: list[str], members: list[str]) -> str:
    """Build one transaction for a freshly restored, runtime-disconnected DB."""
    user_ids, member_ids = _uuid_array(users), _uuid_array(members)
    return f"""
BEGIN;
SET LOCAL row_security = off;
-- Enforce a one-way quarantine of restored bearer and machine authority.
UPDATE identity.web_sessions SET revoked_at=now() WHERE revoked_at IS NULL;
UPDATE org.service_credentials SET revoked_at=now() WHERE revoked_at IS NULL;
UPDATE org.service_principals SET status='revoked', revoked_at=now()
  WHERE status='active';
UPDATE org.invitations SET status='revoked', revoked_at=now() WHERE status='pending';
UPDATE field.offline_packages SET revoked_at=now() WHERE revoked_at IS NULL;
UPDATE org.resource_grants SET revoked_at=now()
  WHERE revoked_at IS NULL AND (
    subject_kind <> 'membership' OR NOT (membership_id=ANY({member_ids}))
  );
UPDATE org.organization_memberships SET status='revoked', revoked_at=now()
  WHERE status='active' AND NOT (id=ANY({member_ids}));
UPDATE identity.users SET status='disabled', updated_at=now()
  WHERE status='active' AND NOT (id=ANY({user_ids}));
-- Close incomplete/non-successful processing jobs; retry must not resurrect
-- these IDs after access is re-established. Published successes stay intact.
UPDATE processing.jobs SET status='cancelled', error_code='{RECOVERY_QUARANTINED}',
  error_message='Restored historical job; submit a newly authorized operation.',
  updated_at=now(), completed_at=COALESCE(completed_at,now())
  WHERE status <> 'succeeded';
UPDATE transfers.jobs SET status='denied', error_code='{RECOVERY_QUARANTINED}',
  error_message='Restored historical job; submit a newly authorized operation.',
  updated_at=now(), completed_at=COALESCE(completed_at,now())
  WHERE status NOT IN ('succeeded','failed','denied','cancelled');
UPDATE transfers.job_attempts SET status='denied', error_code='{RECOVERY_QUARANTINED}',
  error_message='Restored interrupted attempt; not resumable.',
  finished_at=now() WHERE finished_at IS NULL;
-- Assert invariants before COMMIT, not after exposing data.
DO $recovery$
BEGIN
  IF EXISTS (SELECT 1 FROM identity.web_sessions WHERE revoked_at IS NULL)
     OR EXISTS (SELECT 1 FROM org.service_credentials WHERE revoked_at IS NULL)
     OR EXISTS (SELECT 1 FROM org.service_principals WHERE status='active')
     OR EXISTS (SELECT 1 FROM org.invitations WHERE status='pending')
     OR EXISTS (SELECT 1 FROM field.offline_packages WHERE revoked_at IS NULL)
     OR EXISTS (SELECT 1 FROM org.resource_grants WHERE revoked_at IS NULL
          AND (subject_kind<>'membership' OR NOT (membership_id=ANY({member_ids}))))
     OR EXISTS (SELECT 1 FROM org.organization_memberships WHERE status='active'
          AND NOT (id=ANY({member_ids})))
     OR EXISTS (SELECT 1 FROM identity.users WHERE status='active'
          AND NOT (id=ANY({user_ids})))
     OR EXISTS (SELECT 1 FROM processing.jobs WHERE status<>'succeeded'
          AND (status<>'cancelled' OR error_code IS DISTINCT FROM '{RECOVERY_QUARANTINED}'))
     OR EXISTS (SELECT 1 FROM transfers.jobs
          WHERE status NOT IN ('succeeded','failed','denied','cancelled'))
     OR EXISTS (SELECT 1 FROM transfers.job_attempts WHERE finished_at IS NULL)
  THEN RAISE EXCEPTION 'Recovery reconciliation invariant failed';
  END IF;
END
$recovery$;
COMMIT;
"""


QUARANTINE_REPORT_QUERY = """
SELECT json_build_object(
  'active_users', (SELECT count(*) FROM identity.users WHERE status='active'),
  'active_memberships', (SELECT count(*) FROM org.organization_memberships WHERE status='active'),
  'active_grants', (SELECT count(*) FROM org.resource_grants WHERE revoked_at IS NULL),
  'unrevoked_sessions', (SELECT count(*) FROM identity.web_sessions WHERE revoked_at IS NULL),
  'unrevoked_service_credentials', (SELECT count(*) FROM org.service_credentials WHERE revoked_at IS NULL),
  'active_service_principals', (SELECT count(*) FROM org.service_principals WHERE status='active'),
  'pending_invitations', (SELECT count(*) FROM org.invitations WHERE status='pending'),
  'unrevoked_offline_packages', (SELECT count(*) FROM field.offline_packages WHERE revoked_at IS NULL),
  'processing_jobs_quarantined', (SELECT count(*) FROM processing.jobs
    WHERE error_code='RECOVERY_QUARANTINED'),
  'transfer_jobs_quarantined', (SELECT count(*) FROM transfers.jobs
    WHERE error_code='RECOVERY_QUARANTINED'),
  'transfer_attempts_quarantined', (SELECT count(*) FROM transfers.job_attempts
    WHERE error_code='RECOVERY_QUARANTINED')
);
"""
