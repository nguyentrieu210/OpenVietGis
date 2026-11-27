# E07 Inventory & Contract Evidence — Import/export and interoperability formats

- **Task**: `E07-T01` (Acceptance Criterion: `E07-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R005, R010
- **Child Spec**: `specs/064-rust-platform/engines/e07-formats-transfer.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e07.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: GDAL/PROJ native adapters behind Rust transfer job control plane; retain format-specific metadata & limits.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 11 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 7 |
| Classification: `replace` | 4 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 1 |
| Mapped & Triaged Source Files | 14 |
| Mapped Pytest Files | 6 |
| Mapped Pytest Test Case IDs | 18 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/artifacts/{artifact_id}` | `get_derived_artifact_api_v1_organizations__organization_id__projects__project_id__artifacts__artifact_id__get` | `get_derived_artifact` (`transfers`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 6 files (18 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/artifacts/{artifact_id}/download` | `download_derived_artifact_api_v1_organizations__organization_id__projects__project_id__artifacts__artifact_id__download_get` | `download_derived_artifact` (`transfers`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/exports/confirm` | `confirm_export_api_v1_organizations__organization_id__projects__project_id__exports_confirm_post` | `confirm_export` (`transfers`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/exports/preview` | `preview_export_api_v1_organizations__organization_id__projects__project_id__exports_preview_post` | `preview_export` (`transfers`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/imports/confirm` | `confirm_import_api_v1_organizations__organization_id__projects__project_id__imports_confirm_post` | `confirm_import` (`transfers`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | op-id | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/imports/inspect` | `inspect_import_asset_api_v1_organizations__organization_id__projects__project_id__imports_inspect_post` | `inspect_import_asset` (`transfers`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/imports/upload` | `upload_import_asset_api_v1_organizations__organization_id__projects__project_id__imports_upload_post` | `upload_import_asset` (`transfers`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 6 files (18 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/jobs` | `list_transfer_jobs_api_v1_organizations__organization_id__projects__project_id__jobs_get` | `list_transfer_jobs` (`transfers`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 6 files (18 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/jobs/{job_id}` | `get_transfer_job_api_v1_organizations__organization_id__projects__project_id__jobs__job_id__get` | `get_transfer_job` (`transfers`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/jobs/{job_id}/cancel` | `cancel_transfer_job_api_v1_organizations__organization_id__projects__project_id__jobs__job_id__cancel_post` | `cancel_transfer_job` (`transfers`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 6 files (18 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/jobs/{job_id}/retry` | `retry_transfer_job_api_v1_organizations__organization_id__projects__project_id__jobs__job_id__retry_post` | `retry_transfer_job` (`transfers`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 6 files (18 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "source": "apps/api/openvietgis_api/domains/transfers/domain.py",
  "engine": "E07",
  "formats": [
    "geopackage",
    "geojson",
    "shapefile_zip"
  ],
  "supported_crs_codes": [
    3857,
    4326,
    4756,
    5899
  ],
  "supported_encodings": [
    "utf-8",
    "windows-1258",
    "cp1252",
    "latin-1"
  ],
  "job_kinds": [
    "import",
    "export"
  ],
  "job_statuses": [
    "accepted",
    "queued",
    "active",
    "awaiting_confirmation",
    "cancel_requested",
    "recoverable_failed",
    "succeeded",
    "failed",
    "denied",
    "cancelled"
  ],
  "limits": {
    "max_upload_bytes": 26214400,
    "max_zip_expanded_bytes": 52428800,
    "max_zip_entries": 64,
    "max_transfer_features": 50000,
    "max_transfer_fields": 100,
    "max_string_len": 4096,
    "artifact_ttl_hours": 24
  }
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0008_import_export` | `0007_map_workspace` | `apps/api/migrations/versions/0008_import_export.py` | yes | `retain` | Add R005 import/export durable jobs, immutable source assets, and derived artifacts. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E07`: **14**

- `apps/api/openvietgis_api/adapters/formats/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/geojson.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/geopackage.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/postgis_sql.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/registry.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/shapefile_zip.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/wkt_crs.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/transfers.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/transfer_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/storage/local_object_store.py` (`migrate` — triaged from CROSS_CUTTING_REVIEW: Content-addressed/scoped local artifact object store shared by transfer artifacts (E07), print exports (E06), rasters (E09), CAD/models (E10), and field packages (E13).)
- `apps/api/openvietgis_api/domains/transfers/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/transfers/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/transfers/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/transfers/service.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Imports register/populate E02 spatial sources/layers with validated CRS transformations.
- **E09**: Raster uploads/GeoTIFFs are handled in E09 while vector GeoJSON/GPKG/Shapefile ZIP live in E07.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **6 pytest files** containing **18 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/integration/transfers/test_sc007_repeat_oracle.py` (primary, 1 tests): `test_r005_sc007_repeated_proposed_gpkg_point_profile`
- `apps/api/tests/integration/transfers/test_transfers_real.py` (primary, 4 tests): `test_r005_migration_rls_and_append_only_privileges`, `test_r005_import_export_round_trip_gdal_verify_and_idempotency`, `test_r005_vn2000_transform_invalid_geometry_and_revoked_download_security`, `test_r005_sc007_10000_feature_import_and_export_performance`
- `apps/api/tests/unit/transfers/test_codecs_and_domain.py` (primary, 5 tests): `test_detect_format_by_content_not_extension`, `test_geopackage_round_trip_preserves_null_empty_zero_false`, `test_geojson_rejects_legacy_crs_and_mixed_geometries`, `test_shapefile_zip_rejects_path_traversal_and_detects_missing_crs_encoding`, `test_export_loss_matrix_and_job_state_machine`
- `apps/api/tests/unit/transfers/test_gpkg_decimal_notation.py` (primary, 1 tests): `test_decimal_values_never_use_exponent_notation`
- `apps/api/tests/unit/transfers/test_offline_core_operation_namespace.py` (cross-ref from E13, 2 tests): `test_internal_operation_id_reserved`, `test_normal_operations_unchanged`
- `apps/api/tests/unit/transfers/test_offline_snapshot.py` (cross-ref from E13, 5 tests): `test_snapshot_projection_preserves_exact_precision_and_integrity`, `test_intent_must_use_issued_scope_and_canonical_value`, `test_identified_operations_are_canonical_and_unique`, `test_core_feature_identity_has_all_scope_dimensions`, `test_invalid_bounds_rejected`

## 8. Edge Cases & Parity Risks

- Malicious ZIP archives (zip bombs > 50 MiB expanded, > 64 entries, path traversal, uploads > 25 MiB) must be rejected.
- Feature count (> 50,000), field count (> 100), and string length (> 4096) limits enforced consistently.
- Unsupported CRS or unapproved CRS transformation edges and unsupported character encodings (outside utf-8/windows-1258/cp1252/latin-1) must fail with structured TransferError.
- GeoPackage decimal notation, shapefile DBF encoding, and round-trip geometry/attribute fidelity.
- Job cancellation, retry state transitions, and 24-hour artifact expiration (READY/EXPIRED/UNAVAILABLE).

## 9. E07-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **11** live runtime operations in `E07` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E07` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **1** owned migrations, **14** source files, and **6** pytest files (**18** test IDs) linked in `contracts/engines/e07.json`.
- [ ] Next phase (`E07-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
