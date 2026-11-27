# E02 Inventory & Contract Evidence — Project, catalog, data, CRS

- **Task**: `E02-T01` (Acceptance Criterion: `E02-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R003, R004
- **Child Spec**: `specs/064-rust-platform/engines/e02-project-data-crs.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e02.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: PostGIS, PROJ, GEOS, GeoRust; explicit CRS registry & VN-2000 control pipelines; no implicit CRS.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 22 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 22 |
| Classification: `replace` | 0 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 3 |
| Mapped & Triaged Source Files | 36 |
| Mapped Pytest Files | 59 |
| Mapped Pytest Test Case IDs | 247 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers` | `listSpatialLayers` | `list_layers` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | op-id | 44 files (181 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers` | `createSpatialLayer` | `create_layer` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | op-id | 44 files (181 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/manual` | `createManualSpatialLayer` | `create_manual_layer` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}` | `getSpatialLayer` | `get_layer` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | op-id | 44 files (181 IDs) |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}` | `renameSpatialLayer` | `rename_layer` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/display-extent` | `getMapDisplayExtent` | `get_map_display_extent` (`map_viewport`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | rev-pin, op-id | 7 files (33 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/features` | `pageSpatialFeatures` | `read_features` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | rev-pin, op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/features/{feature_id}` | `getSpatialFeature` | `get_feature` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | rev-pin, op-id | 44 files (181 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/geometry-preview` | `previewSpatialGeometry` | `preview_geometry` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:allow, FOR UPDATE) | rev-pin, op-id | 44 files (181 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/retire` | `retireSpatialLayer` | `retire_layer` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/viewport` | `getMapViewport` | `get_map_viewport` (`map_viewport`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | rev-pin, op-id | 7 files (33 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/map` | `getMap` | `get_map` (`maps`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | op-id | 11 files (54 IDs) |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/map` | `saveMap` | `save_map` (`maps`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 11 files (54 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/map/history` | `listMapHistory` | `history` (`maps`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | op-id | 11 files (54 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/map/operations/{operation_id}` | `getMapOperation` | `get_operation` (`maps`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | op-id | 11 files (54 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/changes` | `listSpatialCatalogChanges` | `list_changes` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/crs-profiles` | `listSpatialCrsProfiles` | `list_crs_profiles` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/sources` | `listSpatialSources` | `list_sources` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | op-id | 44 files (181 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/sources` | `registerSpatialSource` | `register_source` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | op-id | 44 files (181 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/sources/{source_id}` | `getSpatialSource` | `get_source` (`spatial`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | op-id | 44 files (181 IDs) |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/sources/{source_id}` | `updateSpatialSource` | `update_source` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 44 files (181 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/spatial/sources/{source_id}/retire` | `retireSpatialSource` | `retire_source` (`spatial`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin, op-id | 44 files (181 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "source": "apps/api/openvietgis_api/domains/spatial/crs.py",
  "engine": "E02",
  "registry_version": "r003-v1",
  "geometry_families_supported": [
    "LineString",
    "Point",
    "Polygon"
  ],
  "geometry_dimensions": [
    "XY",
    "XYZ",
    "XYM",
    "XYZM",
    "mixed",
    "unknown"
  ],
  "field_types": [
    "text",
    "integer",
    "decimal",
    "boolean",
    "timestamp"
  ],
  "crs_profiles": [
    {
      "authority": "EPSG",
      "code": 4326,
      "name": "WGS 84",
      "datum": "World Geodetic System 1984 ensemble",
      "native_axes": [
        "latitude",
        "longitude"
      ],
      "unit": "degree",
      "coordinate_order": "xy",
      "x_axis": "longitude",
      "y_axis": "latitude",
      "projection_parameters": {},
      "area_of_use": "World.",
      "disposition": "readable"
    },
    {
      "authority": "EPSG",
      "code": 3857,
      "name": "WGS 84 / Pseudo-Mercator",
      "datum": "World Geodetic System 1984 ensemble",
      "native_axes": [
        "easting",
        "northing"
      ],
      "unit": "metre",
      "coordinate_order": "xy",
      "x_axis": "easting",
      "y_axis": "northing",
      "projection_parameters": {
        "method": "Popular Visualisation Pseudo Mercator (EPSG:1024)",
        "latitude_of_origin": 0.0,
        "central_meridian": 0.0,
        "false_easting": 0.0,
        "false_northing": 0.0,
        "sphere_radius": 6378137.0
      },
      "area_of_use": "World between 85.06°S and 85.06°N.",
      "disposition": "readable"
    },
    {
      "authority": "EPSG",
      "code": 4756,
      "name": "VN-2000",
      "datum": "Vietnam 2000",
      "native_axes": [
        "latitude",
        "longitude"
      ],
      "unit": "degree",
      "coordinate_order": "xy",
      "x_axis": "longitude",
      "y_axis": "latitude",
      "projection_parameters": {},
      "area_of_use": "Vietnam - onshore.",
      "disposition": "readable"
    },
    {
      "authority": "EPSG",
      "code": 5899,
      "name": "VN-2000 / TM-3 107-45",
      "datum": "Vietnam 2000",
      "native_axes": [
        "easting",
        "northing"
      ],
      "unit": "metre",
      "coordinate_order": "xy",
      "x_axis": "easting",
      "y_axis": "northing",
      "projection_parameters": {
        "method": "Transverse Mercator (EPSG:9807)",
        "latitude_of_origin": 0.0,
        "central_meridian": 107.75,
        "scale_factor": 0.9999,
        "false_easting": 500000.0,
        "false_northing": 0.0
      },
      "area_of_use": "Vietnam - Quang Ninh province; Da Nang municipality and Quang Nam province; Ba Ria-Vung Tau, Dong Nai and Lam Dong provinces.",
      "disposition": "readable"
    },
    {
      "authority": "EPSG",
      "code": 3405,
      "name": "VN-2000 / UTM zone 48N",
      "datum": "Vietnam 2000",
      "native_axes": [
        "easting",
        "northing"
      ],
      "unit": "metre",
      "coordinate_order": "xy",
      "x_axis": "easting",
      "y_axis": "northing",
      "projection_parameters": {
        "method": "Transverse Mercator (EPSG:9807)",
        "latitude_of_origin": 0.0,
        "central_meridian": 105.0,
        "scale_factor": 0.9996,
        "false_easting": 500000.0,
        "false_northing": 0.0
      },
      "area_of_use": "Vietnam - onshore west of 108°E.",
      "disposition": "definition_only"
    },
    {
      "authority": "EPSG",
      "code": 3406,
      "name": "VN-2000 / UTM zone 49N",
      "datum": "Vietnam 2000",
      "native_axes": [
        "easting",
        "northing"
      ],
      "unit": "metre",
      "coordinate_order": "xy",
      "x_axis": "easting",
      "y_axis": "northing",
      "projection_parameters": {
        "method": "Transverse Mercator (EPSG:9807)",
        "latitude_of_origin": 0.0,
        "central_meridian": 111.0,
        "scale_factor": 0.9996,
        "false_easting": 500000.0,
        "false_northing": 0.0
      },
      "area_of_use": "Vietnam - onshore east of 108°E.",
      "disposition": "definition_only"
    }
  ],
  "transformation_profiles": [
    {
      "source_code": 4326,
      "target_code": 3857,
      "operation_id": "epsg-1024-forward",
      "method": "analytic Pseudo-Mercator",
      "validated_area": "Longitude [-180, 180]°, latitude [-85.0511287798066, 85.0511287798066]°; projected X/Y [-20037508.342789244, 20037508.342789244] m; 1e-6 m output roundoff only, no input clipping.",
      "tolerance": 0.001,
      "tolerance_unit": "metre",
      "evidence_id": "analytic-webmercator-formula",
      "requires_trusted_fixture": false,
      "step_count": 1
    },
    {
      "source_code": 3857,
      "target_code": 4326,
      "operation_id": "epsg-1024-inverse",
      "method": "analytic Pseudo-Mercator inverse",
      "validated_area": "Longitude [-180, 180]°, latitude [-85.0511287798066, 85.0511287798066]°; projected X/Y [-20037508.342789244, 20037508.342789244] m; 1e-6 m output roundoff only, no input clipping.",
      "tolerance": 1e-08,
      "tolerance_unit": "degree",
      "evidence_id": "analytic-webmercator-formula",
      "requires_trusted_fixture": false,
      "step_count": 1
    },
    {
      "source_code": 4756,
      "target_code": 4326,
      "operation_id": "epsg-6960-forward",
      "method": "VN-2000 to WGS 84 (2)",
      "validated_area": "Trusted Quang Ninh reference fixtures only; longitude [106.88, 108.07]°, latitude [20.70, 21.42]°. Port-water control results, not an official boundary or extension of EPSG:6960 onshore scope. No survey-accuracy claim.",
      "tolerance": 3.0,
      "tolerance_unit": "metre",
      "evidence_id": "circular-50-2025-tt-bxd-quang-ninh",
      "requires_trusted_fixture": true,
      "step_count": 1
    },
    {
      "source_code": 4326,
      "target_code": 4756,
      "operation_id": "epsg-6960-inverse",
      "method": "inverse EPSG:6960",
      "validated_area": "Trusted Quang Ninh reference fixtures only; longitude [106.88, 108.07]°, latitude [20.70, 21.42]°. Port-water control results, not an official boundary or extension of EPSG:6960 onshore scope. No survey-accuracy claim.",
      "tolerance": 3.0,
      "tolerance_unit": "metre",
      "evidence_id": "circular-50-2025-tt-bxd-quang-ninh",
      "requires_trusted_fixture": true,
      "step_count": 1
    },
    {
      "source_code": 5899,
      "target_code": 4756,
      "operation_id": "epsg-5899-inverse-tm3",
      "method": "inverse native EPSG TM-3 107-45 projection",
      "validated_area": "Trusted Quang Ninh anchorage reference fixtures only; longitude [107.15, 107.18]°, latitude [20.70, 20.73]°. Product control rectangle, not an official boundary, extension of EPSG:6960 onshore scope, or survey-accuracy claim.",
      "tolerance": 1.0,
      "tolerance_unit": "metre",
      "evidence_id": "mae-eia-quang-ninh-table-1-4-qh-04",
      "requires_trusted_fixture": true,
      "step_count": 1
    },
    {
      "source_code": 4756,
      "target_code": 5899,
      "operation_id": "epsg-5899-forward-tm3",
      "method": "native EPSG TM-3 107-45 projection",
      "validated_area": "Trusted Quang Ninh anchorage reference fixtures only; longitude [107.15, 107.18]°, latitude [20.70, 20.73]°. Product control rectangle, not an official boundary, extension of EPSG:6960 onshore scope, or survey-accuracy claim.",
      "tolerance": 1.0,
      "tolerance_unit": "metre",
      "evidence_id": "mae-eia-quang-ninh-table-1-4-qh-04",
      "requires_trusted_fixture": true,
      "step_count": 1
    },
    {
      "source_code": 5899,
      "target_code": 4326,
      "operation_id": "epsg-5899-to-4326-via-6960",
      "method": "explicit inverse TM-3 then EPSG:6960 approved as one registry edge",
      "validated_area": "Trusted Quang Ninh anchorage reference fixtures only; longitude [107.15, 107.18]°, latitude [20.70, 20.73]°. Product control rectangle, not an official boundary, extension of EPSG:6960 onshore scope, or survey-accuracy claim.",
      "tolerance": 1.0,
      "tolerance_unit": "metre",
      "evidence_id": "mae-eia-quang-ninh-table-1-4-qh-04",
      "requires_trusted_fixture": true,
      "step_count": 2
    },
    {
      "source_code": 4326,
      "target_code": 5899,
      "operation_id": "epsg-4326-to-5899-via-6960",
      "method": "explicit inverse EPSG:6960 then TM-3 approved as one registry edge",
      "validated_area": "Trusted Quang Ninh anchorage reference fixtures only; longitude [107.15, 107.18]°, latitude [20.70, 20.73]°. Product control rectangle, not an official boundary, extension of EPSG:6960 onshore scope, or survey-accuracy claim.",
      "tolerance": 1.0,
      "tolerance_unit": "metre",
      "evidence_id": "mae-eia-quang-ninh-table-1-4-qh-04",
      "requires_trusted_fixture": true,
      "step_count": 2
    }
  ]
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0006_spatial_catalog` | `0005_retire_foundation_sample` | `apps/api/migrations/versions/0006_spatial_catalog.py` | yes | `retain` | Create the project-scoped spatial reference catalog (R003). |
| `0007_map_workspace` | `0006_spatial_catalog` | `apps/api/migrations/versions/0007_map_workspace.py` | yes | `retain` | Add atomic project map configuration, receipts and immutable revisions. |
| `0020_manual_layers` | `0019_extension_history` | `apps/api/migrations/versions/0020_manual_layers.py` | no | `retain` | Allow manually created, empty vector sources. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E02`: **36**

- `apps/api/openvietgis_api/adapters/http/map_viewport.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/maps.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/spatial.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/spatial_fixtures.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/spatial_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/spatial_transform.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/web_transform_guard.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/spatial/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/spatial/crs.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/spatial/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/spatial/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/spatial/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/spatial/tiles_generator.py` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/package.json` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/ModeSwitch.tsx` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/cad-editor.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/cad-embed.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/cad-georeference.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/cesium3d.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/coordinator.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/footprint3d.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/gltf.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/maplibre3d.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/mesh-layer.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/mesh-picking.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/mode-switch.css` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/renderer3d.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/session.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/snap-math.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/survey-math.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/terrain-analysis.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/terrain.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/tiles3d.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/tileset.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/src/tin-breaklines.ts` (`migrate` — static-inventory direct mapping)
- `packages/spatial-workspace/tsconfig.json` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E03**: Layer/source schemas and content_revision underpin editing changesets and geometry preview.
- **E05**: map_viewport (/viewport, /display-extent) and maps state directly feed MapLibre rendering and tile caching.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **59 pytest files** containing **247 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/contract/test_map_viewport_http.py` (cross-ref from E05, 3 tests): `test_scoped_context_and_explicit_empty_no_store`, `test_invalid_budget_or_bbox_never_reaches_read`, `test_extent_empty_and_failure_cannot_claim_complete`
- `apps/api/tests/contract/test_spatial_catalog.py` (primary, 9 tests): `test_register_and_inspect_projected_reference_source_contract`, `test_empty_and_invalid_registered_states_and_catalog_pagination`, `test_rename_stale_revision_and_non_destructive_retirement`, `test_explicit_crs_correction_requires_verified_fixture_evidence`, `test_unknown_and_duplicate_reference_key_are_explicit_safe_errors` (+4 more)
- `apps/api/tests/contract/test_spatial_changes.py` (cross-ref from E03, 4 tests): `test_empty_history_default_bound_and_current_scope`, `test_history_preserves_actual_evidence_retirement_and_complete_keyset`, `test_history_requires_read_permission_before_repository_and_reauthorizes`, `test_invalid_history_queries_are_sanitized`
- `apps/api/tests/contract/test_spatial_consumer.py` (primary, 1 tests): `test_independent_contract_consumer`
- `apps/api/tests/contract/test_spatial_crs.py` (primary, 2 tests): `test_crs_discovery_exact_finite_registry_and_current_read`, `test_crs_discovery_does_not_bypass_current_access_or_scope`
- `apps/api/tests/contract/test_spatial_features.py` (primary, 10 tests): `test_native_and_identity_page_detail_exact_contract`, `test_limit_and_ascii_keyset_continuations_with_exact_typed_attributes`, `test_small_fixture_full_traversal_and_geometry_omission`, `test_ready_empty_and_nonexistent_scoped_cursor_and_feature`, `test_one_stale_pin_is_conflict_even_for_missing_feature` (+5 more)
- `apps/api/tests/contract/test_spatial_layers.py` (primary, 8 tests): `test_create_list_detail_share_current_named_source_and_explicit_capabilities`, `test_feature_unavailable_preserves_read_authority_and_metadata_support`, `test_rename_retire_stale_noop_and_dependency_contract`, `test_create_must_resolve_active_ready_source_and_hide_absent_or_retired_ids`, `test_invalid_layer_contract_payloads_are_sanitized_and_side_effect_free` (+3 more)
- `apps/api/tests/contract/test_spatial_transform_wire.py` (primary, 1 tests): `test_transform_contract_preserves_native_attributes_pins_and_selected_identity`
- `apps/api/tests/integration/m2/test_crs_consumer_guards.py` (primary, 2 tests): `test_processing_and_print_fail_closed_for_unapproved_crs`, `test_web_mercator_consumers_use_verified_profile_and_bounds`
- `apps/api/tests/integration/m2/test_r006_r009_real.py` (cross-ref from E03, 4 tests): `test_r006_editing_changesets_conflicts_validation_and_revert`, `test_r006_noop_and_snap_require_current_edit_authority`, `test_r006_concurrent_identical_save_returns_one_receipt`, `test_r007_r008_r009_query_processing_and_cartography`
- `apps/api/tests/integration/m2/test_r006_schema_pins.py` (cross-ref from E03, 1 tests): `test_stale_schema_fails_without_publication_and_pinned_replay_is_exact`
- `apps/api/tests/integration/maps/test_authorization_matrix.py` (primary, 5 tests): `test_all_map_reads_follow_current_grant_and_same_session_revocation`, `test_edit_only_service_can_save_and_reconcile_without_read_disclosure`, `test_dual_authorized_sibling_layer_substitution_and_foreign_save_are_concealed`, `test_map_cookie_save_requires_csrf_but_bearer_uses_live_edit_ceiling`, `test_archived_project_remains_readable_but_denies_save_and_edit_receipts`
- `apps/api/tests/integration/maps/test_map_migrations.py` (primary, 6 tests): `test_map_runtime_nonowner_forced_rls_and_append_only_evidence_privileges`, `test_map_rls_no_scope_foreign_scope_and_reused_connection_are_isolated`, `test_runtime_cannot_mutate_or_delete_existing_receipt_and_history`, `test_direct_configuration_write_without_matching_next_evidence_cannot_commit`, `test_database_configuration_validator_rejects_closed_shape_and_type_violations` (+1 more)
- `apps/api/tests/integration/maps/test_map_viewport_real.py` (cross-ref from E05, 7 tests): `test_exact_geographic_viewport_prefix_and_full_actual_extent`, `test_direct_5899_control_operation_is_geographic_and_never_native_numbers`, `test_display_domain_stale_and_foreign_layer_do_not_fabricate_empty`, `test_sql_failure_rolls_back_and_returns_safe_unavailable`, `test_runtime_has_no_unscoped_geometry_access` (+2 more)
- `apps/api/tests/integration/maps/test_map_warm_revision_security.py` (cross-ref from E05, 1 tests): `test_warmed_viewport_and_extent_recheck_live_pins_and_actor`
- `apps/api/tests/integration/maps/test_saves.py` (primary, 6 tests): `test_open_is_read_only_and_first_empty_save_has_exact_minimal_receipt`, `test_lost_ack_after_later_save_and_new_session_returns_original_receipt`, `test_stale_changed_intent_noop_and_foreign_actor_reuse_are_atomic`, `test_designated_viewpoint_preserve_clear_and_safe_history_pagination`, `test_precommit_failure_rolls_back_state_operation_history_and_success_audit` (+1 more)
- `apps/api/tests/integration/maps/test_wire_contract.py` (primary, 1 tests): `test_real_map_receipts_history_and_pinned_render_payloads_conform`
- `apps/api/tests/integration/spatial/test_authorization.py` (cross-ref from E01, 5 tests): `test_human_current_membership_and_grant`, `test_service_current_grant_and_credential_ceiling`, `test_human_csrf_all_mutations_and_bearer_edit_control`, `test_scope_substitution_and_dual_authorized_cross_project_creation`, `test_change_history_rechecks_current_read_and_scope`
- `apps/api/tests/integration/spatial/test_change_evidence.py` (cross-ref from E03, 6 tests): `test_project_history_exact_wire_values_keyset_and_retired_records`, `test_service_actor_snapshot_immutable_after_credential_and_principal_revoke`, `test_noop_stale_denied_or_dependency_conflict_never_append_evidence`, `test_failed_evidence_append_rolls_back_mutation_and_audit`, `test_concurrent_compare_and_set_has_one_winner_and_one_change` (+1 more)
- `apps/api/tests/integration/spatial/test_consumer.py` (primary, 1 tests): `test_independent_consumer_restricted_postgis`
- `apps/api/tests/integration/spatial/test_crs_controls.py` (primary, 8 tests): `test_all_approved_directions_against_independent_controls`, `test_webmercator_transforms_each_vertex_and_preserves_geometry_shape`, `test_reject_unapproved_direct_edges_and_definition_only_crs`, `test_outside_approved_area_fails_closed_for_page_and_detail`, `test_untrusted_provenance_and_any_invalid_vertex_or_row_fail_whole_read` (+3 more)
- `apps/api/tests/integration/spatial/test_features.py` (primary, 4 tests): `test_all_primary_features_once_in_native_order`, `test_native_read_flags_cursor_and_scoped_detail`, `test_empty_and_unavailable_native_reads`, `test_transform_expanded_page_preserves_order_and_coordinates`
- `apps/api/tests/integration/spatial/test_fixture_validation.py` (primary, 5 tests): `test_all_native_fixtures_verified_twice_in_one_restricted_transaction`, `test_native_correction_verifies_existing_manifest_and_leaves_rows_unchanged`, `test_persisted_integrity_mismatch_fails_closed`, `test_candidate_manifest_and_persisted_scope_guard_fail_closed`, `test_unknown_native_query_failure_is_not_an_accepted_correction_rejection`
- `apps/api/tests/integration/spatial/test_gis_access.py` (cross-ref from E01, 1 tests): `test_scoped_layer_features_and_editing`
- `apps/api/tests/integration/spatial/test_gis_access_batch.py` (cross-ref from E01, 1 tests): `test_batch_rules_and_shared_activation`
- `apps/api/tests/integration/spatial/test_layer_repository.py` (primary, 1 tests): `test_layer_repository_scoped_keyset_and_shared_source`
- `apps/api/tests/integration/spatial/test_layer_service.py` (primary, 1 tests): `test_layer_service_lifecycle_and_capabilities`
- `apps/api/tests/integration/spatial/test_layers.py` (primary, 4 tests): `test_two_layers_share_one_source_and_retirement_is_logical`, `test_layer_rename_cas_stale_and_retired_errors_are_atomic`, `test_nonready_retired_missing_and_dual_authorized_cross_project_source`, `test_concurrent_layer_create_and_source_retire_have_one_winner`
- `apps/api/tests/integration/spatial/test_lifecycle_security.py` (cross-ref from E01, 7 tests): `test_archived_project_preserves_reads_but_checks_permission_before_state`, `test_project_deleted_or_organization_suspended_hides_even_pinned_reads`, `test_old_pins_do_not_cache_grant_or_membership_authority`, `test_workspace_reassignment_rechecks_inherited_grants_with_same_old_pins`, `test_retired_resources_hide_old_pins_and_ordinary_metadata_but_keep_history` (+2 more)
- `apps/api/tests/integration/spatial/test_manual_editor.py` (primary, 1 tests): `test_manual_editor`
- `apps/api/tests/integration/spatial/test_migration_rls.py` (cross-ref from E01, 12 tests): `test_spatial_schema_runtime_role_is_non_owner_and_rls_forced`, `test_project_rls_reads_hide_same_org_sibling_and_foreign_org`, `test_project_rls_write_checks_and_composite_foreign_keys`, `test_append_only_catalog_evidence_and_source_uniqueness`, `test_storage_constraints_enforce_metadata_identity_and_evidence` (+7 more)
- `apps/api/tests/integration/spatial/test_pinned_reads.py` (primary, 3 tests): `test_refresh_changes_only_affected_pins_and_metadata_renames_preserve_them`, `test_read_and_refresh_lock_prevents_mixed_revision`, `test_failed_refresh_rolls_back_manifest_pins_and_evidence`
- `apps/api/tests/integration/spatial/test_repository_scope.py` (cross-ref from E01, 6 tests): `test_repository_fails_closed_before_verified_scope_and_on_wrong_project`, `test_repository_archived_read_allowed_mutation_denied_and_scope_cleared`, `test_repository_denies_ungranted_member_and_action_mismatch`, `test_source_cas_and_audit_are_atomic`, `test_read_scope_cannot_write_and_active_layer_blocks_retire` (+1 more)
- `apps/api/tests/integration/spatial/test_restart.py` (cross-ref from E01, 2 tests): `test_fresh_app_and_pool_recreation_preserve_committed_catalog_and_pins`, `test_interrupted_registration_rolls_back_then_fresh_app_retry_is_complete`
- `apps/api/tests/integration/spatial/test_saved_scenes.py` (primary, 1 tests): `test_saved_scene`
- `apps/api/tests/integration/spatial/test_source_repository.py` (primary, 2 tests): `test_scoped_source_storage_reads_and_duplicate_binding`, `test_source_service_mutation_and_revision_lifecycle`
- `apps/api/tests/integration/spatial/test_sources.py` (primary, 8 tests): `test_ready_native_metadata`, `test_nonready_fixture_preserves_truthful_metadata_and_native_geometry`, `test_binding_rename_retire_and_rejections_leave_only_material_evidence`, `test_source_retirement_rejects_active_layer_without_side_effect`, `test_correction_requires_independent_evidence_and_never_rewrites_geometry` (+3 more)
- `apps/api/tests/integration/spatial/test_team_chat.py` (primary, 1 tests): `test_team_chat_and_task_isolation`
- `apps/api/tests/integration/spatial/test_team_locks.py` (primary, 1 tests): `test_feature_leases_enforce_core_writes_and_expire`
- `apps/api/tests/integration/spatial/test_team_presence.py` (primary, 2 tests): `test_presence_payload`, `test_presence_access_and_broadcast`
- `apps/api/tests/integration/spatial/test_team_realtime.py` (primary, 2 tests): `test_private_messages_conversion_and_notification`, `test_sse_pushes_committed_changes`
- `apps/api/tests/integration/spatial/test_tiles_generator.py` (cross-ref from E05, 2 tests): `test_generate_tileset`, `test_generate_projected_tileset`
- `apps/api/tests/integration/spatial/test_tilesets.py` (primary, 1 tests): `test_tilesets`
- `apps/api/tests/performance/test_spatial_benchmark.py` (primary, 1 tests): `test_spatial_http_p95_10k_native_and_transformed`
- `apps/api/tests/unit/maps/test_map_domain.py` (primary, 9 tests): `test_map_identity_is_stable_and_project_specific`, `test_viewpoint_rejects_invalid_numbers_and_crs`, `test_style_is_bounded_and_cannot_carry_renderer_expressions`, `test_configuration_rejects_duplicate_or_too_many_layer_references`, `test_viewpoint_preserve_and_clear_are_explicit` (+4 more)
- `apps/api/tests/unit/maps/test_map_render_capabilities.py` (cross-ref from E05, 2 tests): `test_supported_finite_profiles_have_explicit_permission_and_no_edit`, `test_latitude86_is_readable_but_never_advertised_as_renderable`
- `apps/api/tests/unit/maps/test_map_service.py` (primary, 6 tests): `test_unsaved_get_is_virtual_and_first_empty_save_commits_once`, `test_lost_ack_replays_original_receipt_even_after_newer_save`, `test_operation_reuse_changed_intent_or_foreign_actor_never_returns_receipt`, `test_replay_and_receipt_lookup_recheck_current_edit_authority`, `test_new_stale_and_noop_operations_preserve_prior_revision` (+1 more)
- `apps/api/tests/unit/maps/test_map_viewport.py` (cross-ref from E05, 8 tests): `test_reject_nonfinite_wrapped_or_degenerate_viewport`, `test_viewport_limits_are_not_source_completeness`, `test_position_budget_keeps_prefix_and_never_skips_offending_feature`, `test_entire_json_envelope_counts_towards_byte_budget`, `test_empty_viewport_is_complete_and_explicitly_empty` (+3 more)
- `apps/api/tests/unit/spatial/test_crs.py` (primary, 6 tests): `test_exact_registry_matches_independent_control_evidence`, `test_no_coercion_of_crs_identity_or_edges`, `test_closed_operation_domain_has_no_input_epsilon`, `test_distinguish_native_geometry_domain_from_operation_domain`, `test_reject_unreadable_geometry_all_vertices` (+1 more)
- `apps/api/tests/unit/spatial/test_crs_registry.py` (primary, 11 tests): `test_registry_is_finite_and_definition_only_is_not_readable`, `test_native_axes_and_explicit_xy_coordinate_reference_are_distinct`, `test_web_mercator_definition_does_not_claim_precise_geodetic_datum_shift`, `test_vn2000_definition_metadata_distinguishes_projection_profiles`, `test_profile_wire_uses_exact_contract_keys_and_does_not_mutate_registry` (+6 more)
- `apps/api/tests/unit/spatial/test_domain.py` (primary, 12 tests): `test_invalid_source_metadata_is_rejected`, `test_source_states_and_revision_identity`, `test_field_identity_and_type_constraints`, `test_typed_attribute_values_are_exact_and_canonical`, `test_type_errors_are_not_silently_coerced` (+7 more)
- `apps/api/tests/unit/spatial/test_feature_persistence.py` (primary, 3 tests): `test_native_wkb_preserves_binary64_and_exact_shape`, `test_invalid_native_wkb_fails_closed`, `test_controlled_refresh_only_changes_one_pin`
- `apps/api/tests/unit/spatial/test_feature_service.py` (primary, 3 tests): `test_exact_logical_attributes_and_native_coordinate_precision`, `test_corrupt_logical_attribute_fails_closed`, `test_mismatched_snapshot_and_missing_attribute_fail_closed`
- `apps/api/tests/unit/spatial/test_service.py` (primary, 5 tests): `test_manifest_pins_derived_not_trusted_and_resource_identity_independent`, `test_source_rename_evidence_actual_values_and_stable_read_pins`, `test_noop_and_cross_project_mutation_rejected`, `test_crs_correction_changes_content_and_preserves_schema`, `test_layer_rename_and_retirement_evidence`
- `apps/api/tests/unit/spatial/test_spatial_fixtures.py` (primary, 9 tests): `test_immutable_allowlist_rejects_unknown_and_arbitrary_binding`, `test_primary_fixtures_have_independent_counts_native_bounds_and_stable_geometry`, `test_typed_attributes_are_stable_and_distinguish_zero_null_and_empty`, `test_materialization_uses_same_content_pins_but_distinct_scoped_owners`, `test_published_hvs_native_coordinates_are_separate_narrow_control_fixtures` (+4 more)
- `apps/api/tests/unit/spatial/test_tiles_generator.py` (cross-ref from E05, 3 tests): `test_wgs84_and_scale`, `test_bad_height`, `test_bad_extent`
- `apps/api/tests/unit/spatial/test_tiles_generator_math.py` (cross-ref from E05, 3 tests): `test_wgs84_and_scale`, `test_bad_height`, `test_bad_extent`
- `apps/api/tests/unit/test_editing_domain.py` (cross-ref from E03, 4 tests): `test_editing_decimal_canonicalizes_before_publish`, `test_editing_timestamp_is_a_canonical_utc_string`, `test_invalid_logical_attribute_rejected_before_commit`, `test_editing_required_null_and_unknown_fields_rejected`
- `apps/api/tests/unit/test_editing_replay_and_rebase.py` (cross-ref from E03, 3 tests): `test_digest_is_exact_and_key_order_independent`, `test_draft_is_bounded_and_typed`, `test_rebase_is_advisory_and_detects_existence_conflicts`

## 8. Edge Cases & Parity Risks

- Implicit or missing CRS must be rejected; definition-only CRS (EPSG:3405, 3406) must not allow coordinate reads.
- VN-2000 (EPSG:4756 / 5899) transformations must enforce trusted-fixture and validated bounding-box guards (Quang Ninh controls) and exact tolerances.
- Axis order must remain strictly XY (lon/lat or easting/northing) with Web Mercator latitude clamp +/-85.0511287798066 deg.
- Optimistic revision mismatch on source/layer/map save must reject stale writes without silent overwrite.
- Retiring a source with active dependent layers must fail with 409 ACTIVE_LAYER_DEPENDENCIES.
- Bounded viewport / feature pagination must never trigger full-table unbounded downloads on 30k/100k/1M layers.

## 9. E02-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **22** live runtime operations in `E02` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E02` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **3** owned migrations, **36** source files, and **59** pytest files (**247** test IDs) linked in `contracts/engines/e02.json`.
- [ ] Next phase (`E02-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
