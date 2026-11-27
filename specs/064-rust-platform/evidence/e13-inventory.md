# E13 Inventory & Contract Evidence — Field, offline, mobile

- **Spec**: `specs/064-rust-platform/engines/e13-field-offline.md` (Parent refs: R013)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e13.json`
- **AC1 Status**: `verified` (`E13-T01`)
- **Architecture Candidate**: Migrate field backend and offline snapshot codec to Rust; retain ServiceWorker/IndexedDB offline client shell and Playwright harness.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 6 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 12 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 11 |
| Mapped test files | 9 |
| Mapped test IDs | 18 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 5 |
| `retain` | 6 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"organization+project": 6}`
- **routes_with_project_access_guard**: 6
- **routes_with_csrf_enforcement**: 3
- **core_permissions_referenced**: none
- **extension_permissions_referenced**: none

## Mounted Runtime HTTP / SSE Routes (6)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/field/packages` | `HTTP` | `field.list_offline_packages` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/field/packages` | `HTTP` | `field.create_offline_package` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/field/packages/{package_id}/download` | `HTTP` | `field.download_offline_package` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/field/packages/{package_id}/revoke` | `HTTP` | `field.revoke_offline_package` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/field/packages/{package_id}/snapshot` | `HTTP` | `field.download_browser_snapshot` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/field/packages/{package_id}/sync` | `HTTP` | `field.sync_offline_outbox` | `organization+project` | project_access_guard, CSRF | `migrate` |

## Component, Connector, Module & Operation Contracts

### Offline Snapshot Format Adapter (`migrate`)
- `apps/api/openvietgis_api/adapters/formats/offline_snapshot.py` — functions: `canonical_json`, `embed_snapshot`, `extract_snapshot`, `validate_bbox`, `validate_offline_ops`

### Offline Client Shell & Playwright Harness (`retain`)
- Route: `/offline-field/:organizationId/:projectId`, ServiceWorker: `/offline-shell-sw.js`
- Playwright config: `playwright.offline-client.config.ts` (baseURL `http://127.0.0.1:5184`)
  - `apps/web-next/src/workbenches/features/offline/OfflineFieldPage.tsx`
  - `apps/web-next/src/workbenches/features/offline/OfflineMap.tsx`
  - `apps/web-next/src/workbenches/features/offline/offline-store.test.ts`
  - `apps/web-next/src/workbenches/features/offline/offline-store.ts`
  - `apps/web-next/src/workbenches/features/offline/offline.css`
  - `apps/web/scripts/build-offline-shell.mjs`
  - `playwright.offline-client.config.ts`
  - `tests/offline-client/static-server.mjs`

## Mapped Existing Test Coverage (9 files, 18 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/api/openvietgis_api/adapters/formats/offline_snapshot.py` | `unit` | 0 |  |
| `apps/api/tests/integration/m3/test_r010_r013_real.py` | `integration` | 2 | `test_r010_external_sources_ssrf_and_allowlist`, `test_r011_r012_r013_collaboration_cad_and_field_offline` |
| `apps/api/tests/integration/m3/test_r013_operation_mapping.py` | `integration` | 1 | `test_identified_group_receipt_replay_and_scope` |
| `apps/api/tests/integration/m3/test_r013_snapshot_pin_race.py` | `integration` | 2 | `test_package_snapshot_serializes_against_core_content_edit`, `test_package_rejects_inconsistent_selected_row_pin` |
| `apps/api/tests/unit/transfers/test_offline_core_operation_namespace.py` | `unit` | 2 | `test_internal_operation_id_reserved`, `test_normal_operations_unchanged` |
| `apps/api/tests/unit/transfers/test_offline_snapshot.py` | `unit` | 5 | `test_snapshot_projection_preserves_exact_precision_and_integrity`, `test_intent_must_use_issued_scope_and_canonical_value`, `test_identified_operations_are_canonical_and_unique`, `test_core_feature_identity_has_all_scope_dimensions` (+1 more) |
| `apps/web-next/src/workbenches/features/offline/offline-store.test.ts` | `unit` | 4 | `requires complete, scope-matched projection and GeoPackage checksums`, `queues a stable group and prohibits intent mutation after attempted submission`, `normalizes decimal precision without floating point coercion`, `enforces type/nullability` |
| `tests/e2e/r013-offline-real.spec.ts` | `playwright_e2e` | 1 | `R013 real OIDC + PostGIS: disconnected durable group, lost server acknowledgment, conflict and revocation` |
| `tests/offline-client/r013-client.spec.ts` | `playwright_e2e` | 1 | `verified package remains readable and draft survives real browser disconnect/reload; sync receipt persists` |

## E13-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e13.json`.
