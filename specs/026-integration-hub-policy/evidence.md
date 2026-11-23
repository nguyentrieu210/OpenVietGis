# R026 / Slice 1 & Slice 2 Foundation — Validation Evidence

Recorded 2026-11-23 on isolated branch `feature/026-integration-hub-policy` (base `main@5baade9`, reconciled cleanly against `origin/main@2373a53`).

## 1. Delivered Scope (Read-Only)

- **Architecture & Contract (`architect`):**
  - `specs/026-integration-hub-policy/qgis-postgis-architecture.md` — documents R002/R003/R005/R006/R010/R025 boundaries, QGIS/PostGIS read-only exchange channels, explicit CRS rules, and why direct internal PostGIS credentials are never exposed.
  - `specs/026-integration-hub-policy/contracts/integration-hub-api.yaml` — OpenAPI 3.1.0 contract for `GET .../integration-hub/catalog` and `GET .../integration-hub/adapters/{adapter_key}`.
- **Backend Domain & HTTP (`backend`):**
  - Pure domain catalog (`domains/integration_hub/catalog.py`) with 12 adapter families, fail-closed `ActionEvaluation`, and `default_write_policy: "deny"`.
  - Pure domain read-only exchange profiles (`domains/integration_hub/profiles.py`) for `qgis`, `postgis`, and `ogc_services` (`supports_direct_db_credentials: false`, `crs_policy: "explicit_only_never_inferred"`); `microstation_v8i` kept as `candidate_external_branch` (`connection_profile: null`), planned adapters return `planned_not_implemented` (`connection_profile: null`, `layers: []`).
  - HTTP router (`adapters/http/integration_hub.py`) exposing strictly 2 `GET` endpoints protected by `authenticate_request`, router-level `project_access_guard` (`SAFE_GET` only; `SAFE_POST` unchanged), `prepare_project_scope(Permission.PROJECT_READ)`, service principal `permission_ceiling`, and per-layer `gis_access` filtering.
  - Zero Alembic migrations, zero DB schema mutations, zero outbound HTTP/DB calls, zero credentials/secrets exposed, and zero edits to R010 `external-sources`.
- **Typed Client SDK & Frontend (`frontend`):**
  - `packages/client-sdk/src/integration-hub.ts` (re-exported via `index.ts`) + `packages/client-sdk/tests/integration-hub.test.ts` — strictly read-only GET helpers with `encodeURIComponent`.
  - `apps/web-next/src/IntegrationCatalog.tsx` + `integration-catalog.css` — read-only catalog & profile inspector inside Project Plugin modal (`LiveApp.tsx`) with honest Vietnamese status labels, explicit CRS rendering (`"Chưa xác định CRS"` when unverified), `scoped_access` / `default_write_policy` banners, and zero forms/mutation controls.
  - `apps/web-next/integration-hub-smoke.mjs` — offline SDK/UI contract & invariant check + optional Playwright loopback browser E2E (`OVG_TEST_URL`).
- **QA & Integration Tests (`qa`):**
  - Unit suites: `test_catalog.py` (7), `test_http_catalog.py` (11), `test_profiles.py` (12).
  - Real PostGIS integration suite authored: `apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py` (3 tests covering anonymous/cross-tenant RLS & 405 methods, service principal `permission_ceiling` & revocation, and `gis_access` scoped layer ACL & VN-2000 EPSG:5899 CRS preservation).
- **Independent Security Review & Rollback (`release`):**
  - `specs/026-integration-hub-policy/security-review.md` — adversarial code/contract review (0 Critical/High/Medium defects; 3 Low + 2 Informational residual items documented; 4 blocking gates).
  - `specs/026-integration-hub-policy/rollback-plan.md` — zero-schema-change rollback runbook (image rollback + git revert + post-rollback checklist + `origin/main` reconciliation).

---

## 2. Locally Verified Checks (Real Outputs — 2026-11-23)

| Check | Command | Observed Result |
| :--- | :--- | :--- |
| Main delta check | `git log --oneline 5baade9..origin/main` | `2373a53`, `db4784f` (2 docs-only commits touching `README.md`, `docs/product.md`) |
| Merge conflict dry check | `git merge-tree --write-tree origin/main HEAD` | Exit `0` (`1407a96a443e046f5b8315f26f86214d25b74e2f`), **0 conflicts** |
| Architecture boundaries | `uv run --project apps/api python tools/check_architecture.py` | `Architecture boundaries: OK` (exit `0`) |
| Documentation navigation | `uv run --project apps/api python tools/check_docs.py` | `Documentation navigation: OK` (exit `0`) |
| Platform contracts (R002–R005) | `uv run --project apps/api python tools/check_contracts.py` | `Spec 002, 003, 004 and 005 contracts: OK` (exit `0`) |
| Scope hygiene | `uv run --project apps/api python tools/check_scope.py` | Exit `1`: 27 pre-existing violations on `main@5baade9`, **0 new violations** in R026 files |
| OpenAPI 3.1 contract validation | `openapi_spec_validator` on `specs/026-integration-hub-policy/contracts/integration-hub-api.yaml` | `OPENAPI_OK` (exit `0`) |
| Python lint (R026 sources & tests) | `uv run --project apps/api ruff check ...` | `All checks passed!` (exit `0`) |
| Python typing (R026 domain & HTTP) | `uv run --project apps/api mypy apps/api/openvietgis_api/domains/integration_hub apps/api/openvietgis_api/adapters/http/integration_hub.py` | `Success: no issues found in 4 source files` (exit `0`) |
| Python unit tests | `uv run --project apps/api pytest apps/api/tests/unit/integration_hub apps/api/tests/unit/authorization -q` | **57 passed** (30 `integration_hub` + 27 `authorization`, exit `0`) |
| PostGIS integration test collection | `uv run --project apps/api pytest apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py --collect-only -q` | **3 tests collected** cleanly (execution requires live PostGIS) |
| Client SDK + UI offline smoke | `node apps/web-next/integration-hub-smoke.mjs` | `PASS typed SDK exports, URL encoding, and read-only GET contract`; `PASS UI read-only invariants, honest labels, and scope hygiene`; `SKIP live browser E2E (OVG_TEST_URL not set)` (exit `0`) |
| Zero DB migrations & R010 untouched | `git diff --name-only 5baade9 -- apps/api/migrations apps/api/openvietgis_api/domains/external_sources apps/api/openvietgis_api/adapters/http/external_sources.py` | Empty (zero changes) |

---

## 3. Gates Requiring Live PostGIS / Running Stack / CI (NOT YET PASSED)

- **GATE-01 (T006 — Live PostGIS RLS / Service Principal / `gis_access` Execution):** `apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py` has not been executed against a live PostgreSQL/PostGIS instance (Docker/PostGIS unavailable locally). Must pass via `pnpm validate:integration` in CI.
- **GATE-02 (T007b — Live Browser E2E & Full TS Workspace Validation):** Live Playwright E2E (`OVG_TEST_URL=http://127.0.0.1:... node apps/web-next/integration-hub-smoke.mjs`) and `pnpm validate:ts` (eslint, tsc, vitest `packages/client-sdk/tests/integration-hub.test.ts`, Vite production build) must pass in CI (`pnpm` unavailable in local shell).
- **GATE-03 (Branch Reconciliation & Full CI):** PR merge/rebase onto `origin/main@2373a53` with full CI green (`validate:py`, `validate:ts`, `validate:contracts`, `validate:arch`, `validate:docs`, `validate:integration`, `validate:e2e`).
- **GATE-04 (T009 — Staging / Production Release & Rollback Drill):** No staging or production deployment, API restart, frontend replacement, or live rollback drill has been executed.

## 4. Follow-On Capabilities Explicitly Out of Scope (Not Implemented)

- Connector runtime write/sync execution (`sync.push`, `data.import`, `connection.configure`, `sync.approve`), connector credential vault, device pairing, desktop plugins, or proprietary vendor CAD/GIS SDK binaries.

**Release decision: NOT PROMOTED (HOLD).** Ready for PR submission and CI execution; keep isolated from production until GATE-01 through GATE-04 are verified with live evidence.

---

## Slice 2 — Read-Only QGIS & PostGIS Export + QGIS Desktop Connector (2026-11-23)

- **Backend read-only export endpoint**:
  - Added `GET /api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export?format=geopackage|geojson|postgis_sql` (`Cache-Control: no-store`, `X-Content-SHA256`, `X-OpenVietGIS-CRS`, `X-OpenVietGIS-Content-Revision`, `X-OpenVietGIS-Schema-Revision`, `X-OpenVietGIS-Feature-Count`, audit event `integration_hub.layer_export`).
  - Added `apps/api/openvietgis_api/adapters/formats/postgis_sql.py` (transactional PostGIS SQL writer with explicit readable SRID, identifier quoting, SQL literal escaping, NUL stripping, finite WKT coordinates, zero internal DB credentials).
  - Reused existing OGC GeoPackage 1.3 writer (`write_geopackage`) and GeoJSON writer (`write_geojson`, enforcing EPSG:4326 without unverified reprojection).
  - Enforced `prepare_project_scope(Permission.PROJECT_READ)`, `gis_access` layer + feature-level `export` ACL, explicit readable CRS (`get_readable_crs_profile`), and 25,000-feature bound; zero mutations (`default_write_policy: "deny"`), zero DB migrations.
- **QGIS Desktop 3.28+ read-only plugin**:
  - Added `integrations/qgis_plugin/openvietgis_connector/` (`metadata.txt`, `__init__.py`, pure-Python `client.py`, PyQt5 `plugin.py`, bilingual `README.md`) with Bearer Service Principal auth, mandatory `X-Content-SHA256` verification, path-traversal filename sanitization, and `QgsVectorLayer.setReadOnly(True)`.
- **OpenAPI 3.1, Typed SDK & UI**:
  - Updated `specs/026-integration-hub-policy/contracts/integration-hub-api.yaml` (`OPENAPI_OK`).
  - Added `buildIntegrationLayerExportPath` and `downloadIntegrationLayerExport` in `packages/client-sdk/src/integration-hub.ts`.
  - Upgraded `apps/web-next/src/IntegrationCatalog.tsx` with read-only download actions (`.gpkg` / `.geojson` for QGIS; `.sql` / `.gpkg` for PostGIS) gated by `export_permitted`, still zero write/sync/credential controls.
- **Local Verification Results**:
  - `ruff check` (backend + plugin + tests): **PASS**
  - `mypy --strict` (backend + tests): **PASS**
  - `pytest` unit (`integration_hub` + `authorization`): **117 passed** (90 `integration_hub`, 27 `authorization`)
  - `pytest --collect-only` T006 PostGIS integration (`test_integration_hub_rls_acl.py` extended for `.gpkg`/`.sql` RLS & per-feature ACL): **3 tests collected** (live execution pending Docker/PostGIS)
  - `node apps/web-next/integration-hub-smoke.mjs`: **PASS**
  - `check_architecture.py` / `check_docs.py` / `check_contracts.py`: **PASS**; `check_scope.py`: **0 new violations**

**Release decision remains `HOLD — NOT PROMOTED`** pending live PostGIS T006 run, full `pnpm validate:ts` + Playwright E2E, and T009 staging rollback drill.

---

## Slice 3 — AutoCAD 2D DXF Export + Controlled Two-Step QGIS GeoPackage Sync (2026-11-23)

- **AutoCAD 2D DXF Read-Only Export (`autocad`)**:
  - Extended `GET .../integration-hub/adapters/{adapter_key}/layers/{layer_id}/export` with `format=dxf` (`application/dxf`, reusing `adapters/cad/dxf_codec.py::write_dxf_2d` R2010).
  - Enforces projected metre CRS (`EPSG:5899`, `EPSG:3857`) via `get_readable_crs_profile`, refuses geographic degrees (`4326`/`4756`) with `422`, enforces 1–10,000 feature bound and `gis_access` layer/feature ACL, discloses tabular attribute loss via `X-OpenVietGIS-Attribute-Loss: true`, and keeps DWG binaries & CAD writeback denied.
- **Controlled Two-Step QGIS GeoPackage Sync (`qgis` `sync.push`)**:
  - Added pure domain diff planner `apps/api/openvietgis_api/domains/integration_hub/sync_diff.py` (canonical `diff_sha256`, max 1,000 changed ops, zero auto-deletions — omitted server features are reported in `omitted_server_feature_ids`).
  - Added `POST .../adapters/qgis/layers/{layer_id}/sync-preview` (`200`, dry-run diff) and `POST .../adapters/qgis/layers/{layer_id}/sync-push` (`201`, committing exclusively via `SqlEditingRepository.commit_changeset` with `Permission.PROJECT_EDIT`, CSRF, exact `base_content_revision`/`base_schema_revision` pin, `expected_diff_sha256` match, `operation_id` idempotency rejecting `field-sync-*`, and `gis_access.verify_edit` per-feature ACL). Zero DB migrations.
- **OpenAPI 3.1, QGIS Desktop Plugin, SDK & UI**:
  - Updated `specs/026-integration-hub-policy/contracts/integration-hub-api.yaml` (`OPENAPI_OK`).
  - Added `preview_layer_geopackage_sync` & `push_layer_geopackage_sync` in `integrations/qgis_plugin/openvietgis_connector/` (`client.py`, `plugin.py`, `README.md`).
  - Added `previewQgisGpkgSync` & `pushQgisGpkgSync` in `packages/client-sdk/src/integration-hub.ts` and two-step UI + AutoCAD `.dxf` download in `apps/web-next/src/IntegrationCatalog.tsx`.
- **Local Verification Results**:
  - `ruff check` (all R026 backend + plugin + tests): **PASS**
  - `mypy --strict` (all R026 backend + tests): **PASS**
  - `pytest` unit (`integration_hub` + `authorization`): **154 passed** (127 `integration_hub`, 27 `authorization`)
  - `pytest --collect-only` T006 PostGIS integration (`test_integration_hub_rls_acl.py` with AutoCAD DXF + QGIS sync push & `gis_access.verify_edit`): **5 tests collected** (live execution pending Docker/PostGIS)
  - `node apps/web-next/integration-hub-smoke.mjs`: **PASS**
  - `check_architecture.py` / `check_docs.py` / `check_contracts.py`: **PASS**; `check_scope.py`: **0 new violations**

**Release decision remains `HOLD — NOT PROMOTED`** pending live PostGIS T006 run, full `pnpm validate:ts` + Playwright E2E, and T009 staging rollback drill.

---

## Live VPS Verification Record — Milion (`cdfa3dc` / `5caec1f`, 2026-11-23)

Executed in isolated worktree `/opt/openvietgis-integration-hub` against disposable loopback PostGIS container `ovg-r026-verify-pg` (`postgis/postgis@sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262`, PostgreSQL 18.7 / PostGIS 3.7.0, destroyed on exit; zero production services or live data touched):

- **T006 Live PostGIS Integration (`apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py`)**: **5 passed in 25.80s** (all 38 Core + 10 official module migrations applied cleanly under owner role; API runtime exercised strictly under `openvietgis_runtime` role):
  1. `test_integration_hub_rejects_anonymous_and_cross_tenant_rls` — **PASS**
  2. `test_integration_hub_service_principal_ceiling_and_revocation` — **PASS**
  3. `test_integration_hub_gis_scoped_layer_acl_and_crs_preservation` — **PASS**
  4. `test_integration_hub_autocad_dxf_export_and_crs_guard` — **PASS**
  5. `test_integration_hub_qgis_sync_preview_push_and_gis_acl` — **PASS** (editable manual layer R006 changeset commit `201`, omitted features preserved, stale revision `409`, `gis_access.verify_edit` `403`, and `reference_fixture` `403 SOURCE_READ_ONLY`)
- **Python Unit (`integration_hub` + `authorization`)**: **154 passed**
- **Node `v26.13.1` / `pnpm 12.21.0` Workspace Validation (`5caec1f`)**:
  - `@openvietgis/client-sdk`: `eslint --max-warnings=0` **PASS**, `tsc --noEmit` **PASS**, `vitest run` **16 files / 75 tests PASS** (incl. 11/11 in `tests/integration-hub.test.ts`)
  - `@openvietgis/web-next`: `tsc --noEmit` **PASS**, `vite build` (Vite 8.3.8) **PASS**
  - `node apps/web-next/integration-hub-smoke.mjs`: **PASS**

**Remaining before production promotion:** live Playwright browser E2E (`OVG_TEST_URL`), full main-branch CI convergence (pre-existing `main` ruff/scope backlog), and T009 staging rollback drill.
