# E09 Inventory & Contract Evidence — Raster, terrain, survey

- **Task**: `E09-T01` (Acceptance Criterion: `E09-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R008, R012
- **Child Spec**: `specs/064-rust-platform/engines/e09-raster-terrain.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e09.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: GDAL/COG + bounded Rust kernels for raster analysis, tiles, byte-range serving, and terrain preview.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 8 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 3 |
| Classification: `replace` | 5 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 1 |
| Mapped & Triaged Source Files | 5 |
| Mapped Pytest Files | 12 |
| Mapped Pytest Test Case IDs | 23 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/analyze` | `analyze_raster_api_v1_organizations__organization_id__projects__project_id__raster_analyze_post` | `analyze_raster` (`raster`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 11 files (22 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers` | `raster_layers_api_v1_organizations__organization_id__projects__project_id__raster_layers_get` | `raster_layers` (`raster`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 11 files (22 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers` | `store_raster_api_v1_organizations__organization_id__projects__project_id__raster_layers_post` | `store_raster` (`raster`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 11 files (22 IDs) |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers/{layer_id}` | `delete_raster_api_v1_organizations__organization_id__projects__project_id__raster_layers__layer_id__delete` | `delete_raster` (`raster`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 11 files (22 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers/{layer_id}/file` | `raster_file_api_v1_organizations__organization_id__projects__project_id__raster_layers__layer_id__file_get` | `raster_file` (`raster`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 11 files (22 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers/{layer_id}/terrain-preview` | `terrain_preview_api_v1_organizations__organization_id__projects__project_id__raster_layers__layer_id__terrain_preview_post` | `terrain_preview` (`terrain`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 9 files (19 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png` | `raster_tile_api_v1_organizations__organization_id__projects__project_id__raster_layers__layer_id__tiles__z___x___y__png_get` | `raster_tile` (`raster`) | `replace` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 11 files (22 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/raster/reports` | `raster_report_api_v1_organizations__organization_id__projects__project_id__raster_reports_post` | `raster_report` (`raster`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 11 files (22 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "sources": [
    "apps/api/openvietgis_api/adapters/http/raster.py",
    "apps/api/openvietgis_api/adapters/http/terrain.py"
  ],
  "engine": "E09",
  "analyze_methods": [
    "band",
    "ndvi",
    "ndwi",
    "ndbi",
    "hillshade"
  ],
  "store_and_derive_methods": [
    "band",
    "ndvi",
    "ndwi",
    "ndbi",
    "hillshade",
    "calculator",
    "rgb",
    "clip",
    "change",
    "slope",
    "aspect",
    "mosaic",
    "resample"
  ],
  "report_methods": [
    "zonal",
    "contour"
  ],
  "resampling_methods": [
    "near",
    "bilinear"
  ],
  "terrain_preview": {
    "elevation_units": [
      "metre"
    ],
    "vertical_references": [
      "local",
      "orthometric",
      "ellipsoidal"
    ],
    "requires_acknowledge_dem": true
  }
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0024_raster_layers` | `0023_gis_access` | `apps/api/migrations/versions/0024_raster_layers.py` | yes | `retain` | Persistent project-scoped raster assets and bounded XYZ tile pyramids. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E09`: **5**

- `apps/api/openvietgis_api/adapters/formats/persistent_raster_worker.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/raster_worker.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/formats/terrain_worker.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/raster.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/terrain.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Links raster/terrain products to project/layer catalog and explicit CRS metadata.
- **E05**: Raster PNG tiles (/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png) participate in E05 tile caching/ACL revocation.
- **E08**: Provides raster computation operations consumed by E08 processing_models DAGs.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **12 pytest files** containing **23 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/integration/m3/test_cogo_authorization.py` (primary, 3 tests): `test_cogo_preview_requires_live_project_access`, `test_dxf_requires_explicit_units_and_crs_and_leaves_no_rejected_artifact`, `test_dxf_inspection_requires_edit_scope_and_does_not_publish`
- `apps/api/tests/integration/m3/test_r010_cog_viewport.py` (primary, 1 tests): `test_r010_cog_viewport_authorization_pixels_and_generation`
- `apps/api/tests/integration/m3/test_r010_live_provider.py` (primary, 1 tests): `test_r010_provider_preview_real_auth_live_bytes_and_reopen`
- `apps/api/tests/integration/m3/test_r010_r013_real.py` (primary, 2 tests): `test_r010_external_sources_ssrf_and_allowlist`, `test_r011_r012_r013_collaboration_cad_and_field_offline`
- `apps/api/tests/integration/m3/test_r010_raster_provider.py` (primary, 1 tests): `test_r010_raster_profiles_protected_api`
- `apps/api/tests/integration/spatial/test_persistent_raster.py` (primary, 1 tests): `test_persistent_raster_lifecycle`
- `apps/api/tests/integration/spatial/test_raster_advanced.py` (primary, 1 tests): `test_advanced_raster_operations`
- `apps/api/tests/integration/spatial/test_raster_analysis.py` (primary, 1 tests): `test_raster_analysis_is_bounded_and_read_only`
- `apps/api/tests/integration/spatial/test_raster_terrain.py` (primary, 1 tests): `test_terrain_and_report`
- `apps/api/tests/integration/spatial/test_terrain_preview.py` (primary, 1 tests): `test_project_dem_preview`
- `apps/api/tests/unit/test_cogo_bounds.py` (primary, 3 tests): `test_cardinal_bearings`, `test_intersection_offset_and_deterministic_snap`, `test_invalid_controls_fail_before_output`
- `apps/api/tests/unit/test_external_raster_provider.py` (primary, 7 tests): `test_fixed_approved_tile_real_pixel_proof`, `test_tile_policy_valid_and_invalid_coordinates_bounds_origin`, `test_xyz_metadata_mismatch_never_requests_tile`, `test_wmts_metadata_wrong_matrix_crs_scale_or_limits_denied`, `test_tiled_geotiff_real_range_pixels_and_georeference` (+2 more)

## 8. Edge Cases & Parity Risks

- Raster upload size limits, storage quota enforcement, and malformed GeoTIFF/COG rejection.
- Explicit nodata propagation, band index bounds (1..16), formula sanitization in raster calculator, and resampling allowlist (near/bilinear).
- Byte-range (HTTP Range / 206) serving on /raster/layers/{layer_id}/file must handle invalid/unsatisfiable ranges safely.
- Terrain preview requires explicit elevation_unit='metre', valid vertical_reference, and acknowledge_dem=True (no survey certification claim).

## 9. E09-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **8** live runtime operations in `E09` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E09` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **1** owned migrations, **5** source files, and **12** pytest files (**23** test IDs) linked in `contracts/engines/e09.json`.
- [ ] Next phase (`E09-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
