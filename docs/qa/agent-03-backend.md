# Agent 03 — Backend Contracts, DB Migrations & Plugin Boundary Audit (P0/C)

**Baseline audited:** `main@fc8b8de` · **Date:** 2026-11-23 · **DB writes:** None (static + read-only verification).

## 1. Core vs. Business Plugin Boundary (`VERIFIED_FROM_CODE`)
- Official vertical modules (e.g. `modules/house_numbering/`) are discovered dynamically at startup via `discover_official_module_runtimes` in `apps/api/openvietgis_api/main.py` and loaded in frontend via `import.meta.glob("../../../modules/*/manifest.json")`.
- `tools/check_architecture.py` -> **PASS** (`Architecture boundaries: OK`).
- `tools/check_scope.py` -> **PASS** (`Accepted runtime scope and MapEngine boundary: OK`; `DeveloperPortal.tsx` hardcoded `/modules/house-numbering/` path removed in `a58db74`).
- `tools/check_contracts.py` -> **PASS** (`Spec 002, 003, 004 and 005 contracts: OK`).
- Static quality gates: `ruff check apps/api tools tests modules` = **0 errors**, `mypy apps/api/openvietgis_api` = **0 errors across 185 source files**.

## 2. Alembic Migration Graph Audit (`VERIFIED_FROM_CODE`)
- Command: `uv run --project apps/api alembic -c apps/api/alembic.ini heads`
- Result: **Single linear head** `0039_r027_connector_bindings (head)` across **48** migration scripts under `apps/api/migrations/versions/`.
- **No multiple-head conflicts.** Note: deploying `main@fc8b8de` to production requires running `alembic upgrade head` to apply `0039_r027_connector_bindings` before enabling R027 binding routes.

## 3. Tenant, Auth & GIS ACL Isolation (`VERIFIED_FROM_CODE` + `NEEDS_RUNTIME_TEST`)
- `VERIFIED_FROM_CODE`: All project-scoped routers in `apps/api/openvietgis_api/main.py` attach `dependencies=[Depends(project_access_guard)]` (including `integration_hub`, `integration_bindings`, `v8i_bridge`, `spatial`, `editing`, `transfers`, `cad`, `raster`).
- `VERIFIED_FROM_CODE`: `integration_bindings.py` enforces `actor_type == "human"`, `Permission.PROJECT_MANAGE`, and rejects scoped/restricted project actors (`403`). Default write/delete flags remain `False`.
- `VERIFIED_FROM_CODE`: R026 integration tests (`test_integration_hub_rls_acl.py`) passed 5/5 against live PostGIS 18.7/3.7.0.
- `NEEDS_RUNTIME_TEST`: Run `test_r027_bindings.py`, `test_r027_manual_workspace.py`, and `test_r025_v8i_capabilities.py` against the isolated CI PostGIS test database on the merged `main` SHA.
