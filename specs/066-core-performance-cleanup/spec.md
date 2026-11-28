# Feature Specification: 066 Core Architecture, Cleanup & Performance
Status: implementation in progress; slice 01 verified locally; no release certification. Date: 2026-11-27.
Parent roadmap: v1.0 (030), R001/R015 cross-cutting; dependencies R002–R014, 062 single frontend, 064 Rust platform, 065 QGIS UX (coordinate after merge).
Baseline source snapshot: 5bfc83826161acd83b98a925e85690cebde53ae1. This is a source inventory, not a production runtime certification.

## User stories
- US1 P0: visitor opens the existing approved entry experience quickly; an authenticated user opens or refreshes a GIS/CAD/3D deep link directly without mounting another engine, redundant bootstrap or forced re-login.
- US2 P0: operators pan, zoom, edit, select, inspect, save and switch projects with incremental updates, predictable resource use and no lost changes.
- US3 P0: maintainers use a single canonical production path, reproducible dev/test commands and a clear directory/module graph; verified superseded implementations are removed.
- US4 P0: teams obtain fresh authorized data after edits, revocation, reconnect and release changes without stale sensitive caches or full reload.
- US5 P1: operators build once, deploy verified immutable images, preserve data and roll back without rebuilding.
- US6 P1: administrators see comparable startup, API, rendering, resource and deployment measurements and regressions.

## Requirements
FR-01 inventory every tracked path, dependency, route, entrypoint, artifact, migration and runtime owner; record retain/move/merge/delete with evidence.
FR-02 converge 062 and 064, preserve 065 changes. Do not infer Python is redundant solely from Rust presence. Preserve QGIS Python plugin, native adapters, migrations and independent oracles until verified replacements and ADR acceptance.
FR-03 enforce existing monorepo and modular monolith, core cannot depend on domain modules, no speculative services/crates.
FR-04 one canonical SPA; direct route hydration; critical shell first, lazy non-active features/engine/Cesium/WASM; no forced landing policy change beyond approved 065 behavior.
FR-05 deduplicate scoped bootstrap, cancellation and query cache; no whole-page spinner for unrelated work; route/session/project epochs prevent races.
FR-06 restore authorized project, workbench, view and document; dirty state reflects actual edit, drafts are recoverable; no auto-discard on refresh or navigation.
FR-07 cache by classification, permission revision, data revision, CRS/style/provider version; bounded eviction and single-flight; no protected payload in shared CDN/SW; revoke/clear user caches on identity/permission change.
FR-08 viewport queries, tile/LOD, selective invalidation and delta updates, virtualized tables, bounded workers; preserve exact source geometry and CRS.
FR-09 CAD/3D loading, render lifecycle, undo, selection, snapping, autosave and layout precision; dispose inactive rendering loops and GPU resources.
FR-10 profile Rust, SQL/PostGIS, connection pools and background jobs; improve measured hotspots without weakening accuracy, tenant scope or audit.
FR-11 realtime ordered revisions, resume cursor, deduplication, gap recovery, backpressure and QGIS/MCP compatibility.
FR-12 immutable digest-pinned release, path-based CI without skipping required gates, no VPS rebuild of already verified image, safe cleanup of inactive resources only.
FR-13 explicit baseline and candidate evidence, repeatable benchmark and negative security tests; no unverified checkmarks.
FR-14 preserve public URLs, SDK/API contracts, OAuth, offline packages, export/import, migrations, tenant permissions, backups and current capability behavior.
FR-15 preserve approved product UX; guest route if already supported remains read-only and separate from tenant resources; no local GIS shadow project.
Non-goals: feature expansion, wholesale framework rewrites, destructive database cleanup, rewriting Git history, new microservices, unverified performance claims.

## Acceptance
SC-01 direct deep link/F5 mounts exactly its target engine, zero unrelated GIS/CAD/3D initialization; no full-document navigation within app.
SC-02 clean canonical build and complete route/contract coverage; deletion manifest has zero unresolved consumers; accepted ADR transition before retiring Python runtime.
SC-03 cache tests cover two tenants, user switch, logout, role revocation, mutation, reconnect and multi-tab; zero leakage or unauthorized 304/tiles.
SC-04 data integrity/CRS/permissions/offline drafts/undo/QGIS parity pass; no P0/P1 regression.
SC-05 benchmark protocol and agreed budgets in performance-budget.md; every measured metric reports baseline/candidate and pass/fail, no invented numbers.
SC-06 release verified on same digest in staging and production (only with authorized rollout), rollback rehearsal passes; no production build.
SC-07 all mandatory tasks evidenced on exact commit; deviations explicitly reviewed and tracked.
