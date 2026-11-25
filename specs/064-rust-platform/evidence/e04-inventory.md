# E04 Inventory & Contract Evidence — Queries, expression, attribute table

- **Task**: `E04-T01` (Acceptance Criterion: `E04-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R007
- **Child Spec**: `specs/064-rust-platform/engines/e04-query-attributes.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e04.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: PostGIS typed query compiler in Rust (zero arbitrary SQL); bounded AST/timeout/pagination.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 4 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 4 |
| Classification: `replace` | 0 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 1 |
| Mapped & Triaged Source Files | 4 |
| Mapped Pytest Files | 2 |
| Mapped Pytest Test Case IDs | 7 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/query` | `execute_layer_query_api_v1_organizations__organization_id__projects__project_id__layers__layer_id__query_post` | `execute_layer_query` (`queries`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR SHARE) | rev-pin | 2 files (7 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/saved-queries` | `list_saved_queries_api_v1_organizations__organization_id__projects__project_id__saved_queries_get` | `list_saved_queries` (`queries`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 2 files (7 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/saved-queries` | `create_saved_query_api_v1_organizations__organization_id__projects__project_id__saved_queries_post` | `create_saved_query` (`queries`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 2 files (7 IDs) |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/saved-queries/{query_id}` | `delete_saved_query_api_v1_organizations__organization_id__projects__project_id__saved_queries__query_id__delete` | `delete_saved_query` (`queries`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 2 files (7 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "source": "apps/api/openvietgis_api/domains/queries/domain.py",
  "engine": "E04",
  "spatial_predicates": [
    "intersects",
    "within",
    "contains"
  ],
  "bounds": {
    "max_query_limit": 500,
    "max_query_offset": 50000,
    "max_query_features": 50000,
    "max_query_positions": 1000000,
    "max_reference_features": 500,
    "max_reference_positions": 100000,
    "max_page_positions": 20000,
    "max_drawn_positions": 2048,
    "max_ast_depth": 6,
    "max_ast_nodes": 100,
    "max_ast_params": 1000,
    "statement_timeout": "5s"
  }
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0010_saved_queries` | `0009_editing_changesets` | `apps/api/migrations/versions/0010_saved_queries.py` | yes | `retain` | R007 Attribute & Spatial Query saved definitions with RLS. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E04`: **4**

- `apps/api/openvietgis_api/adapters/http/queries.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/query_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/queries/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/queries/domain.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Reads E02 source field types, explicit readable CRS codes, and native PostGIS geometries.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **2 pytest files** containing **7 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/integration/m2/test_query_integrity.py` (primary, 3 tests): `test_typed_decimal_literal_and_revision_paging`, `test_spatial_explicit_bounds_reference_scope_and_saved_replay`, `test_approved_web_mercator_filter_transformation`
- `apps/api/tests/unit/test_query_domain.py` (primary, 4 tests): `test_reject_invalid_typed_values`, `test_reject_bad_ast`, `test_exact_sql_values_and_literal_wildcards`, `test_drawn_polygon_shape_bounds`

## 8. Edge Cases & Parity Risks

- Zero arbitrary SQL injection: typed AST compiler only, enforcing field types and C-collation text comparisons.
- NULL comparisons must use is_null operator rather than raw null equality.
- AST bounds (depth <= 6, nodes <= 100, params <= 1000), page limit <= 500, HTTP offset <= 25,000 (domain cap 50,000), and 5s statement timeout must fail safely.
- Drawn spatial filter geometries must declare an explicit readable CRS and stay within MAX_DRAWN_POSITIONS (2048).

## 9. E04-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **4** live runtime operations in `E04` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E04` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **1** owned migrations, **4** source files, and **2** pytest files (**7** test IDs) linked in `contracts/engines/e04.json`.
- [ ] Next phase (`E04-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
