# Tasks — 066
Implementation in progress; only fully verified tasks checked. See evidence/implementation-01.md evidence/implementation-02.md evidence/implementation-03.md evidence/implementation-04.md evidence/implementation-05.md evidence/implementation-06.md evidence/implementation-07.md evidence/implementation-08.md evidence/implementation-09.md and evidence/implementation-10.md evidence/implementation-11.md and evidence/implementation-12.md and evidence/implementation-13.md and evidence/implementation-14.md and evidence/implementation-15.md and evidence/implementation-16.md for partial work. W1 precedes W2–W7. W2 retirement depends on 062/064 parity/ADR acceptance; W3/W4 share cache/session contract; W5/W6 depend on W4 revision contract; W7 may start in parallel after W1; W8 depends on all mandatory work. P0 = hard gate, P1 = optimization. Each task requires exact file list (expand directory references during audit), commit, tests and evidence. No marks until verification.

## W1 audit
- [x] T001 [P0] Reconcile main and active 065; record accepted ADRs and contract dependency matrix — paths: `CLAUDE.md; ROADMAP.md; docs/adr; specs/062*,064*,065*`. Acceptance: No dirty branch touched; pinned SHA + prerequisites.
- [x] T002 [P0] Inventory all tracked paths, sizes, imports, Cargo/pnpm dependency graph — paths: `apps; crates; packages; modules; integrations`. Acceptance: Machine-readable inventory with consumers and owners.
- [x] T003 [P0] Map API routes, tool schemas, runtime profiles, migrations and tests — paths: `apps/api-rust/src/routes.rs; apps/api; apps/mcp-rust; apps/mcp`. Acceptance: Every active contract mapped; unknowns explicit.
- [ ] T004 [P0] Inventory dev/build/CI scripts, required checks, duplicate jobs — paths: `package.json; pnpm-workspace.yaml; Cargo.toml; .github/workflows`. Acceptance: Complete command/status matrix.
- [x] T005 [P0] Measure cold/warm browser, API, GIS/CAD/3D, resources/build baseline — paths: `tools/benchmark; tests/e17; tests/rust-parity`. Acceptance: Raw baseline with profile and SHA.
- [x] T006 [P0] Read-only runtime/image/worktree/cache inventory and rollback map — paths: `infra/prod; infra/rust-platform; VPS /opt`. Acceptance: Active ownership and retention recorded.

## W2 cleanup
- [ ] T007 [P0] Finish 062 parity; retire old SPA only after offline/route coverage — paths: `apps/web; apps/web-next; pnpm-workspace.yaml`. Acceptance: Canonical build independent of old app; route matrix PASS.
- [ ] T008 [P0] Complete 064 route/migration/ops parity and ADR decision before retirement — paths: `apps/api; apps/api-rust; docs/adr/ADR-015-rust-platform.md`. Acceptance: All contracts and independent oracle evidence retained.
- [ ] T009 [P0] Converge MCP implementations after protocol and security parity — paths: `apps/mcp; apps/mcp-rust; crates/ovg-mcp-policy`. Acceptance: Tool schema/OAuth/client compatibility PASS.
- [ ] T010 [P1] Repair stale commands, lockfiles, workspace members after gates — paths: `package.json; Cargo.toml; pnpm-lock.yaml; pnpm-workspace.yaml`. Acceptance: Clean checkout bootstrap/build/test reproducible.
- [ ] T011 [P1] Organize domain folders incrementally, remove proven duplicates — paths: `apps/web-next/src; crates; packages`. Acceptance: Architecture/import guards and affected tests PASS.
- [ ] T012 [P1] Retain independent checks, archive obsolete docs with link map — paths: `tools; tests; docs; specs`. Acceptance: No broken links or lost evidence.
- [ ] T013 [P1] Remove committed generated artifacts only if reproducible/licensed — paths: `.gitignore; .dockerignore; apps/web-next/public`. Acceptance: Clean build output and correct ignores.

## W3 startup
- [x] T014 [P0] Route-first bootstrap and lazy auth/marketplace/QGIS paths — paths: `apps/web-next/src/main.tsx; route-compat.ts; workbench-registry.ts`. Acceptance: F5 each deep link initializes only target.
- [x] T015 [P0] Split shell and engine mounts, preserve 065 UX — paths: `apps/web-next/src/LiveApp.tsx; SpatialWorkspace.tsx; map-surface.ts`. Acceptance: No eager other engine requests; screenshot/route tests.
- [x] T016 [P0] Scope bootstrap and protected-query dedup, cancellation, session epochs — paths: `apps/web-next/src/workbenches/app; SessionBoundary.tsx`. Acceptance: No stale cross-project response; request counts recorded.
- [ ] T017 [P0] Restore CAD route and dirty state accurately — paths: `apps/web-next/src/CadTool.tsx; LayoutDesigner.tsx`. Acceptance: Untouched document no warning; changed draft recovered.
- [x] T018 [P1] Analyze bundles, split optional workbench/styles/assets, prefetch selectively — paths: `apps/web-next/vite.config.ts; package.json; style.css`. Acceptance: Measured compressed JS and waterfall improvement.
- [x] T019 [P1] Lifecycle cleanup of listeners, workers, map and render loops — paths: `apps/web-next/src; packages/spatial-workspace`. Acceptance: Repeated module switch soak: no unbounded memory growth.

## W4 cache
- [x] T020 [P0] Unify cache classification and scoped keys — paths: `SpatialCache.ts; SpatialCacheStorage.ts; protected-query.ts`. Acceptance: Two-tenant, revocation and logout tests PASS.
- [x] T021 [P0] Integrate revision invalidation and gap recovery — paths: `SpatialIncremental.ts; SpatialSyncCore.ts; crates/ovg-sync`. Acceptance: Mutation visible without full layer reload.
- [x] T022 [P0] Version assets, safe SW migration, public-only edge policy — paths: `apps/web-next/src/main.tsx; offline shell service worker; infra/prod/nginx.conf`. Acceptance: No protected cache entry; offline draft preservation.
- [ ] T023 [P1] Add bounded cache/singleflight only for profiled paths — paths: `apps/api-rust/src/state.rs; map_render.rs; crates/ovg-storage`. Acceptance: Memory cap, stampede, auth-before-hit tests.
- [x] T024 [P1] Cache hit/miss, bytes, invalidation and startup metrics — paths: `apps/api-rust/src/metrics.rs; apps/web-next/src/SpatialPerformance.ts`. Acceptance: Scoped aggregate telemetry without sensitive data.
- [x] T025 [P1] Verify cache headers/ETag/304, provider restrictions and versioning — paths: `infra/prod; CDN configuration inventory`. Acceptance: Public hash immutable; private no-store as classified.

## W5 engines
- [ ] T026 [P0] Profile allocations, blocking Tokio work and bounded Rayon/jobs — paths: `apps/api-rust/src; crates/ovg-core; crates/ovg-processing`. Acceptance: Comparable benchmark and no correctness change.
- [x] T027 [P0] EXPLAIN with representative PostGIS data, index/pool/query optimization — paths: `crates/ovg-storage; apps/api-rust/src/queries.rs; catalog.rs`. Acceptance: Plans and latency evidence; no unsafe index removal.
- [ ] T028 [P1] Batch/paginate/stream suitable payloads and cancellation — paths: `apps/api-rust/src/map_render.rs; tilesets.rs; transfers`. Acceptance: Bounded payload and concurrency; contract tests.
- [x] T029 [P0] Viewport BBOX, LOD, incremental render and virtual table — paths: `apps/web-next/src/ProfessionalTiles.ts; SpatialWorkspace.tsx; QgisWorkspace.tsx`. Acceptance: 10k/100k/1m tests; source geometry unchanged.
- [ ] T030 [P1] CAD selection/snap/culling/undo/autosave profile — paths: `apps/web-next/src/CadTool.tsx; native-dxf.ts; packages/spatial-workspace`. Acceptance: CAD accuracy, undo/redo and performance evidence.
- [x] T031 [P1] Lazy Cesium, LOD and texture/GPU lifecycle, optimize copy — paths: `apps/web-next/src/Spatial3DSettings.tsx; TilesetPanel.tsx; vite.config.ts`. Acceptance: No 3D initialization outside 3D; measured frame/bytes.
- [x] T032 [P1] Bound workers and cancel stale geometry calculations — paths: `apps/web-next/src/SpatialCacheWorker.ts; terrain-flow.worker.ts`. Acceptance: No unbounded queue; stale result ignored.
- [x] T033 [P0] Differential CRS/geometry/export and security tests — paths: `tests/rust-parity; tests/architecture; specs/064*/evidence`. Acceptance: No coordinate/data regression.

## W6 sync
- [x] T034 [P0] Ordered delta/resume/dedup/backpressure — paths: `crates/ovg-sync; apps/api-rust/src/managed_sync.rs; presence.rs`. Acceptance: Reconnect, gaps, concurrent edits, persistence PASS.
- [x] T035 [P0] Scope subscriptions and preserve offline outbox — paths: `apps/web-next/src/SpatialSyncCore.ts; LivePresence.tsx; offline-store.ts`. Acceptance: No full reload on normal reconnect; draft preserved.
- [ ] T036 [P1] Batch/cancel bounded tool results and QGIS sync — paths: `apps/mcp-rust; integrations/qgis_plugin; packages/client-sdk`. Acceptance: Real protocol tests, no schema break.
- [x] T037 [P0] Revocation and cache invalidation across sessions — paths: `apps/api-rust/src/tenancy.rs; middleware.rs; apps/web-next/src/workbenches/app`. Acceptance: No unauthorized stale response across tabs.

## W7 ops
- [ ] T038 [P1] Optimize multi-stage image and BuildKit cache — paths: `infra/prod/Dockerfile.web; infra/rust-platform/Dockerfile.api-rust; .dockerignore`. Acceptance: Size/build comparison; SBOM and smoke PASS.
- [ ] T039 [P0] Map then consolidate path filters/concurrency/caches and required checks — paths: `.github/workflows/ci.yml; rust-platform.yml; other workflows`. Acceptance: No lost quality gate; exact final candidate verified.
- [ ] T040 [P0] Build once on GitHub, publish digest and consume on VPS — paths: `infra/prod/docker-compose.prod.yml; infra/rust-platform; deployment scripts`. Acceptance: Same digest staging/deploy; no repeated build.
- [x] T041 [P1] Plan safe cache/log/image/worktree retention, dry-run — paths: `infra/prod; VPS inventory`. Acceptance: Active releases, volumes, backups protected; no destructive action without approval.
- [ ] T042 [P0] Rehearse rollback, health and database compatibility in isolation — paths: `infra/prod; tools/backup_restore.py; rollback-plan.md`. Acceptance: Restore and rollback evidence on staged candidate.
- [ ] T043 [P1] Track p95/p99, memory, errors, cache hit, build/deploy — paths: `apps/api-rust/src/metrics.rs; infra/prod`. Acceptance: Dashboard/structured metrics with privacy checks.

## W8 convergence
- [ ] T044 [P0] Repeat exact baseline profiles and load/soak — paths: `tests; tools/benchmark; performance-budget.md`. Acceptance: Raw results, targets or reviewed exceptions.
- [ ] T045 [P0] Run final route/tenant/CRS/offline/MCP/CAD regressions — paths: `tests; checklists/requirements.md; contracts`. Acceptance: No P0/P1 regression or data loss.
- [ ] T046 [P0] Update architecture/ADR only with review; attach task evidence — paths: `docs/adr; ROADMAP.md; spec.md; cleanup-manifest.md`. Acceptance: No unresolved deletion consumer; no false completion.
- [ ] T047 [P0] Independent code/security review; final CI; fix defects — paths: `.github/workflows; exact candidate`. Acceptance: Review findings addressed and required checks green.
- [ ] T048 [P0] Merge and authorized digest release; verify live health or record blocker — paths: `infra/prod; rollback-plan.md`. Acceptance: Exact digest and post-release smoke; rollback if severe failure.
