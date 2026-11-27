# Tasks: Rust-native platform
All implementation tasks unchecked. IDs map requirements and success criteria.

## Setup & baseline (blocking)
- [ ] T001 [FR-001] Record refs/live artifact/schema; reconcile committed dev/main, inventory pending work and create verified rollback refs in evidence/baseline.md.
- [ ] T002 [FR-002 FR-009 FR-013 SC-001] Extract route/workflow parity matrix into contracts/inventory.json from legacy server, frontend, SDK and connectors.
- [ ] T003 [FR-012 SC-003 SC-005] Pin baseline benchmark data/workload and capture CPU/RSS/latency/error baselines in tools/benchmark/rust-platform/.
- [ ] T004 [FR-011 SC-006] Verify resource capacity, isolated staging backup/restore and rollback rehearsal plan in infra/rust-platform/.
- [ ] T005 [FR-012] Review ADR-015, versions, licenses/NOTICE, threat model and optional engine evidence before adoption.
## US1 foundation and spatial parity
- [ ] T006 [FR-008 FR-013] Initialize Cargo workspace/toolchain, CI, health/tracing/error contracts in apps/api-rust/.
- [ ] T007 [FR-003 FR-013 SC-002] Implement OIDC/session/CSRF, tenant-scoped SQLx/RLS transactions; real negative tests in tests/rust-parity/.
- [ ] T008 [FR-002 FR-004] Implement project/layer/features and spatial kernels in crates/ovg-core/, crates/ovg-storage/, crates/ovg-spatial/.
- [ ] T009 [FR-005 FR-011] Implement revision, conflict, idempotency and audit transaction semantics; golden edit/export tests.
- [ ] T010 [FR-004 SC-002] Verify VN-2000/CRS control points, invalid geometry, supported import/export and precision; differential fixtures.
## US2 large maps
- [ ] T011 [FR-006 FR-003] Add Martin/authorized tile gateway, bounded filters/attributes and scoped cache invalidation.
- [ ] T012 [FR-006 FR-008 SC-003 SC-004 SC-005] Integrate viewport streaming with canonical web client; benchmark 30k/100k/1M and backpressure.
## US3 sync
- [ ] T013 [FR-007 FR-005 FR-013] Implement durable outbox/cursor/replay, SSE compatibility and optional WebSocket in crates/ovg-sync/.
- [ ] T014 [FR-007 FR-003 SC-004] QGIS and web reconnect, revision conflicts, revoke, duplicates, restart and slow consumer tests.
## US4 complete workflows
- [ ] T015 [FR-008 FR-009] Migrate bounded processing/jobs and test recovery in crates/ovg-processing/.
- [ ] T016 [FR-010 FR-009] Migrate house numbering behind module SDK; preserve workflow and isolation.
- [ ] T017 [FR-009 FR-013] CAD/3D/layout/terrain/offline/chat/MCP integration and browser regressions; resolve inventory gaps.
- [ ] T018 [FR-012 FR-010] Evaluate optional GeoArrow/SedonaDB/WASM/Wasmtime/wgpu or RustGIS modules separately; only adopt demonstrated benefit.
## US5 quality and release
- [ ] T019 [FR-003 FR-012 SC-002] Security, license, dependency and independent code review; all critical/high issues resolved.
- [ ] T020 [FR-008 FR-012 SC-003 SC-004 SC-005] Run comparative benchmark, load/soak, memory and failure recovery; record raw evidence.
- [ ] T021 [FR-011 SC-006] Staging migration/rollback rehearsal preserving acknowledged writes; verify hashes and recovery.
- [ ] T022 [FR-014 SC-001 SC-006] Convergence report on exact SHA, obtain owner approval before production cutover.
- [ ] T023 [FR-014] Only after successful observation and explicit approval, retire legacy backend; preserve rollback artifacts.

## Child engine execution
[Engine tasks](engine-tasks.md) contains 90 additional unchecked tasks (18 slices × 5); child tasks refine rather than replace parent gates. T002 must close all static inventory triage and enumerate dynamic registrations, frontend commands, algorithms and runtime routes before scope freeze.
