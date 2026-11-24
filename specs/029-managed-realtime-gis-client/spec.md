# R029 — Managed Realtime GIS Client
**Branch**: feature/029-managed-realtime-gis-client
**Created**: 2026-11-24
**Status**: Draft / ready for implementation planning; no runtime claims
**Baseline**: main 57164b5; R027 plugin 0.8.0 inspected at 529fade.
**Input**: QGIS is a managed client of OpenVietGIS; server-authoritative edits, two-way realtime with Web and AI, permission revocation, optional AI control of an active QGIS session.

## Scope and boundaries
Reuse R002 tenancy/permissions, R003 spatial catalog/CRS, R006 changesets/audit/revert, R011 collaboration, R021 MCP, R026 Integration Hub and R027 binding/workspace. R028 branch number reserved for Windows V8i receiver. No new standalone server or direct internal PostGIS credentials. Keep R027 snapshot sync as a separately labeled legacy/offline mode. Git stores code, not operational spatial data.
Managed mode is online-only for edits; no offline authoritative writes. Local copies and external QGIS installations cannot be made tamper-proof; authorized exports remain portable. Security guarantee is rejection of unauthorized official mutations and reads, not DRM over downloaded data. For a stronger no-local-data environment, consider a separately scoped hosted desktop/VDI, not an MVP promise.

## User stories and independent acceptance
### US1 — Managed session and central permission (P1)
Authorized employee signs in as an individual through an approved human auth flow, chooses an organization/project, opens allowed layers. Viewer sees read-only layers; editor can edit only permitted operations/fields/features/regions. Admin enables managed binding per project; defaults deny.
Acceptance: viewer cannot commit even with modified client or raw API; editor cannot cross project, edit forbidden attribute or move geometry outside permitted region; revoke blocks next server request and client becomes read-only on notice/heartbeat. No personal token in project file, cache or logs.
### US2 — Server-authoritative QGIS edits (P1)
QGIS native editing tools stage geometry/attributes; save invokes R006 using current revisions and stable operation ID. Successful save means durable server receipt, not a successful local GeoPackage write.
Acceptance: rejection or offline leaves edits marked unsaved/unconfirmed without changing authoritative state; lost response retrieves same receipt; add/update/delete respect policy, delete default off and explicit impact approval. Undo after accepted commit is a new authorized revert, not silent rollback.
### US3 — Realtime convergence across QGIS, Web and AI (P1)
Any accepted mutation via R006 becomes a durable ordered invalidation; clients fetch authorized deltas by cursor and update changed features without reloading entire GeoPackage. Reconnect catches gaps; if retention expires, scoped rebootstrap.
Acceptance: AI edits feature A, QGIS receives it; QGIS edits B, web receives it; duplicate/reordered signals or restart do not lose/duplicate changes; uncommitted local edits never silently overwritten; remote schema/CRS changes require revalidation.
### US4 — Concurrent editing (P1)
Use optimistic version preconditions and optional expiring feature leases. Same-field/geometry conflict cannot silently overwrite; independent changes may merge only when server policy validates the result and current revision.
Acceptance: two users modify same feature concurrently; stale write rejected with safe conflict response; explicit conflict resolution generates R006 changeset; lease expiry does not bypass revision check.
### US5 — AI desktop bridge (P2)
AI can request allowlisted tools from an active opted-in QGIS session: inspect project/layer, select/zoom, preview topology/processing, stage allowed edits. Server MCP can already mutate through existing R006; desktop bridge is optional for native QGIS UI/tools.
Acceptance: session pairing bound to user/project/device/expiry, local consent and visible operation preview; no arbitrary Python/COM/shell/filesystem/network tools; mutating actions require existing GIS approval and server authorization; session closed/revoked fails closed.
### US6 — Operations and admin control (P2)
Admin sees client state, binding scope, last acknowledged cursor, sync health, audit origin and revocation. Legacy mode remains opt-in separate. No release claims before native desktop tests.

## Functional requirements
- FR-001: Reuse existing server auth and layer/feature/field/region ACL; verify individual identity and binding scope for every read/write/replay. Default deny; no broad shared service key as human identity.
- FR-002: Server owns canonical PostGIS state; use R006 exclusive mutation path, atomic receipts/audit, explicit CRS and existing revision semantics; no direct SQL from clients.
- FR-003: Managed layer save MUST require server acceptance; network/policy failure must not report local success as authoritative. Native provider feasibility spike is a release blocker.
- FR-004: Delta events durable, transactionally tied to canonical mutation; monotonic project cursor with defined commit ordering, deduplication and retention; SSE remains wake-up only.
- FR-005: Scope-filter delta reads on current rights; no unauthorized IDs/geometry/attributes; explicit visibility removals or safe cache reset when ACL changes.
- FR-006: Client persists acknowledged cursor only after applying corresponding authorized state; idempotent operation retry and receipt query; gap/expired cursor triggers scoped rebootstrap.
- FR-007: Permission epoch, bounded recheck of long-lived SSE/session, read-only lockout on disconnect/expiry/revoke; no unauthorized retry; clear or quarantine managed cache according to policy.
- FR-008: Managed edits use stable feature IDs, revisions and idempotency keys; conflict on stale revision; no last-write-wins. Optional leases advisory, never security boundary.
- FR-009: Geometry, CRS, schema validation and R006 review/delete rules remain enforced; no automatic deletion.
- FR-010: Background controller independent of dock visibility; worker I/O off GUI thread; apply changes on QGIS main thread safely; no silent edit buffer overwrite.
- FR-011: AI server operations preserve current approval gates; desktop bridge allowlisted, session scoped, revocable, explicit consent, no arbitrary execution.
- FR-012: UI/admin truthfully show pending/accepted/conflicted/denied/offline/unsupported; origin actor/device and revisions auditable, no credentials or denied data in logs.
- FR-013: Bounded pages/batches/rate limits and tenant fairness; avoid full GeoPackage transfers in steady-state managed mode.
- FR-014: Existing R027 legacy mode remains available with distinct labeling and tests; managed mode feature flag defaults off until qualification.
- FR-015: Cross-client, real PostGIS and native QGIS tests mandatory; deploy only after security, performance and rollback gates.

## Success criteria (targets, not measured)
SC-001: 100% denied operation suite fails server-side including forged client, cross-tenant, stale token and changed ACL.
SC-002: p95 accepted change visible in other active clients within 2 s on reference network (RTT <=100 ms, 10 clients, 10k features, 1 feature change/s); report p99 and workload.
SC-003: zero missing or duplicate authoritative changes after restart/reconnect/reordered notification in 100 scripted scenarios; full snapshot only bootstrap/recovery.
SC-004: revoked session cannot perform next new authoritative write; connected client read-only within 15 s under reference network; offline managed save fails closed.
SC-005: native QGIS 3.34+ supported version matrix verified on Linux and Windows; 2 QGIS clients + Web + AI end-to-end; no unsupported provider claims.
SC-006: Every accepted mutation linked to R006 audit receipt; zero false accepted status on rejection or response loss.

## Assumptions
Managed mode prioritizes data integrity over availability. No scope for style parity, arbitrary desktop remote control, other CAD adapter certification, or client OS lockdown in MVP. Feature-level revision support must reuse existing R006 semantics or be introduced in a backward-compatible migration after spike; initial layer revision conflicts are acceptable.
