# E10 Inventory & Contract Evidence — CAD, 3D, model and scene

- **Spec**: `specs/064-rust-platform/engines/e10-cad-3d.md` (Parent refs: R012, R037)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e10.json`
- **AC1 Status**: `verified` (`E10-T01`)
- **Architecture Candidate**: Existing CAD/WASM and Cesium retained; Rust API/CAD backend behind explicit contracts and adapters.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 30 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 18 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 24 |
| Mapped test files | 33 |
| Mapped test IDs | 84 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 12 |
| `retain` | 12 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"organization+project": 30}`
- **routes_with_project_access_guard**: 30
- **routes_with_csrf_enforcement**: 17
- **core_permissions_referenced**: `PROJECT_EDIT`, `PROJECT_READ`
- **extension_permissions_referenced**: none

## Mounted Runtime HTTP / SSE Routes (30)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/conversions` | `HTTP` | `cad.list_cad_conversions` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/export` | `HTTP` | `cad.export_cad_dxf` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/import` | `HTTP` | `cad.import_cad_file` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/inspect` | `HTTP` | `cad.inspect_cad_file` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/precision/intersection` | `HTTP` | `cad.cogo_intersection` | `organization+project` | project_access_guard, CSRF, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/precision/offset` | `HTTP` | `cad.cogo_offset` | `organization+project` | project_access_guard, CSRF, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/precision/snap` | `HTTP` | `cad.cogo_snap` | `organization+project` | project_access_guard, CSRF, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/precision/traverse` | `HTTP` | `cad.cogo_traverse` | `organization+project` | project_access_guard, CSRF, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents` | `HTTP` | `cad_documents.list_documents` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents` | `HTTP` | `cad_documents.create_document` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/links` | `HTTP` | `cad_documents.feature_cad_links` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions` | `HTTP` | `cad_documents.list_versions` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions` | `HTTP` | `cad_documents.append_version` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/download` | `HTTP` | `cad_documents.download_version` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/export-georef-draft` | `HTTP` | `cad_documents.export_georef_draft` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/import-georef` | `HTTP` | `cad_documents.import_georef` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/inspect-georef` | `HTTP` | `cad_documents.inspect_georef` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/links` | `HTTP` | `cad_documents.cad_feature_links` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/preview-native-import` | `HTTP` | `cad_documents.preview_native_import` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/models` | `HTTP` | `model_vault.list_models` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/models` | `HTTP` | `model_vault.create_model` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/models/{model_id}/download` | `HTTP` | `model_vault.download` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/models/{model_id}/placements` | `HTTP` | `model_vault.history` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/models/{model_id}/placements` | `HTTP` | `model_vault.revise_model` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/scenes` | `HTTP` | `saved_scenes.list_scenes` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/scenes` | `HTTP` | `saved_scenes.create_scene` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/scenes/{scene_id}/versions` | `HTTP` | `saved_scenes.history` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/scenes/{scene_id}/versions` | `HTTP` | `saved_scenes.revise_scene` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/scenes/{scene_id}/versions/{version}` | `HTTP` | `saved_scenes.open_scene` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integrations/v8i/capabilities` | `HTTP` | `v8i_bridge.read_v8i_capabilities` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |

## Component, Connector, Module & Operation Contracts

### CAD Adapters (`migrate`)
- `apps/api/openvietgis_api/adapters/cad/__init__.py` — functions: none
- `apps/api/openvietgis_api/adapters/cad/dxf_codec.py` — functions: `parse_dxf_2d`, `write_dxf_2d`
- `apps/api/openvietgis_api/adapters/cad/native_delta.py` — functions: `cad_to_gis_preview`, `gis_to_cad_preview`

### MicroStation V8i Bridge (`retain`)
- `integrations/microstation-v8i-bridge/bridge.py` — functions: `read_snapshot`, `main`
- `integrations/microstation-v8i-bridge/bridge_contract.py` — functions: `validate_snapshot`, `stable_digest`, `diff_snapshots`
- `integrations/microstation-v8i-bridge/windows_com.py` — functions: `probe_running_v8i`, `scan_model_preview`, `scan_running_v8i`
- Fixtures: `integrations/microstation-v8i-bridge/fixtures/sample-snapshot-v2.json`, `integrations/microstation-v8i-bridge/fixtures/sample-snapshot.json`

### 3D / CAD Browser Workspace & Relay (`retain`)
- `packages/spatial-workspace/src/` CAD/3D modules: 12
- `infra/cad-relay/` assets: 6

## Mapped Existing Test Coverage (33 files, 84 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/api/tests/integration/m3/test_cogo_authorization.py` | `integration` | 3 | `test_cogo_preview_requires_live_project_access`, `test_dxf_requires_explicit_units_and_crs_and_leaves_no_rejected_artifact`, `test_dxf_inspection_requires_edit_scope_and_does_not_publish` |
| `apps/api/tests/integration/m3/test_r025_v8i_capabilities.py` | `integration` | 1 | `test_v8i_capabilities_project_isolation` |
| `apps/api/tests/integration/spatial/test_cad_documents.py` | `integration` | 1 | `test_cad_versions_are_private_and_immutable` |
| `apps/api/tests/integration/spatial/test_cad_georef.py` | `integration` | 1 | `test_version_pinned_georeference` |
| `apps/api/tests/integration/spatial/test_model_vault.py` | `integration` | 1 | `test_model_vault` |
| `apps/api/tests/integration/spatial/test_saved_scenes.py` | `integration` | 1 | `test_saved_scene` |
| `apps/api/tests/unit/test_cogo_bounds.py` | `unit` | 3 | `test_cardinal_bearings`, `test_intersection_offset_and_deterministic_snap`, `test_invalid_controls_fail_before_output` |
| `apps/api/tests/unit/test_dxf_integrity.py` | `unit` | 15 | `test_dxf_declares_length_units`, `test_unsupported_topology_is_not_silently_omitted`, `test_declared_import_units_mismatch`, `test_bulged_polyline_is_not_silently_straightened` (+11 more) |
| `apps/api/tests/unit/test_r048_native_delta.py` | `unit` | 6 | `test_bidirectional_native_proposal_preserves_handle_and_unrelated_entity`, `test_order_is_not_identity`, `test_unsafe_native_changes_rejected`, `test_revision_and_mapping_checks` (+2 more) |
| `apps/api/tests/unit/test_v8i_capabilities.py` | `unit` | 1 | `test_v8i_capabilities_fail_closed` |
| `infra/cad-relay/tests/fixture.dxf` | `unit` | 0 |  |
| `infra/cad-relay/tests/index.html` | `unit` | 0 |  |
| `infra/cad-relay/tests/smoke.cjs` | `smoke` | 1 | `smoke_entrypoint` |
| `integrations/microstation-v8i-bridge/tests/test_bridge_contract.py` | `unit` | 15 | `SnapshotContractTests::test_validate_explicit_crs_and_projected_coordinates`, `SnapshotContractTests::test_stable_digest_independent_of_element_and_property_order`, `SnapshotContractTests::test_missing_crs_rejected`, `SnapshotContractTests::test_implicit_crs_rejected` (+11 more) |
| `integrations/microstation-v8i-bridge/tests/test_windows_scan.py` | `unit` | 8 | `SafeScanContract::test_extract_line_shape_skip_text`, `SafeScanContract::test_reject_unknown_dlong_identifier`, `SafeScanContract::test_stable_id_must_be_unique`, `SafeScanContract::test_known_but_malformed_geometry_aborts` (+4 more) |
| `packages/spatial-workspace/tests/browser/3d-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/3d.ts` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-draft-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-draft.tsx` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-selection-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-selection.tsx` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad.ts` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/multi-scene-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/multi-scene.ts` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/scene-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/scene.ts` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/tiles-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/tiles.ts` | `unit` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/cad-georeference.test.mjs` | `unit` | 4 | `millimetre CAD rotation and translation with verified residual`, `missing/degree CRS rejected`, `wrong unit and collinear points rejected`, `inconsistent control points and insufficient count rejected` |
| `packages/spatial-workspace/tests/footprint3d.test.mjs` | `unit` | 4 | `explicit base and height form actual top, identity retained`, `missing or invalid heights/duplicate ids rejected`, `non-4326 or unspecified vertical reference rejected`, `unclosed polygon and geographic out of range rejected` |
| `packages/spatial-workspace/tests/mesh-picking.test.mjs` | `unit` | 2 | `triangle hit uses nearest projected depth independent of winding`, `scene budget bounded on count and aggregate triangles` |
| `packages/spatial-workspace/tests/tileset.test.mjs` | `unit` | 3 | `region bounds, REPLACE refinement, capped deterministic selection`, `explicit ECEF transform, glTF y-up and WGS84 ellipsoid control points`, `reject unsupported URI, refinement and nested bounds` |

## E10-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e10.json`.
