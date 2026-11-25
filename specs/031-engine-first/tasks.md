# Tasks — Engine-first R031
All tasks remain unchecked until reproducible evidence is attached. [P] = parallel after prerequisite; dependencies explicit.

## Phase 0 — Inventory / G0
- [ ] T001 Pin main/release artifacts and identify actual live build separately (read-only).
- [ ] T002 [P] Enumerate registered commands, UI actions, runtime handlers, SDK exports and existing tests; reconcile unmapped entries.
- [ ] T003 [P] Audit runtime dependencies, lockfiles, WASM/vendor artifacts, license notices and upstream pins.
- [ ] T004 [P] Reproduce GIS loading/reload and QGIS sync problems; instrument request counts and delta lag.
- [ ] T005 [P] Capture synthetic GIS/CAD/3D fixtures, baseline latency/memory/accuracy and independent reference.
- [ ] T006 Review complete Build/Reuse/Replace/Remove/Defer matrix; G0 (T001–005).

## Phase 1 — Contracts / G1
- [ ] T007 Reuse current command/capability contracts; document concrete adapter seam and error mapping (T006).
- [ ] T008 Implement only necessary flag/registry/lifecycle hooks, no data migration (T007).
- [ ] T009 [P] Contract tests: ACL, revision, idempotency, cancellation, dirty drafts, mode failure (T007).
- [ ] T010 Gate G1; verify legacy parity (T008–009).

## Phase 2 — GIS / G2
- [ ] T011 [P] Compare Terra Draw with existing editor; exact pin/license and fixture evidence (T010).
- [ ] T012 [P] Inspect existing PostGIS algorithms; evaluate GeoLibre-WASM API and selected bounded algorithms (T010).
- [ ] T013 [P] Prototype editing adapter behind flag; snapping/topology/undo/revision checks (T011).
- [ ] T014 [P] Prototype processing worker only if T012 proves benefit; provenance and cancel/limits (T012).
- [ ] T015 G2 independent parity, performance and real-browser acceptance (T013–014).

## Phase 3 — Data / G3
- [ ] T016 [P] Inventory GDAL/OGR/PROJ runtime and actual import/export path (T006).
- [ ] T017 [P] Compare isolated QGIS Processing only for unmet approved workflows (T006).
- [ ] T018 Consolidate duplicate paths if verified; CRS/control point and format roundtrip (T016–017).
- [ ] T019 G3 reference comparisons, loss disclosure and permissions (T018).

## Phase 4 — CAD / G4
- [ ] T020 [P] Audit existing OpenCAD relay artifact, build pin, upstream license and deployed capability (T006).
- [ ] T021 Repair proven build/runtime issues; preserve dedicated-origin scope, dirty/save/retry contract (T020).
- [ ] T022 G4 actual supported native open/edit/save/reopen, deny/read-only and corruption fixtures (T021).

## Phase 5 — 3D / G5
- [ ] T023 [P] Benchmark custom renderer and pinned CesiumJS candidate, standalone assets/no mandatory cloud (T006).
- [ ] T024 Integrate selected engine if better; explicit georeference, authorized asset resolver, lazy load/dispose (T010,T023).
- [ ] T025 G5 model/terrain/tiles correctness, picking, GPU and failure regression (T024).

## Phase 6 — Realtime / G6 (P1)
- [ ] T026 [P] Trace existing bootstrap, cache keys, revision/cursor, polling and event transport (T004,T006).
- [ ] T027 Implement measured fixes only: bounded viewport/bootstrap, scoped cache/deltas, avoid full reload (T026).
- [ ] T028 Verify real QGIS/Web two-client reconnect, conflict, revoke, offline replay, 100 changes and no duplicates (T027).
- [ ] T029 G6 independent latency/byte counts and Windows QGIS validation (T028).

## Phase 7 — convergence / G7
- [ ] T030 Integrate accepted adapters on single candidate; run all R030 tracks and actual build/test suite (G1–G6).
- [ ] T031 Independent license/security/tenant review, SBOM and dependency notices; rollback drill (T030).
- [ ] T032 Remove proven redundant code in separate reviewed commits, rerun regression and migration compatibility (T031).
- [ ] T033 Exact final SHA, independent human acceptance, release evidence and approval; no production promotion otherwise (T032).
