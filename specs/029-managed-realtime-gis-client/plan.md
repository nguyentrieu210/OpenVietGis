# Implementation plan — R029
## Technical context
Existing Python/FastAPI + PostgreSQL/PostGIS API, React web-next, Python/PyQGIS connector, MCP. SSE endpoint integration_bindings.py uses LISTEN/NOTIFY project invalidations; plugin.py currently downloads GPKG via workspace.py and commits local first. R006 changesets existing source of truth. No new standalone service.
## Constitution check
Platform-first: shared protocol, no vertical dependency. Portability: retain authorized export/R027 legacy. Tenant isolation: R002 + GIS ACL + RLS applied to all new endpoints; human and service scope tested separately. Explicit CRS: R003 only. Audit: R006 only. Capability-driven: managed client capability feature flag. PASS at design level; implementation verification pending.
## Phase 0 — feasibility / stop gate
Inventory exact R006 transaction, audit/revision and notifications including schema/catalog changes and visibility changes. Native PyQGIS spike compares plugin-controlled commit interception/provider contract vs OGC WFS-T gateway; choose only if real QGIS Save/undo/reject behavior is proven. Prototype server-acknowledged native save and no false success on HTTP failures. Document constraints and supported QGIS versions; if native save cannot safely be intercepted, use explicit managed Save command with standard provider read-only and no claim of transparent native save until provider proven.
## Phase 1 — backend foundation
Define session capability and policy epoch, operation receipt, transactional event ledger and cursor semantics. Add migration following highest current migration at implementation time; project sequence serialized with existing lock order, events committed atomically with canonical changes. Reuse R006 and ACL query path, not duplicate permission logic. Build scoped delta/bootstrap endpoints and SSE reconnect revalidation. Prove retention, cursor-gap, ACL visibility changes and deduplication.
## Phase 2 — managed QGIS
Create separate managed mode under integrations/qgis_plugin/openvietgis_connector; isolate session/controller from dock; approved human auth with device flow or PKCE depending on provider proof; no embedded secrets. Read-only on disconnect or expired capability; staged edits + server acknowledgment; GUI-thread apply and worker I/O; conflict and schema rebootstrap. Preserve R027.
## Phase 3 — web and admin
Extend apps/web-next/src/QgisBindingManager.tsx and existing Integration Hub: manage user/binding mode, device/session status, revoke, cursor health, policy and audit. Subscribe to existing events; fetch scoped deltas where appropriate. Keep existing frontend and public interfaces compatible.
## Phase 4 — AI bridge
Reuse apps/mcp/workspace_tools.py and existing GIS approvals for server-side edit. Add opt-in QGIS session tool bridge with typed command schema, allowlist, explicit approval for mutating actions, TTL and user/project/device binding. No generic shell/Python tool.
## Phase 5 — certification
Native QGIS Windows/Linux matrix, real PostGIS/RLS tests, simultaneous QGIS+Web+AI, network failures, permissions and load, rollback rehearsals. Release disabled by default until all P1 gates passed. P2 AI desktop optional flag independent.
## Workstream ownership
Architect: provider/ordering ADR + contract. Backend: migration/auth/delta/commit. QGIS: provider/controller and native tests. Frontend: management UX. MCP: desktop bridge/consent. QA: adversarial/E2E/load. Release: CI, compatibility and rollback. Contracts and Phase 0 gate before shared implementation; parallel isolated worktrees, integration owner merges only reviewed PRs.
