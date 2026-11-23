# Agent backend — Pairing, API, data boundaries

Governing spec R025, tasks T007–T010. Owned scope: API module/service contracts, migrations, RLS policies, integration tests if real test database available. Review `docs/developer-integrations.md`, R010 read-only implementation, project authorization, audit and changeset mechanics before editing.

Never reinterpret partner `project.read` credentials as write access. Device credentials are separate and bound to one project, revocable, hash-only at rest. Require idempotency and revision pins. Start with preview-only authenticated endpoint; no automatic apply/delete. Check credential scopes, CSRF/service identity, tenant isolation, rate limit and logs.

Avoid migration conflicts and changes to live. Record evidence in `docs/qa/r025-backend.md`. If missing authorized test DB, leave tests pending and report blocker rather than invent pass.
