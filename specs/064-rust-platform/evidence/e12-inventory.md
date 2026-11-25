# E12 Inventory & Contract Evidence — QGIS, external sources, integration

- **Spec**: `specs/064-rust-platform/engines/e12-connectors-external.md` (Parent refs: R010, R027)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e12.json`
- **AC1 Status**: `verified` (`E12-T01`)
- **Architecture Candidate**: Keep PyQGIS and desktop connector adapters; migrate backend routes and SSRF/provider transport to Rust.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 26 |
| SSE streaming endpoints | 1 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 21 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 27 |
| Mapped test files | 18 |
| Mapped test IDs | 96 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 16 |
| `retain` | 11 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"organization": 2, "organization+project": 24}`
- **routes_with_project_access_guard**: 26
- **routes_with_csrf_enforcement**: 14
- **core_permissions_referenced**: `PROJECT_EDIT`, `PROJECT_READ`
- **extension_permissions_referenced**: none
- **provider_transport_guard**: Public IPv4 SSRF blocklist, bounded byte/range fetch, strong ETag check.
- **qgis_connector_auth**: Browser OIDC pairing or scoped Bearer token; secret redaction in logs/errors.

## Mounted Runtime HTTP / SSE Routes (26)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/organizations/{organization_id}/external-sources/allowlist` | `HTTP` | `external_sources.list_allowlisted_hosts` | `organization` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/external-sources/allowlist` | `HTTP` | `external_sources.register_allowlisted_host` | `organization` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/external-sources` | `HTTP` | `external_sources.list_external_connections` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/external-sources` | `HTTP` | `external_sources.create_external_connection` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/external-sources/{connection_id}/invalidate-cache` | `HTTP` | `external_sources.invalidate_external_cache` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/external-sources/{connection_id}/preview` | `HTTP` | `external_sources.preview_external_source` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/external-sources/{connection_id}/revoke` | `HTTP` | `external_sources.revoke_external_connection` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/external-sources/{connection_id}/viewport` | `HTTP` | `external_sources.preview_external_viewport` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings` | `HTTP` | `integration_bindings.list_bindings` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings` | `HTTP` | `integration_bindings.create_binding` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/candidates` | `HTTP` | `integration_bindings.list_binding_candidates` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/activate` | `HTTP` | `integration_bindings.activate_binding` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/conflicts` | `HTTP` | `integration_bindings.list_binding_conflicts` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/conflicts` | `HTTP` | `integration_bindings.record_binding_conflicts` | `organization+project` | project_access_guard, PROJECT_EDIT | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/conflicts/{conflict_id}/resolve` | `HTTP` | `integration_bindings.resolve_binding_conflict` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/lifecycle` | `HTTP` | `integration_bindings.change_binding_lifecycle` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/pairing-tokens` | `HTTP` | `integration_bindings.list_pairing_tokens` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/pairing-tokens` | `HTTP` | `integration_bindings.issue_pairing_token` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings/{binding_id}/pairing-tokens/{token_id}/revoke` | `HTTP` | `integration_bindings.revoke_pairing_token` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/events` | `SSE` | `integration_bindings.stream_integration_spatial_events` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}` | `HTTP` | `integration_hub.read_integration_adapter_profile` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export` | `HTTP` | `integration_hub.export_integration_layer` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-preview` | `HTTP` | `integration_hub.preview_integration_layer_sync` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-push` | `HTTP` | `integration_hub.push_integration_layer_sync` | `organization+project` | project_access_guard, CSRF, PROJECT_EDIT | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/catalog` | `HTTP` | `integration_hub.read_integration_catalog` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/export-scopes` | `HTTP` | `integration_hub.get_export_scopes` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |

## Realtime Contracts (SSE & WebSocket)

### `SSE` `/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/events`
- Handler: `openvietgis_api.adapters.http.integration_bindings.stream_integration_spatial_events` (`apps/api/openvietgis_api/adapters/http/integration_bindings.py:973`)
- Classification: `migrate`
- Events: `ready`, `spatial_change`, `unavailable`
- PG NOTIFY channels: `ovg_team_collaboration`
- Cursor/replay params: none
- Revalidates access on stream: `True`


## Component, Connector, Module & Operation Contracts

### External Provider Transport (`migrate`)
- `apps/api/openvietgis_api/adapters/http/external_provider_transport.py` — functions: `fetch_provider_bytes`, `fetch_provider_range`
- Security boundary: Public IPv4 SSRF guard, bounded deadline/size, strong ETag verification for range/byte fetches.

### PyQGIS Connector (`retain`)
- Plugin metadata: `{"name": "OpenVietGIS Connector", "qgisMinimumVersion": "3.34", "version": "1.0.0", "description": "Connect QGIS to OpenVietGIS maps, edit layers and save safely to Web; GeoPackage recovery under Advanced.", "about": "OpenVietGIS QGIS Connector 1.0. Project-scoped HTTPS service credentials, explicit project.read for session and export, project.edit plus GIS ACL for writes. Local baseline comparison and server-managed realtime with durable changesets. Denied sync never deletes local edits. No internal PostGIS credentials.", "author": "OpenVietGIS", "email": "security@openvietgis.org", "category": "Web", "tags": "gis,openvietgis,geopackage,vn2000,manual-sync", "homepage": "https://openvietgis.cloud", "repository": "https://github.com/nguyentrieu210/OpenVietGis", "tracker": "https://github.com/nguyentrieu210/OpenVietGis/issues", "hasProcessingProvider": "no", "experimental": "True", "deprecated": "False"}`
- Connector modules: 8
- Consumed API paths (1):
  - `/api/v1/organizations/{config.organization_id}`

## Mapped Existing Test Coverage (18 files, 96 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py` | `integration` | 5 | `test_integration_hub_rejects_anonymous_and_cross_tenant_rls`, `test_integration_hub_service_principal_ceiling_and_revocation`, `test_integration_hub_gis_scoped_layer_acl_and_crs_preservation`, `test_integration_hub_autocad_dxf_export_and_crs_guard` (+1 more) |
| `apps/api/tests/integration/integration_hub/test_r027_bindings.py` | `integration` | 1 | `test_r027_draft_binding_is_scoped_and_non_executable` |
| `apps/api/tests/integration/integration_hub/test_r027_manual_workspace.py` | `integration` | 1 | `test_qgis_workspace_manual_roundtrip_real_http_postgis` |
| `apps/api/tests/integration/m3/test_r010_cog_viewport.py` | `integration` | 1 | `test_r010_cog_viewport_authorization_pixels_and_generation` |
| `apps/api/tests/integration/m3/test_r010_live_provider.py` | `integration` | 1 | `test_r010_provider_preview_real_auth_live_bytes_and_reopen` |
| `apps/api/tests/integration/m3/test_r010_r013_real.py` | `integration` | 2 | `test_r010_external_sources_ssrf_and_allowlist`, `test_r011_r012_r013_collaboration_cad_and_field_offline` |
| `apps/api/tests/integration/m3/test_r010_raster_provider.py` | `integration` | 1 | `test_r010_raster_profiles_protected_api` |
| `apps/api/tests/unit/integration_hub/test_qgis_plugin_client.py` | `unit` | 16 | `test_config_rejects_plain_http_for_non_localhost`, `test_config_allows_insecure_localhost_only_when_explicitly_enabled`, `test_config_rejects_embedded_credentials_and_unsafe_urls`, `test_config_rejects_invalid_uuid` (+12 more) |
| `apps/api/tests/unit/integration_hub/test_qgis_workspace.py` | `unit` | 15 | `test_initialize_creates_separate_durable_baseline_and_editable_workfile`, `test_non_conflicting_both_directions_merged_then_committed`, `test_concurrent_edit_blocks_both_directions_before_writes`, `test_deleted_feature_quarantined_not_recreated` (+11 more) |
| `apps/api/tests/unit/integration_hub/test_r027_pairing_code.py` | `unit` | 2 | `test_pairing_code_roundtrip`, `test_pairing_code_rejects_malformed` |
| `apps/api/tests/unit/test_external_cog_provider.py` | `unit` | 6 | `test_actual_cog_overview_and_native_window_pixels`, `test_profile_and_viewport_default_deny_no_egress`, `test_bad_range_or_version_denied`, `test_invalid_cog_directory_georeference_and_decompression_denied` (+2 more) |
| `apps/api/tests/unit/test_external_provider.py` | `unit` | 16 | `test_wms_real_preview_bytes_and_request_parameters`, `test_wfs_real_bounded_read_and_query`, `test_wfs_count_semantics`, `test_wfs_invalid_count_metadata_denied` (+12 more) |
| `apps/api/tests/unit/test_external_raster_provider.py` | `unit` | 7 | `test_fixed_approved_tile_real_pixel_proof`, `test_tile_policy_valid_and_invalid_coordinates_bounds_origin`, `test_xyz_metadata_mismatch_never_requests_tile`, `test_wmts_metadata_wrong_matrix_crs_scale_or_limits_denied` (+3 more) |
| `tools/smoke_qgis_native.py` | `smoke` | 1 | `script_main` |
| `tools/tests/test_package_qgis_plugin.py` | `unit` | 2 | `test_plugin_zip_installable_deterministic_and_no_secrets`, `test_plugin_zip_refuses_missing_source_and_unreviewed_metadata` |
| `tools/tests/test_qgis_connector_simple_ui.py` | `unit` | 3 | `test_simple_plugin_has_one_primary_open_save_flow`, `test_simple_plugin_preserves_edit_recovery_guards`, `test_download_and_version_are_consistent` |
| `tools/tests/test_qgis_desktop_recovery.py` | `unit` | 5 | `test_atomic_draft_and_project_isolation`, `test_three_way_merge_and_conflict`, `test_browser_handoff_state_origin_and_one_time`, `test_callback_does_not_report_success_when_profile_fails` (+1 more) |
| `tools/tests/test_qgis_managed_snapshots.py` | `unit` | 11 | `test_native_save_does_not_lose_diff`, `test_deletes_are_denied_by_default`, `test_global_cursor_is_not_advanced_by_write_receipt`, `test_native_geometry_wkt_preserves_projected_coordinates` (+7 more) |

## E12-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e12.json`.
