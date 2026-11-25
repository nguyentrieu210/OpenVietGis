# E11 checkpoint — outbox hardening

Base: 1a5cb7b (Spec 064 handoff). Worktree/branch: 064-e11.
Scope: existing in-memory prototype in ovg-sync, not the production Python realtime routes.

- Scope idempotency lookup to organization AND project; evict index entries with retained events.
- Reject noncanonical and oversized stream cursor tokens before decoding.
- Limit replay cloning to page size plus one for has_more.
- Add regression tests covering cross-organization key collisions, index retention and cursor validation.

Validation: git diff --check. Rust compilation, formatting and tests must run in GitHub Actions (handoff rule); no local/VPS cargo. No release or production migration.

Open gates:
- In-memory replay is not durable; implement transactional Postgres outbox with authenticated and authorized scope, durable retention and retry guarantees.
- Global sequence and global broadcast lag may expose cross-scope activity metadata; isolate scope before exposing replay or fanout to clients.
- Existing SSE/WS wire semantics, actor-specific notifications, ACL revalidation, edit leases, org chat and managed sync require parity coverage and implementation.
- E11-T02/T03/T04/T05 remain incomplete. No measured performance result.
