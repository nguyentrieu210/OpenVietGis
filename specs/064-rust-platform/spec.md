# Feature Specification: Rust-native OpenVietGIS Platform
**Branch**: 064-rust-platform | **Created**: 2026-11-25 | **Status**: Draft, implementation not started
**Input**: Rebuild the platform architecture around Rust; retain proven product behavior and data.
**Parent**: R031 engine-first; R002–R015 as applicable; Spec 062 canonical frontend; Spec 063 native WKB experiment is separate, not a dependency.

## User Scenarios & Testing
### US1 (P1) — Existing users can open projects and edit safely
Given existing organizations, users and projects, a client using existing API contracts can authenticate, list/query layers, edit with a revision precondition and export without changing identity or coordinates. Independently test with the same restored database against both servers; unauthorized access must fail.
### US2 (P1) — Large maps remain responsive
Given 30k, 100k and 1M synthetic or authorized features, pan, zoom, filter and inspect without full-layer downloads. Independently test cold/warm viewport loads, tile ACLs, memory and edit-to-tile invalidation.
### US3 (P1) — QGIS and web converge after reconnect
Given two authorized editors, edits propagate incrementally; reconnect resumes from a durable cursor, duplicates are idempotent and conflicting revisions are surfaced. Independently test disconnect, retries, revoked permissions and restart.
### US4 (P2) — Existing workflows remain usable
Given CAD, 3D, terrain, import/export, layouts, offline, chat/tasks, MCP and house-numbering workflows recorded in the baseline inventory, run equivalent acceptance tests against the candidate. Plugin absence must not prevent core workflows.
### US5 (P1) — Operators can release and recover
Given a commit-pinned candidate, restore a staging backup, verify compatibility and migrate traffic only after all gates pass. Test rollback after writes using compatible schema; prove recovery rather than assume it.

## Requirements
- FR-001 Preserve Git history and explicitly reconcile all in-scope main/dev commits, live artifact commit, unmerged features and dirty work separately; no force push.
- FR-002 Provide contract parity for auth, organization, workspace, project, layers, geometry, processing, modules and integration APIs; inventory every route and supported behavior.
- FR-003 Enforce tenant/resource authorization on server and data paths including tile/cache/stream/job/export; fail closed.
- FR-004 Preserve explicit source/storage/display CRS, VN-2000 control points, precision, topology, Z/M policy and supported format semantics.
- FR-005 Preserve change attribution, revisions, idempotency, undo/conflict rules and transactional change history.
- FR-006 Bound viewport queries; serve authorized vector/raster tiles and paginated attributes; cache by scope/revision and invalidate on mutation/revocation.
- FR-007 Use durable ordered per-scope change cursors, replay, snapshot fallback and bounded backpressure; no silent lost edits.
- FR-008 Use bounded CPU/job concurrency and memory; cancellation, timeouts, recovery and observability.
- FR-009 Maintain supported CAD/3D/frontend/QGIS/offline/layout/terrain/MCP/chat behavior; classify any gaps as blockers or obtain explicit scope decision.
- FR-010 Keep business modules independent of core; migrate house numbering against stable SDK; untrusted plugins require sandbox, quotas and explicit capabilities.
- FR-011 Preserve data IDs and schema compatibility; additive migrations first, backup/restore, rehearsal and verified rollback; no destructive cutover.
- FR-012 Pin dependencies, audit licenses/security/third-party attribution and measure comparative performance before integrating optional engines.
- FR-013 Preserve API semantics for errors, pagination, auth/session/CSRF, realtime transport and connector versions; publish any versioned changes.
- FR-014 Freeze feature expansion during rewrite; keep Python reference until parity, security, performance and recovery evidence pass.

## Edge cases
Expired/revoked credentials, stale cache, cursor retention gap, duplicate events, reordered events, concurrent edit/delete, invalid geometry, CRS axis order, malformed files, giant geometries, slow clients, worker crash, partial migration, incompatible schema and insufficient RAM/disk.

## Success Criteria (proposed acceptance targets, not measured results)
- SC-001 100% inventoried in-scope supported contracts/workflows have passing parity evidence or explicit owner-approved exception.
- SC-002 Zero known cross-tenant leaks, lost/duplicated acknowledged edits, or unresolved critical/high security findings; control-point tolerances documented and met per CRS.
- SC-003 Same machine/database/data/workload: at 20 concurrent map clients, p95 viewport response <= 70% Python baseline, p95 feature API <= baseline, no increase in error rate; report p50/p95/p99 and RSS.
- SC-004 1M feature map loads bounded viewport tiles, never all features; 30k/100k/1M test sets; warm edit-to-visible p95 <= 250 ms on controlled LAN at 20 clients; no lost changes after reconnect.
- SC-005 On isolated 4 GiB profile, proposed aggregate app RSS <= 1 GiB excluding DB/OS and no sustained swap growth/OOM; tune only through documented benchmark evidence.
- SC-006 Staging restore and rollback pass with verified data integrity, acknowledged-write preservation and recorded recovery duration; no production cutover before owner approval.

## Engine-level scope and anti-omission gate
[Engine registry](engine-scope.md) defines 18 child slices, including symbology, labels, expressions, terrain, land assets, marketplace, AI/MCP, field/offline, CAD/3D and operational concerns. [Engine tasks](engine-tasks.md) expands the parent 23 tasks by 90 child tasks. [Static inventory](contracts/static-inventory.json) provides pinned tracked-file and handler candidates, NOT proof of runtime support. Before final scope freeze, every candidate, cross-cutting item, dynamic route, frontend command, registered algorithm, database migration, module manifest and connector contract must be classified and linked to an acceptance test. Missing or intentionally retired behavior requires explicit owner approval. Reconcile any changes after baseline and uncommitted Spec 063 separately. Desktop packaging and replacement GPU renderer remain optional discovery, not promised release requirements.
