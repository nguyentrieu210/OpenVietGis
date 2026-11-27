# E05 Inventory & Contract Evidence — Map rendering, tile, cache

- **Task**: `E05-T01` (Acceptance Criterion: `E05-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R004, R043
- **Child Spec**: `specs/064-rust-platform/engines/e05-render-tiles.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e05.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: MapLibre retained client-side; Martin/Rust tile gateway, MVT/3D Tiles/PMTiles; scoped cache & ACL revocation.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 7 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 4 |
| Classification: `replace` | 3 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 6 |
| Mapped & Triaged Source Files | 10 |
| Mapped Pytest Files | 18 |
| Mapped Pytest Test Case IDs | 72 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets` | `list_tilesets_api_v1_organizations__organization_id__projects__project_id__3d_tilesets_get` | `list_tilesets` (`tilesets`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets` | `create_tileset_api_v1_organizations__organization_id__projects__project_id__3d_tilesets_post` | `create_tileset` (`tilesets`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (8 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets/generate` | `generate_tileset_api_v1_organizations__organization_id__projects__project_id__3d_tilesets_generate_post` | `generate_tileset` (`tilesets`) | `replace` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets/{tileset_id}/feature` | `get_tile_feature_api_v1_organizations__organization_id__projects__project_id__3d_tilesets__tileset_id__feature_get` | `get_tile_feature` (`tilesets`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | rev-pin | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets/{tileset_id}/feature-by-id` | `locate_tile_feature_api_v1_organizations__organization_id__projects__project_id__3d_tilesets__tileset_id__feature_by_id_get` | `locate_tile_feature` (`tilesets`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets/{tileset_id}/tileset.json` | `get_manifest_api_v1_organizations__organization_id__projects__project_id__3d_tilesets__tileset_id__tileset_json_get` | `get_manifest` (`tilesets`) | `replace` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/3d/tilesets/{tileset_id}/{uri}` | `get_tile_api_v1_organizations__organization_id__projects__project_id__3d_tilesets__tileset_id___uri__get` | `get_tile` (`tilesets`) | `replace` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "tileset_capabilities": [
    "3D Tiles archive upload & manifest validation (tileset.json)",
    "3D Tiles server-side generation from spatial sources",
    "Tile feature metadata lookup & feature-by-id location index",
    "Authorized tileset manifest & tile URI streaming"
  ],
  "cross_engine_render_and_tile_routes": [
    {
      "engine": "E02",
      "module": "map_viewport",
      "paths": [
        "/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/viewport",
        "/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/display-extent"
      ]
    },
    {
      "engine": "E08",
      "module": "professional",
      "paths": [
        "/api/v1/organizations/{organization_id}/projects/{project_id}/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt"
      ]
    },
    {
      "engine": "E09",
      "module": "raster",
      "paths": [
        "/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png"
      ]
    }
  ]
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0032_tilesets` | `0031_saved_scenes` | `apps/api/migrations/versions/0032_tilesets.py` | yes | `retain` | Immutable scoped 3D Tiles resources. |
| `0033_tileset_provenance` | `0032_tilesets` | `apps/api/migrations/versions/0033_tileset_provenance.py` | no | `retain` | Retain the immutable source pins for generated tilesets. |
| `0034_tile_feature_index` | `0033_tileset_provenance` | `apps/api/migrations/versions/0034_tile_feature_index.py` | no | `retain` | Immutable per-tile triangle-to-GIS feature index. |
| `0036_consolidated_tiles` | `('0035_consolidated_scenes', '0032_tilesets')` | `apps/api/migrations/versions/0036_consolidated_tiles.py` | no | `retain` | Converge scoped tiles and unified migrations. |
| `0037_consolidated_generation` | `('0036_consolidated_tiles', '0033_tileset_provenance')` | `apps/api/migrations/versions/0037_consolidated_generation.py` | no | `retain` | Converge generated tiles provenance with the consolidated schema. |
| `0038_consolidated_feature_index` | `('0037_consolidated_generation', '0034_tile_feature_index')` | `apps/api/migrations/versions/0038_consolidated_feature_index.py` | no | `retain` | Converge the 3D tile feature-index and generated-tiles migration branches. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E05`: **10**

- `apps/api/openvietgis_api/adapters/http/tilesets.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/map_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/map_viewport.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/rendering/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/rendering/map_renderer.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/maps/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/maps/domain.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/maps/ports.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/maps/service.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/maps/viewport.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Depends on E02 spatial layers, CRS, maps state, and map_viewport endpoints.
- **E03**: Requires cache invalidation on E03 changeset commits and E01 ACL revocations.
- **E08**: Coordinates MVT tile serving (/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt) with Martin/Rust tile gateway.
- **E09**: Coordinates raster tile serving (/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png).

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **18 pytest files** containing **72 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/contract/test_map_viewport_http.py` (primary, 3 tests): `test_scoped_context_and_explicit_empty_no_store`, `test_invalid_budget_or_bbox_never_reaches_read`, `test_extent_empty_and_failure_cannot_claim_complete`
- `apps/api/tests/integration/m2/test_processing_integrity.py` (cross-ref from E08, 4 tests): `test_stale_job_fails_durably_without_repinning`, `test_clip_retains_family_and_empty_output`, `test_atomic_rollback_on_publication_failure_and_consumers`, `test_idempotency_and_null_dissolve_groups`
- `apps/api/tests/integration/maps/test_authorization_matrix.py` (cross-ref from E02, 5 tests): `test_all_map_reads_follow_current_grant_and_same_session_revocation`, `test_edit_only_service_can_save_and_reconcile_without_read_disclosure`, `test_dual_authorized_sibling_layer_substitution_and_foreign_save_are_concealed`, `test_map_cookie_save_requires_csrf_but_bearer_uses_live_edit_ceiling`, `test_archived_project_remains_readable_but_denies_save_and_edit_receipts`
- `apps/api/tests/integration/maps/test_map_migrations.py` (cross-ref from E02, 6 tests): `test_map_runtime_nonowner_forced_rls_and_append_only_evidence_privileges`, `test_map_rls_no_scope_foreign_scope_and_reused_connection_are_isolated`, `test_runtime_cannot_mutate_or_delete_existing_receipt_and_history`, `test_direct_configuration_write_without_matching_next_evidence_cannot_commit`, `test_database_configuration_validator_rejects_closed_shape_and_type_violations` (+1 more)
- `apps/api/tests/integration/maps/test_map_viewport_real.py` (primary, 7 tests): `test_exact_geographic_viewport_prefix_and_full_actual_extent`, `test_direct_5899_control_operation_is_geographic_and_never_native_numbers`, `test_display_domain_stale_and_foreign_layer_do_not_fabricate_empty`, `test_sql_failure_rolls_back_and_returns_safe_unavailable`, `test_runtime_has_no_unscoped_geometry_access` (+2 more)
- `apps/api/tests/integration/maps/test_map_warm_revision_security.py` (primary, 1 tests): `test_warmed_viewport_and_extent_recheck_live_pins_and_actor`
- `apps/api/tests/integration/maps/test_saves.py` (cross-ref from E02, 6 tests): `test_open_is_read_only_and_first_empty_save_has_exact_minimal_receipt`, `test_lost_ack_after_later_save_and_new_session_returns_original_receipt`, `test_stale_changed_intent_noop_and_foreign_actor_reuse_are_atomic`, `test_designated_viewpoint_preserve_clear_and_safe_history_pagination`, `test_precommit_failure_rolls_back_state_operation_history_and_success_audit` (+1 more)
- `apps/api/tests/integration/maps/test_wire_contract.py` (cross-ref from E02, 1 tests): `test_real_map_receipts_history_and_pinned_render_payloads_conform`
- `apps/api/tests/integration/spatial/test_processing_models.py` (cross-ref from E08, 1 tests): `test_saved_processing_graph_and_real_chain`
- `apps/api/tests/integration/spatial/test_professional.py` (cross-ref from E08, 2 tests): `test_pro_tools`, `test_projected_native_tile_is_revision_pinned`
- `apps/api/tests/integration/spatial/test_tiles_generator.py` (primary, 2 tests): `test_generate_tileset`, `test_generate_projected_tileset`
- `apps/api/tests/unit/maps/test_map_domain.py` (cross-ref from E02, 9 tests): `test_map_identity_is_stable_and_project_specific`, `test_viewpoint_rejects_invalid_numbers_and_crs`, `test_style_is_bounded_and_cannot_carry_renderer_expressions`, `test_configuration_rejects_duplicate_or_too_many_layer_references`, `test_viewpoint_preserve_and_clear_are_explicit` (+4 more)
- `apps/api/tests/unit/maps/test_map_render_capabilities.py` (primary, 2 tests): `test_supported_finite_profiles_have_explicit_permission_and_no_edit`, `test_latitude86_is_readable_but_never_advertised_as_renderable`
- `apps/api/tests/unit/maps/test_map_service.py` (cross-ref from E02, 6 tests): `test_unsaved_get_is_virtual_and_first_empty_save_commits_once`, `test_lost_ack_replays_original_receipt_even_after_newer_save`, `test_operation_reuse_changed_intent_or_foreign_actor_never_returns_receipt`, `test_replay_and_receipt_lookup_recheck_current_edit_authority`, `test_new_stale_and_noop_operations_preserve_prior_revision` (+1 more)
- `apps/api/tests/unit/maps/test_map_viewport.py` (primary, 8 tests): `test_reject_nonfinite_wrapped_or_degenerate_viewport`, `test_viewport_limits_are_not_source_completeness`, `test_position_budget_keeps_prefix_and_never_skips_offending_feature`, `test_entire_json_envelope_counts_towards_byte_budget`, `test_empty_viewport_is_complete_and_explicitly_empty` (+3 more)
- `apps/api/tests/unit/spatial/test_tiles_generator.py` (primary, 3 tests): `test_wgs84_and_scale`, `test_bad_height`, `test_bad_extent`
- `apps/api/tests/unit/spatial/test_tiles_generator_math.py` (primary, 3 tests): `test_wgs84_and_scale`, `test_bad_height`, `test_bad_extent`
- `apps/api/tests/unit/test_processing_domain.py` (cross-ref from E08, 3 tests): `test_buffer_valid_distance`, `test_invalid_buffer_parameter_is_domain_error`, `test_unknown_parameter_rejected`

## 8. Edge Cases & Parity Risks

- No shared private tile cache across tenants or revoked permissions; tile/manifest routes must verify project ACL.
- Malformed 3D Tiles archives, path traversal in {uri:path}, or invalid boundingVolume/geometricError trees must be rejected.
- Edit-to-tile invalidation and bounded zoom/geometry payloads on 30k/100k/1M feature layers.
- Cross-engine convergence across 3D tilesets (E05), map_viewport (E02), MVT /pro/tiles (E08), and raster PNG tiles (E09).

## 9. E05-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **7** live runtime operations in `E05` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E05` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **6** owned migrations, **10** source files, and **18** pytest files (**72** test IDs) linked in `contracts/engines/e05.json`.
- [ ] Next phase (`E05-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
