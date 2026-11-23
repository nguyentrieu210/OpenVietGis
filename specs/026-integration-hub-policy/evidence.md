# R026 / Slice 1 — validation evidence

Recorded 2026-11-23, isolated from main@5baade9.

- Read-only catalog: 12 adapter families with separate lifecycle and per-action
  permission preconditions; no active vendor sync or storage writes.
- API: GET route registered in FastAPI OpenAPI, protected by existing
  project_access_guard and prepare_project_scope(project.read).
- Tests: 36 green (integration-hub unit + existing authorization unit).
- Python: Ruff changed sources green; mypy integration hub green.
- Frontend: TypeScript typecheck green; Vite production build green, with existing
  vendor chunk size and module directive warnings.
- Documentation navigation and architecture checks green.
- Scope: 27 existing violations on main@5baade9, 27 on this branch; no delta.
- No migration, tenant data mutation, production API restart, production frontend
  replacement or live release attempted.

Not yet accepted:
- Real PostGIS cross-tenant/service-credential and feature-level GIS ACL tests.
- Independent security review and tested rollback.
- Client SDK typed contract + browser E2E.
- Connector runtime handshake, data import, sync, device pairing or vendor licenses.

**Release decision: NOT PROMOTED.** Keep this candidate branch isolated until
all required RLS/security and rollback gates have evidence.
