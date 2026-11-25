# Tasks — Spec 059
Check off each task with reproducible evidence. See [the baseline and contract audit](evidence/baseline-and-contracts.md). P0 gates precede P1 scope.

## G0 — Baseline and CI (P0)
- [x] T001 Pin exact main/live SHA, artifact checksums, API/QGIS versions and test profile; record evidence.
- [x] T002 Trace R031–R047 open requirements to existing implementation/evidence; identify duplicates, gaps and deferred scope.
- [ ] T003 Inspect latest exact-main CI and seven historical visual failures; review actual diffs; fix regressions or approve intentional baselines.
- [x] T004 Establish reproducible synthetic fixtures and isolated PostGIS/OIDC profile, hardware/network budgets and latency harness.
- [x] T005 Record existing API/cache/cursor/module contract matrix, permissions and CRS boundaries; G0 review.

## G1 — Data engine (P0; T005)
- [ ] T006 Measure cold/warm viewport load, full-page requests, memory and p95 on 10k/100k/1M fixtures.
- [ ] T007 Audit spatial indexes, MVT native CRS and query plans; repair measured bottlenecks, no speculative cache infrastructure.
- [x] T008 Validate scoped cache keys, revision metadata, TTL, invalidation and restricted scopes.
- [x] T009 Implement/fix bounded delta apply and gap fallback; prove AC01.
- [ ] T010 Audit keyset pagination and server-side filter/search/sort; fix supported query parity.
- [x] T011 Verify CRS round-trip and independently sourced VN-2000 control points, fail ambiguous transforms.
- [x] T012 Test tenant/feature-level ACL, cache/tile leak denial and actual runtime RLS; G1 evidence.

## G2 — Realtime (P0; T012)
- [x] T013 Inventory web/QGIS changeset, cursor, receipt and binding contract parity.
- [ ] T014 Verify snapshot consistency during concurrent commits and alias-layer updates.
- [ ] T015 Verify 100 mutations across Web and real native Windows QGIS with actual network.
- [ ] T016 Fix stale cursor, reconnect, out-of-order, duplicate and ambiguous commit receipt behavior.
- [ ] T017 Verify conflict UI, durable draft recovery, crash/timeout handling and bounded retry.
- [ ] T018 Verify revoked sessions, access changes and two-client offline/online reconciliation without bypass.
- [ ] T019 Measure end-to-end event-to-visible p50/p95 and network bytes; prove AC02/03; G2 evidence.

## G3 — Core/module contract (P0; T012, T019)
- [x] T020 Audit module identity links, lifecycle, commands, selection, permissions and revision events; map to existing public interfaces.
- [ ] T021 Repair actual contract gaps without reverse dependencies or duplicate authoritative geometry.
- [ ] T022 Verify map-to-plugin and plugin-to-map selection/editing; confirm source rename/style change preserves links.
- [ ] T023 Verify delete/relink guards, approved-number invariants and conflict handling, including external QGIS edit.
- [ ] T024 Verify module disable/re-enable and Core-alone use; verify ACL and incompatible manifest denial.
- [ ] T025 Run end-to-end house-numbering reference workflow on synthetic/deidentified 30k+ fixture; private real-data acceptance only with authorization, R051/053 semantics unchanged.
- [x] T026 Document SDK/REST coverage and second independent reference-module validation; mark unsupported paths. G3 evidence.

## G4 — Convergence (P0; T026)
- [ ] T027 Run CAD/3D/core mode and data portability regression; preserve existing functionality.
- [ ] T028 Run full root validation, backend, QGIS package/desktop smoke on exact candidate; review actual screenshot differences.
- [ ] T029 Run independent security, license, dependency and tenant isolation checks; no sensitive fixtures in repository.
- [ ] T030 Rehearse isolated database backup/restore and release rollback; verify schema/manifest compatibility.
- [ ] T031 Publish exact-SHA evidence ledger, unresolved constraints, original task closure mapping and regression results; G4 review.

## G5 — Release acceptance (T031)
- [ ] T032 Obtain independent review and record fail-closed release gate, artifact checksums and user-observed acceptance.
- [ ] T033 Obtain explicit promotion approval; only then merge/deploy per release policy and verify frontend/backend/package/health and rollback plan.
- [ ] T034 Verify post-promotion read-only smoke and record remaining backlog; no silent legacy removal.
