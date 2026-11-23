# Agent 05 — Automated QA & Release Gate Audit (P0/E)

**Baseline audited:** `main@fc8b8de` · **Date:** 2026-11-23

## 1. CI Workflows & Observed Gate Status
| Workflow / Gate | Command | Observed Result on `main@fc8b8de` |
|---|---|---|
| Python Lint | `ruff check apps/api tools tests modules` | **PASS** (0 errors; fixed from 835 via PR #16) |
| Python Types | `mypy apps/api/openvietgis_api` | **PASS** (0 issues across 185 source files) |
| Scope & Renderer Boundary | `python tools/check_scope.py` | **PASS** (0 violations; fixed from 27 via `a58db74`) |
| Architecture & Contracts | `check_architecture.py`, `check_contracts.py`, `check_docs.py` | **PASS** |
| Unit / Contract / Tools Tests | `pytest apps/api/tests/unit apps/api/tests/contract tools/tests tests/architecture` | **PASS** (100%) |
| R025 V8i Bridge Unit Tests | `pytest integrations/microstation-v8i-bridge/tests` | **PASS** (100%) |
| R026/R027 QGIS Plugin Package | `pytest tools/tests/test_package_qgis_plugin.py` | **PASS** (100%) |
| Canonical Frontend Typecheck & Build | `pnpm --filter @openvietgis/web-next exec tsc --noEmit` + `vite build` | **PASS** |
| Spatial Workspace 3D | `pnpm --filter @openvietgis/spatial-workspace typecheck` + `test` | **PASS** |
| Integration Hub UI/SDK Smoke | `node apps/web-next/integration-hub-smoke.mjs` | **PASS** |
| Full Playwright Browser E2E (Keycloak + PostGIS) | `pnpm validate:e2e` | **NOT_VERIFIED locally** (executes in GitHub Actions `validate` job) |
| Windows COM V8i Live Probe | Requires licensed MicroStation V8i on Windows | **NOT_VERIFIED** (offline contract tests PASS) |

## 2. Community QA Synthetic Fixtures (Issue #7)
- Added reproducible synthetic fixture generator (`tools/generate_community_qa_fixtures.py`) and verified fixture pack under `fixtures/community-qa/` covering Point, LineString, Polygon with hole, intentional invalid geometry (negative test), CRS EPSG:4326 & EPSG:3857, GeoJSON, GeoPackage (`.gpkg`), Shapefile (`.zip`), and 2D DXF (`.dxf`), with public `manifest.json` and `SHA256SUMS.txt` (`contains_private_data: false`).
- Guarded by `tools/tests/test_community_qa_fixtures.py` to prevent fixture drift or checksum mismatch.

## 3. Mapping to Community Acceptance Matrix (`docs/community/ACCEPTANCE_MATRIX.md`)
- **Round 1 (CQ-00..CQ-05, CQ-09):** Core login, layer catalog, CRS, import/export, editing, ACL — supported by automated backend/unit/contract suites + `fixtures/community-qa/`.
- **Round 2 (CQ-06..CQ-08, CQ-10..CQ-17):** Processing, Layout, Raster, CAD/DXF, 3D, Integration Hub, Plugins, AI — ready for independent community verification on the pinned `main` SHA.
