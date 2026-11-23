# R026 Independent Security & Boundary Review

**Feature:** R026 — Integration Hub & Policy Gate (Slice 1 + Slice 2 Read-Only Profile Foundation)  
**Branch:** `feature/026-integration-hub-policy` (base `main@5baade9`, reconciled against `origin/main@2373a53`)  
**Review Date:** 2026-11-23  
**Reviewer Track:** Independent Security & Release Reviewer (T008)  
**Release Gate Decision:** **HOLD — NOT PROMOTED** (Zero Critical/High/Medium security defects in R026 code; merge & production promotion remain gated on live PostGIS integration tests, live browser E2E, and full CI).

---

## 1. Scope of Inspected Files

Every changed and newly created file on `feature/026-integration-hub-policy` was inspected directly:

- `apps/api/openvietgis_api/domains/integration_hub/__init__.py`
- `apps/api/openvietgis_api/domains/integration_hub/catalog.py`
- `apps/api/openvietgis_api/domains/integration_hub/profiles.py`
- `apps/api/openvietgis_api/adapters/http/integration_hub.py`
- `apps/api/openvietgis_api/adapters/http/gis_access_guard.py`
- `apps/api/openvietgis_api/main.py`
- `specs/026-integration-hub-policy/contracts/integration-hub-api.yaml`
- `specs/026-integration-hub-policy/qgis-postgis-architecture.md`
- `packages/client-sdk/src/index.ts`
- `packages/client-sdk/src/integration-hub.ts`
- `packages/client-sdk/tests/integration-hub.test.ts`
- `apps/web-next/src/IntegrationCatalog.tsx`
- `apps/web-next/src/integration-catalog.css`
- `apps/web-next/src/LiveApp.tsx`
- `apps/web-next/integration-hub-smoke.mjs`
- `apps/api/tests/unit/integration_hub/test_catalog.py`
- `apps/api/tests/unit/integration_hub/test_http_catalog.py`
- `apps/api/tests/unit/integration_hub/test_profiles.py`
- `apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py`

Supporting platform boundary files cross-checked during review: `adapters/persistence/scope_helper.py`, `adapters/persistence/authorization_repository.py`, `adapters/persistence/gis_access.py`, `adapters/http/gis_access.py`, `adapters/http/errors.py`, `domains/spatial/crs.py`, `migrations/versions/0006_spatial_catalog.py`, `migrations/versions/0023_gis_access.py`.

---

## 2. Security & Boundary Control Matrix (PASS)

| Control Area | Verdict | Evidence from Code Inspection |
| :--- | :--- | :--- |
| **Auth before adapter lookup (no pre-auth / cross-tenant enumeration)** | **PASS** | Both routes depend on `authenticate_request` and router-level `project_access_guard` (`main.py:114`). `read_integration_adapter_profile` invokes `prepare_project_scope(session, context, permission=Permission.PROJECT_READ)` (`integration_hub.py:166`) **before** `get_adapter(adapter_key)` (`integration_hub.py:167`). Anonymous callers receive `401`; unauthorized or cross-tenant callers fail identically for known vs unknown adapter keys. |
| **RLS + `prepare_project_scope(Permission.PROJECT_READ)` on both routes** | **PASS** *(code-verified; live PG pending GATE-01)* | Both `GET .../integration-hub/catalog` (`integration_hub.py:139`) and `GET .../integration-hub/adapters/{adapter_key}` (`integration_hub.py:166`) invoke `prepare_project_scope(..., permission=Permission.PROJECT_READ)`, binding actor/service RLS GUCs and verifying active org/project + `project.read`. `_ACTIVE_LAYERS_SQL` uses parameterized binds, active lifecycle filters, and `LIMIT 1000`. |
| **Service principal `permission_ceiling` enforcement** | **PASS** *(code-verified; live PG pending GATE-01)* | Both endpoints resolve effective permissions through `AuthorizationService(SqlEffectivePermissionProvider(session)).effective_permissions(context)`, which intersects active grants with `service_credentials.permission_ceiling` (`authorization_repository.py:219-221`). Read-only service credentials cannot elevate write/manage actions in policy preflights even if principal holds a `manager` grant. |
| **GIS scoped account (`gis_access`) isolation & guard hygiene** | **PASS** *(code-verified; live PG pending GATE-01)* | `gis_access_guard.py` adds **only** `/integration-hub/catalog` and `/integration-hub/adapters/{adapter_key}` to `SAFE_GET`; `SAFE_POST` is completely unchanged, and canonical `org.organizations -> workspace.projects` `FOR SHARE` lock ordering is preserved. `_load_accessible_layers` filters layers via `_source_allowed(..., "read")` (in addition to PG RESTRICTIVE RLS `layers_gis_acl` / `data_sources_gis_acl`) and gates `export_permitted` on `_source_allowed(..., "export") and crs_explicit`. |
| **Zero mutation surface** | **PASS** | `integration_hub.router` registers exactly 2 `GET` endpoints; `POST`/`PUT`/`PATCH`/`DELETE` return `405`. `@openvietgis/client-sdk` (`integration-hub.ts`) exports only 2 `GET` functions. `IntegrationCatalog.tsx` has zero `<form>`, zero password/submit inputs, zero mutation calls, and only a single read-only expand/collapse `<button type="button">`. |
| **Zero DB migrations / zero schema mutation** | **PASS** | `git diff --name-only 5baade9 -- apps/api/migrations` and `git status --short -- apps/api/migrations` are empty. Zero DDL or Alembic revisions added. |
| **Zero DB credentials, secrets, or SSRF** | **PASS** | All `ReadOnlyConnectionProfile` payloads set `supports_direct_db_credentials: False` and `default_write_policy: "deny"`. No hostnames, ports, usernames, passwords, DSNs, or tokens appear in code or responses. Zero outbound HTTP/socket/DB libraries are imported in `domains/integration_hub` or `adapters/http/integration_hub.py`. |
| **Explicit CRS preservation (no silent 4326 fallback)** | **PASS** *(see RESIDUAL-01)* | `profiles.py` enforces `crs_policy: "explicit_only_never_inferred"`. Missing `stored_crs_code` (`None`) or non-`ready` `validation_state` yields `crs_explicit: False` and `export_permitted: False`. `IntegrationCatalog.tsx` renders `"Chưa xác định CRS"` when `!layer.crs_explicit` and never coerces missing CRS to `EPSG:4326`. |
| **R010 (`external-sources`) boundary preserved** | **PASS** | `git diff 5baade9 -- apps/api/openvietgis_api/domains/external_sources apps/api/openvietgis_api/adapters/http/external_sources.py` is empty. `ogc_services` remains `lifecycle: "existing_read_only"`, `existing_api: "external-sources"`, `execution_status: "existing_read_only_api"`. R010 SSRF allowlists and read-only rules are untouched. |
| **R025 (`microstation_v8i`) boundary preserved** | **PASS** | `microstation_v8i` remains `lifecycle: "candidate"`, `mode: "windows_bridge"`, `execution_status: "candidate_external_branch"`, `connection_profile: null`, `layers: []`. Zero Windows COM/DCOM or Bentley binaries exist in cloud API or web assets. |
| **Frontend XSS, URL encoding & tenant-switch state isolation** | **PASS** | `IntegrationCatalog.tsx` renders all strings via React text nodes (no `dangerouslySetInnerHTML`), delegates exclusively to typed SDK helpers using `encodeURIComponent`, and clears cached catalog/profile state with a `scopeGeneration` guard whenever `org` or `projectId` changes. |

---

## 3. Residual Risks & Non-Blocking Findings

No Critical, High, or Medium vulnerabilities were identified. The following Low and Informational items are documented for hardening prior to or following merge:

### RESIDUAL-01 (Low — Defense-in-Depth CRS Readability Check in Domain Layer)
- **Location:** `apps/api/openvietgis_api/domains/integration_hub/profiles.py:155-157`, `apps/api/openvietgis_api/adapters/http/integration_hub.py:103`
- **Observation:** `crs_explicit` is computed as `stored_crs_code is not None and validation_state == "ready"` without checking `get_readable_crs_profile(stored_crs_code) is not None` from `domains/spatial/crs.py`.
- **Current Mitigation:** In PostgreSQL, `0006_spatial_catalog.py` enforces `CONSTRAINT ck_source_ready_metadata` requiring `stored_crs IN (4326, 3857, 4756, 5899)` whenever `validation_state = 'ready'`, so definition-only EPSG codes (`3405`, `3406`) cannot be `ready` in the live database.
- **Recommendation:** Add an explicit `get_readable_crs_profile(stored_crs_code) is not None` check in `profiles.py` and `integration_hub.py` so the domain invariant does not rely solely on a database CHECK constraint.

### RESIDUAL-02 (Low — Minor OpenAPI Contract Drift on `adapter_key` Path Validation)
- **Location:** `specs/026-integration-hub-policy/contracts/integration-hub-api.yaml:66-70` vs `apps/api/openvietgis_api/adapters/http/integration_hub.py:161`
- **Observation:** The OpenAPI contract constrains `adapter_key` to `minLength: 1`, `maxLength: 64`, `pattern: "^[a-z0-9_]+$"` (which implies `422` for malformed keys), whereas FastAPI declares `adapter_key: str` with no `Path(...)` regex and returns `404` after `prepare_project_scope`.
- **Security Impact:** None — `_ADAPTERS_BY_KEY.get(key)` is a safe O(1) dictionary lookup executed after auth/RLS, and avoiding FastAPI pre-auth regex rejection prevents any pre-auth status divergence.
- **Recommendation:** Either align the OpenAPI spec description to note post-auth `404` for unknown/malformed keys, or validate `adapter_key` syntax **after** `prepare_project_scope`.

### RESIDUAL-03 (Low — Missing `Cache-Control: no-store` Header on Hub Responses)
- **Location:** `apps/api/openvietgis_api/adapters/http/integration_hub.py:131-193`
- **Observation:** Both hub routes return actor-specific authorization preflight results (`eligible_by_role`, `scoped_access`) and ACL-filtered layer metadata without setting `Cache-Control: no-store` (whereas `service_principals.py` and `gis_access.py` set `no-store`).
- **Recommendation:** Set `response.headers["Cache-Control"] = "no-store"` on both GET routes to prevent intermediary or browser caching across session/role changes.

### RESIDUAL-04 (Informational — Scoped Human View of `ogc_services` & Shared-Source Layer ACL Reliance)
- **Location:** `apps/api/openvietgis_api/domains/integration_hub/profiles.py:127-150`, `apps/api/openvietgis_api/adapters/http/integration_hub.py:104-109`
- **Observation:**
  1. For a `scoped_access: true` human inspecting `ogc_services`, `connection_profile.existing_endpoints` advertises `GET .../external-sources`, which is intentionally excluded from `SAFE_GET` in `gis_access_guard.py` (correctly fails closed with `403` if called).
  2. `_load_accessible_layers` filters in Python using `gis_access.source_allowed(..., "read")` while relying on PostgreSQL's RESTRICTIVE RLS policy `layers_gis_acl` (`gis_access.layer_allowed`) to isolate distinct layers that share the same `source_id`. `export_permitted` uses `gis_access.source_allowed(..., "export")`, matching `adapters/http/gis_access.py:194`.
- **Mitigation:** Fail-closed at DB RLS and `project_access_guard`; underscores why live PostGIS execution of T006 (`GATE-01`) is mandatory before merge.

### RESIDUAL-05 (Informational — Minor Fixture & Schema Discrepancies)
- `contracts/integration-hub-api.yaml:181` allows `ExistingEndpointDescriptor.method` enum `["GET", "POST"]`, while runtime (`profiles.py`) and SDK (`integration-hub.ts`) strictly emit/type `"GET"`.
- Mock fixtures in `packages/client-sdk/tests/integration-hub.test.ts` and `apps/web-next/integration-hub-smoke.mjs` show `qgis` `connection.inspect` as `executable_in_hub: true`, whereas backend `catalog.py` keeps `hub_executable_actions = (ConnectorAction.DISCOVER,)` (stricter fail-closed).
- `IntegrationCatalog.tsx` caches per-adapter profiles in component state until org/project switch or modal unmount; backend endpoints enforce live authorization independently.

---

## 4. Blocking Release Gates (Required Before Merge / Promotion)

| Gate ID | Requirement | Current State | Blocking? |
| :--- | :--- | :--- | :--- |
| **GATE-01 (T006)** | Execute `apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py` against live PostgreSQL/PostGIS (`openvietgis_runtime` role, real RLS, real service principal ceiling & revocation, real `gis_access` scoped layer ACL & VN-2000 CRS preservation) | **PENDING** — 3 tests authored & collected cleanly (`pytest --collect-only`), not yet executed against live PostGIS (Docker/PostGIS unavailable locally). | **YES — Blocks merge & deploy** |
| **GATE-02 (T007b)** | Run full TypeScript workspace validation (`pnpm validate:ts`: eslint, typecheck, `packages/client-sdk/tests/integration-hub.test.ts` via vitest, `web-next` build) + live Playwright browser E2E (`OVG_TEST_URL=... node apps/web-next/integration-hub-smoke.mjs`) | **PENDING** — Local `integration-hub-smoke.mjs` passed contract & UI invariants but skipped live browser E2E (`OVG_TEST_URL` unset); `pnpm` unavailable in local shell. | **YES — Blocks merge & deploy** |
| **GATE-03 (CI & Reconciliation)** | Reconcile branch cleanly with `origin/main` (`2373a53`, `db4784f`) and achieve green CI across `validate:py`, `validate:ts`, `validate:contracts`, `validate:arch`, `validate:docs`, `validate:integration`, `validate:e2e`. (`check_scope.py` has 27 pre-existing violations on `main@5baade9`, 0 R026 delta.) | **PENDING** — Dry `git merge-tree --write-tree origin/main HEAD` succeeded with 0 conflicts (`1407a96a443e046f5b8315f26f86214d25b74e2f`); full CI pipeline not yet run on PR. | **YES — Blocks merge & deploy** |
| **GATE-04 (T009)** | Staging/production release SHA verification, pre-deploy snapshot, post-deploy health/smoke check, and rollback readiness sign-off | **NOT STARTED** — No production deployment performed or claimed. | **YES — Blocks production promotion** |

---

## 5. Independent Verification Commands Executed Locally

All outputs recorded directly on `feature/026-integration-hub-policy` on 2026-11-23:

- `git log --oneline 5baade9..origin/main` -> `2373a53 docs: link public OpenVietGIS demo and source in product direction`, `db4784f docs: introduce OpenVietGIS community preview and live demo in README` (2 docs-only commits touching `README.md` and `docs/product.md`).
- `git merge-tree --write-tree origin/main HEAD` -> exit `0` (`1407a96a443e046f5b8315f26f86214d25b74e2f`, zero conflicts).
- `uv run --project apps/api python tools/check_architecture.py` -> `Architecture boundaries: OK` (exit `0`).
- `uv run --project apps/api python tools/check_docs.py` -> `Documentation navigation: OK` (exit `0`).
- `uv run --project apps/api python tools/check_contracts.py` -> `Spec 002, 003, 004 and 005 contracts: OK` (exit `0`).
- `uv run --project apps/api python tools/check_scope.py` -> exit `1` with **27 pre-existing violations** (identical to `main@5baade9`; **0 new violations** in R026 files).
- OpenAPI 3.1 validation (`specs/026-integration-hub-policy/contracts/integration-hub-api.yaml`) -> `OPENAPI_OK` (exit `0`).
- `uv run --project apps/api ruff check ...` on R026 Python files -> `All checks passed!` (exit `0`).
- `uv run --project apps/api mypy apps/api/openvietgis_api/domains/integration_hub apps/api/openvietgis_api/adapters/http/integration_hub.py` -> `Success: no issues found in 4 source files` (exit `0`).
- `uv run --project apps/api pytest apps/api/tests/unit/integration_hub apps/api/tests/unit/authorization -q` -> **57 passed** (30 `integration_hub` + 27 `authorization`, exit `0`).
- `uv run --project apps/api pytest apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py --collect-only -q` -> **3 tests collected** (live PostGIS execution pending).
- `node apps/web-next/integration-hub-smoke.mjs` -> `PASS typed SDK exports, URL encoding, and read-only GET contract`, `PASS UI read-only invariants, honest labels, and scope hygiene`, `SKIP live browser E2E (OVG_TEST_URL not set)` (exit `0`).
