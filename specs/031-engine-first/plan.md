# Implementation plan — R031
## Constitution check
Pass in design: platform-first, open data, tenant isolation, explicit CRS, revision, capability-driven, engine-agnostic, workflow-first. No constitution change needed. Accepted ADRs remain binding; engine choices are proposed until evidence and review.

## Existing baseline
Canonical frontend apps/web-next; apps/web legacy until parity; apps/api FastAPI; packages/spatial-workspace owns mode lifecycle. Existing client SDK and command contracts are retained. See [research](research.md) for actual inspected source. R022 owns shared workspace; R027/R029 own sync; R030 owns unified release.

## Phases and quality gates
0. Pin clean main and release manifest, inventory source, commands, lockfiles, vendor assets and actual builds; baseline fixed synthetic datasets; reproduce user-reported reload/slow sync and engine failures. Gate G0: complete inventory and reproducible baseline.
1. Reuse existing capability/command boundaries; minimal adapter contract and registry only where proven necessary; no migrations. Gate G1: denied/stale/unsupported operations, scope changes, cancel/dispose, feature flags and legacy regression tests.
2. P1 GIS editing: Terra Draw or alternative compatible with MapLibre; test geometry IDs, snapping/topology requirements and undo via existing ChangeSets. P1 processing: PostGIS first, evaluate browser WASM (GeoLibre candidate) only for selected bounded algorithms; worker isolation, time/memory budgets; compare independent reference fixtures. Gate G2: correctness, performance, UX and failure tests.
3. Import/CRS: inventory existing GDAL/OGR and PROJ deployment and consolidate only if duplication verified; evaluate QGIS Processing worker only if needed, isolated queue, no raw shell or arbitrary algorithm/plugin. Gate G3: format matrix, CRS control points and roundtrip.
4. CAD: audit existing OpenCAD relay and build pipeline; test version-pinned self-hosted artifact and compatibility; do not replace a working adapter merely for style. Gate G4: real open/edit/save/reopen, permissions, dirty/retry, format and license checks.
5. 3D: isolated CesiumJS or alternative PoC for actual project assets; compare against custom subset and keep existing safety policies. Gate G5: georeferencing, asset ACL, viewport streaming, GPU/memory budget, mode transitions.
6. Realtime: parallel P1 track after G0; instrument current data path, bounded bootstrap, revision cache, delta delivery, offline queue and revocation. Gate G6: QGIS/Web/AI authorized workflows, no ordinary full reload, duplicate/out-of-order/reconnect tests.
7. Integration: independent review, full exact-SHA CI, real browser/PostGIS, Windows QGIS, license and security scan, rollback drill; retire redundant paths only after verified parity. Gate G7: R030 release evidence; no promotion without recorded approval.

## Execution model
One coordinator owns shared contracts and dependency pins. Isolated worktrees per stream (inventory, editing, processing/import, CAD, 3D, sync, QA). Parallel research can start after G0 baseline is pinned; implementation after relevant contract review. Integrate small independently tested commits, rebase against clean main, run exact candidate tests. Unproven candidates remain off. No uncontrolled repo-wide formatting, no vendor source dump.
