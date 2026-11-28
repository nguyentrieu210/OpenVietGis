# Implementation plan
Keep current technology: React/Vite/pnpm, Rust Axum/Tokio/SQLx, PostgreSQL/PostGIS, MapLibre/Cesium/OpenCAD WASM, existing QGIS integration. Do not introduce Redis/CDN/new crate automatically.
W1 audit + baseline: pin verified main and read 062/064/065, inventory and reproduce existing CI; establish device/network/data profiles.
W2 cleanup: separate worktree, map contracts/consumers, move files with imports and tests, retire only approved replacements; update docs/scripts/workspace lockfiles.
W3 shell: route/engine ownership, critical chunk budget, scoped bootstrap, session and dirty recovery.
W4 data/cache: typed cache keys, authorization checks, revision/outbox invalidation, SW/static versioning, tests and telemetry.
W5 backend/data/rendering: profile API/SQL and GIS/CAD/3D; apply evidence-driven optimizations, differential geometry and load tests.
W6 sync/integration: bounded delta delivery, MCP/QGIS and offline correctness.
W7 operations: build-once, digest deploy, path-filter CI, resource retention, staging and rollback.
W8 convergence: review, security, repeat benchmarks, evidence, merge only after checks, authorized release verification.
Parallelize after shared contracts: UI/engine, API/DB, sync, infrastructure; one owner per overlapping file. 065 is active; reconcile its changes first and do not overwrite its branch. Use small logical commits and no redundant CI on transient commits; final exact-commit gate mandatory.
Architecture: see architecture.md. Acceptance: spec SC-01..07, budgets, checklist. Tasks are pending, not implementation claims.
