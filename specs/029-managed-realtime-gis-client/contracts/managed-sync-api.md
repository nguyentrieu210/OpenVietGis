# Draft protocol (not implemented)
Base: /api/v1/organizations/{org}/projects/{project}/integration-hub/managed
All endpoints require current project and resource access + session scope; existing human CSRF/auth and service ceilings preserved. Endpoint naming subject to compatibility review.
- POST /sessions: negotiate client version, managed binding, device, requested mode; return session ID, expiry, epoch, capabilities. Default deny.
- GET /sessions/{id}: state, epoch, capability, expiration; DELETE revokes with admin/self permission.
- GET /bootstrap?layer_id=...: authorized snapshot with stable revision/cursor boundary; paginated, no direct DB credentials.
- GET /delta?cursor=...&limit=...: authorized changes only, ordered cursor, has_more, epoch; 410/structured reset on expired cursor, 409 epoch change; opaque cursor bound to scope where appropriate.
- GET /events: reuse SSE semantics or extend existing R027 route compatibly; wake-up signal only; heartbeat and current auth check <=15s target; reconnect reads durable delta.
- POST /commits: adapter wrapper around existing R006 changeset service (prefer existing route extension if safe). Request: session_id, source_id, base_content_revision, base_schema_revision, operation_id, operations, optional reviewed delete token. Return accepted receipt and revision/cursor only after commit. 401/403 deny, 409 stale/conflict/idempotency mismatch, 422 invalid geometry/schema, 429 bounded retry. Never pretend local save succeeded.
- GET /commits/{operation_id}: scoped original receipt for uncertain response recovery; verify actor and digest.
- Optional P2 POST /desktop-sessions/{id}/commands: typed allowlisted command, expiry and consent binding; no arbitrary Python, shell or code.
## Semantics
Canonical R006 edits and event row atomic; notification after commit; duplicate wakeups harmless. Bootstrap and cursor must agree (no race). Server ACL on read and write including pre/post geometry and field-level redaction; revocation invalidates open streams/managed state and blocks next request. Error bodies must not leak inaccessible records. TLS only except explicit dev loopback.
