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
