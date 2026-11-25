# E11 staged Rust spatial replay

Base `e10dd403`. The candidate feature `e11-candidate` mounts one project-wide
invalidation polling route for direct staging tests. The feature is disabled in
the standard Rust build. Existing hybrid routing, collaboration SSE, managed QGIS
delta/SSE and presence WebSocket contracts keep their existing owners.

## Candidate HTTP contract

`GET /api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/spatial-invalidations`

The route requires a live human session and unrestricted `project.read`, with
authorization rechecked in the RLS-bound database transaction on each poll.
Responses carry `Cache-Control: no-store`. No feature, actor or message
payloads are serialized.

Optional query parameters: `limit` 1–500 (default 100); `epoch` and `after`
must be used together, with the UUID and sequence from the latest cursor.
`events` mode returns ordered `{sequence, kind}` entries, `next_cursor` and
`has_more`. `snapshot_required` mode carries the latest cursor when a retained
history cannot bridge the requested checkpoint. After taking a fresh snapshot,
the consumer resumes from that cursor. Malformed cursors are denied. A missing
candidate schema returns HTTP 503.

## E03-to-E11 transaction trace

The Rust E03 handler creates or reverses a changeset through `EditScope` and
awaits the database commit before returning 201. The staged SQL installs a
privileged, fixed-search-path trigger on `editing.changesets`. Once installed,
it appends to the organization/project outbox in the same transaction, including
for writes coming from FastAPI. The outbox sequence lock is held until commit.
A retry that returns a stored changeset receipt never inserts a second changeset.
A rejected or rolled-back mutation cannot make its event visible. Runtime
credentials cannot call append or write the outbox directly.

The existing migrations have separate triggers for collaboration wake-up
`NOTIFY` and the managed QGIS project event ledger. The staging outbox neither
replaces those triggers nor adds public route ownership. Existing collaboration
SSE has no replay cursor. The managed QGIS delta uses a separate session and
richer persistence contract, so the polling route has its own staging contract.

## Verification

The E11 job builds with the explicit feature and runs the Rust repository and
HTTP probes against a disposable PostGIS database. It exercises authenticated
paging, scoped and foreign denials, invalid and expired cursors, session
revocation, a Rust E03 commit, duplicate retry, stale rejection, revert, the
retained collaboration notification and managed event ledger, and schema rollback.
The ordinary Rust unit suite also checks the route is absent when the feature is
disabled.

Outstanding before rollout: dedicated CI evidence, consumer integration,
operational/resource measurements, approved Alembic promotion of the staging SQL,
and full legacy SSE/WS, chat, tasks, lease, and managed connector qualification.
