# E18 Inventory & Contract Evidence — Ops, deployment, backup, performance

- **Spec**: `specs/064-rust-platform/engines/e18-ops-release.md` (Parent refs: R015, R042, R043, R044, R045, R047)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e18.json`
- **AC1 Status**: `verified` (`E18-T01`)
- **Architecture Candidate**: Rust modular monolith with bounded resource budgets; replace API deploy/compose manifests while preserving rollback artifacts and retaining backup/release/quality scripts.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 8 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 3 |
| Migrations | 0 |
| Infra & operational scripts | 69 |
| Tracked source files | 69 |
| Mapped test files | 36 |
| Mapped test IDs | 149 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 5 |
| `retain` | 61 |
| `replace` | 3 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"public_or_catalog": 2, "actor_or_platform": 4, "organization": 2}`
- **routes_with_project_access_guard**: 0
- **routes_with_csrf_enforcement**: 1
- **core_permissions_referenced**: none
- **extension_permissions_referenced**: none

## Mounted Runtime HTTP / SSE Routes (8)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/health/live` | `HTTP` | `health.liveness` | `public_or_catalog` | public/health | `migrate` |
| `GET` | `/api/v1/health/ready` | `HTTP` | `health.readiness` | `public_or_catalog` | public/health | `migrate` |
| `GET` | `/api/v1/ops/health/live` | `HTTP` | `ops.ops_health_live` | `actor_or_platform` | public/health | `migrate` |
| `GET` | `/api/v1/ops/health/ready` | `HTTP` | `ops.ops_health_ready` | `actor_or_platform` | get_db_session | `migrate` |
| `GET` | `/api/v1/ops/metrics` | `HTTP` | `ops.ops_metrics` | `actor_or_platform` | get_db_session | `migrate` |
| `GET` | `/api/v1/ops/release-capabilities` | `HTTP` | `ops.ops_release_capabilities` | `actor_or_platform` | public/health | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/ops/entitlement` | `HTTP` | `ops.get_org_entitlement` | `organization` | authenticate_request, get_db_session | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/ops/entitlement` | `HTTP` | `ops.upsert_org_entitlement` | `organization` | CSRF | `migrate` |

## Component, Connector, Module & Operation Contracts

### Infrastructure Artifacts (35)
| File | Classification | Notes |
|---|---|---|
| `infra/cad-relay/relay.css` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/relay.html` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/relay.js` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/release.sha256` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/serve-test.py` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/stage-test.py` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/tests/fixture.dxf` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/tests/index.html` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/cad-relay/tests/smoke.cjs` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/compose.identity.yaml` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/compose.yaml` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/realm-openvietgis.json` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/messages/messages_en.properties` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/messages/messages_vi.properties` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/register.ftl` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/css/openvietgis-modal-v3.css` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/css/openvietgis.css` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/img/favicon.ico` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/img/ovg-mark.png` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/img/studio-map.svg` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/img/studio-mark.svg` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/js/account-switch-modal-v2.js` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/resources/js/account-switch.js` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/keycloak/themes/openvietgis/login/theme.properties` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/dev/postgres/001-runtime-role.sql` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/001-runtime-role.sh` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/Dockerfile.api` | `replace` | Add Rust API service build/compose/proxy profile; preserve legacy rollback artifacts. |
| `infra/prod/Dockerfile.postgres` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/Dockerfile.web` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/README.md` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/docker-compose.prod.yml` | `replace` | Add Rust API service build/compose/proxy profile; preserve legacy rollback artifacts. |
| `infra/prod/nginx.conf` | `replace` | Add Rust API service build/compose/proxy profile; preserve legacy rollback artifacts. |
| `infra/prod/openvietgis-webhook@.service.example` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/postgres/ensure-runtime-role.sh` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |
| `infra/prod/webhook.env.example` | `retain` | Retained database/Keycloak/dev/CAD-relay/webhook infrastructure. |

### Operational, Quality, Release & Backup Tools (34)
| Script | Category | Classification | Entry Functions / Classes |
|---|---|---|---|
| `tools/agent_kit.py` | `packaging_and_inventory` | `retain` | `git`, `prepare`, `describe`, `run_agent`, `main`, `AgentKitError` |
| `tools/agent_kit_v8i.py` | `packaging_and_inventory` | `retain` | `git`, `prepare`, `describe`, `run_agent`, `main`, `AgentKitError` |
| `tools/backup_restore.py` | `backup_and_recovery` | `retain` | `compute_artifact_manifest`, `backup_docker_postgres_and_artifacts`, `verify_isolated_restore_docker` |
| `tools/check_architecture.py` | `quality_security_gate` | `retain` | `references_vertical_module_code`, `runtime_source`, `frontend_boundary_violations`, `domain_framework_violations`, `find_violations` |
| `tools/check_contracts.py` | `quality_security_gate` | `retain` | `validate_contracts`, `main` |
| `tools/check_docs.py` | `quality_security_gate` | `retain` | `find_documentation_issues`, `main` |
| `tools/check_node_runtime.mjs` | `quality_security_gate` | `retain` | script |
| `tools/check_release_gate.py` | `release_gate` | `retain` | `release_gate_failures`, `main` |
| `tools/check_scope.py` | `quality_security_gate` | `retain` | `find_scope_violations`, `main` |
| `tools/check_secret_surfaces.py` | `quality_security_gate` | `retain` | `scan_text`, `scan_artifact`, `main`, `Finding`, `ArtifactError`, `_SafeParser` |
| `tools/check_self_host_fixture.py` | `quality_security_gate` | `retain` | `check_origin`, `check`, `main` |
| `tools/check_tenant_timing.py` | `quality_security_gate` | `retain` | `collect`, `main`, `TimingEvidenceError` |
| `tools/configure_keycloak_e2e_redirect.py` | `environment_and_migration` | `retain` | `configure`, `main` |
| `tools/control_kit_audit.py` | `quality_security_gate` | `retain` | `sha256`, `git`, `check_assets`, `strip_cloudflare_beacon`, `check_index_assets` |
| `tools/engine_inventory.py` | `packaging_and_inventory` | `retain` | `inventory`, `main` |
| `tools/generate_community_qa_fixtures.py` | `packaging_and_inventory` | `retain` | `generate` |
| `tools/local_candidate_gate.py` | `release_gate` | `retain` | `commands`, `local_failures`, `run`, `main` |
| `tools/marketplace_release.py` | `release_gate` | `retain` | `read_json`, `write_json`, `sign`, `main` |
| `tools/migrate_official_modules.py` | `environment_and_migration` | `retain` | `migrate_official_modules`, `main` |
| `tools/package_qgis_plugin.py` | `packaging_and_inventory` | `retain` | `package_qgis_plugin`, `main` |
| `tools/product_inventory.py` | `packaging_and_inventory` | `retain` | `git`, `matches`, `is_test`, `python_candidates`, `ui_candidates` |
| `tools/r015_extension_oracle.py` | `quality_security_gate` | `retain` | `fingerprint`, `seed`, `verify_http`, `verify_denied` |
| `tools/r015_synthetic_drill.py` | `quality_security_gate` | `retain` | `main` |
| `tools/recovery_authority.py` | `backup_and_recovery` | `retain` | `safe_human_authority`, `reconciliation_sql` |
| `tools/release_consolidation.py` | `release_gate` | `retain` | `git`, `hash_file`, `artifact_hashes`, `snapshot`, `candidate_manifest` |
| `tools/reset_dev.py` | `environment_and_migration` | `retain` | `assert_safe_target`, `main` |
| `tools/run_with_test_database.py` | `environment_and_migration` | `retain` | `main` |
| `tools/seed_identity_e2e.py` | `environment_and_migration` | `retain` | `assert_safe_seed_target`, `main` |
| `tools/smoke_org_chat_sse.py` | `smoke_and_benchmark_probe` | `retain` | `http`, `check`, `listen` |
| `tools/smoke_qgis_native.py` | `smoke_and_benchmark_probe` | `retain` | `main` |
| `tools/spike_r029_postgres_cursor.py` | `smoke_and_benchmark_probe` | `retain` | `query`, `start`, `send`, `finish`, `wait_for` |
| `tools/spike_r029_qgis_commit.py` | `smoke_and_benchmark_probe` | `retain` | `main` |
| `tools/test_database.py` | `environment_and_migration` | `retain` | `assert_safe_test_target`, `build_test_environment` |
| `tools/validate_engine_first.sh` | `release_gate` | `retain` | script |

## Mapped Existing Test Coverage (36 files, 149 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/api/tests/integration/m3/test_r015_ops_real.py` | `integration` | 2 | `test_r014_core_extension_platform_generic_contracts`, `test_r015_production_ops_health_metrics_and_entitlements` |
| `apps/api/tests/integration/test_health.py` | `integration` | 2 | `test_liveness_and_readiness_use_real_database`, `test_readiness_reports_database_unavailable` |
| `apps/api/tests/integration/test_migrations.py` | `integration` | 2 | `test_migrations_apply_from_zero_to_head`, `test_populated_extension_history_refuses_lossy_downgrade` |
| `apps/api/tests/integration/test_startup_config.py` | `integration` | 1 | `test_application_import_fails_fast_when_required_config_is_missing` |
| `apps/api/tests/performance/test_authorization_benchmark.py` | `performance` | 1 | `test_authorization_p95_dynamic_scope_and_next_request_revocation` |
| `apps/api/tests/performance/test_spatial_benchmark.py` | `performance` | 1 | `test_spatial_http_p95_10k_native_and_transformed` |
| `apps/api/tests/unit/test_config.py` | `unit` | 8 | `test_settings_require_security_and_database_configuration`, `test_settings_reject_unknown_environment`, `test_settings_reject_empty_database_url`, `test_runtime_url_must_be_explicit_and_distinct` (+4 more) |
| `apps/api/tests/unit/test_ops_release_gate.py` | `unit` | 7 | `test_release_inventory_is_not_production_certification`, `test_readiness_safe_probe_and_metric_certification_counts`, `test_missing_storage_is_not_created`, `test_symlink_storage_is_not_probed` (+3 more) |
| `apps/api/tests/unit/test_reset_safety.py` | `unit` | 5 | `test_accepts_only_documented_local_reset_databases`, `test_rejects_unsafe_targets`, `test_reset_rejects_actual_migration_destination_before_alembic`, `test_reset_accepts_distinct_credentials_for_same_local_target` (+1 more) |
| `tests/architecture/test_architecture.py` | `architecture` | 11 | `test_rejects_core_to_vertical_module_dependency`, `test_accepts_module_to_public_contract_dependency`, `test_core_validation_succeeds_when_modules_directory_is_absent`, `test_rejects_domain_to_adapter_dependency` (+7 more) |
| `tools/r015_synthetic_drill.py` | `unit` | 1 | `script_main` |
| `tools/tests/test_agent_kit.py` | `unit` | 4 | `AgentKitTests::test_create_five_isolated_worktrees_and_idempotence`, `AgentKitTests::test_dirty_source_is_rejected`, `AgentKitTests::test_nested_worktree_root_is_rejected`, `AgentKitTests::test_existing_unrelated_path_is_not_overwritten` |
| `tools/tests/test_backup_restore.py` | `unit` | 10 | `test_verified_artifact_copy_and_tamper`, `test_reject_symlink_and_traversal`, `test_inventory_includes_immutable_cad_versions`, `test_required_database_reference_cannot_be_missing` (+6 more) |
| `tools/tests/test_canonical_frontend_gate.py` | `unit` | 3 | `test_ci_validation_builds_canonical_production_frontend`, `test_canonical_frontend_has_build_script`, `test_ci_runs_built_workspace_and_mcp_regression` |
| `tools/tests/test_community_qa_fixtures.py` | `unit` | 2 | `test_community_qa_fixtures_manifest_and_checksums`, `test_community_qa_geojson_geometry_families` |
| `tools/tests/test_contracts.py` | `unit` | 2 | `test_contract_validation_covers_all_integrated_contracts`, `test_sdk_generator_covers_all_integrated_contracts` |
| `tools/tests/test_control_kit_audit.py` | `unit` | 4 | `ControlKitTests::test_release_asset_hashes`, `ControlKitTests::test_missing_or_escaping_assets`, `ControlKitTests::test_cloudflare_beacon_only_normalization`, `ControlKitTests::test_index_asset_references` |
| `tools/tests/test_docs.py` | `unit` | 1 | `test_required_documentation_navigation_is_complete` |
| `tools/tests/test_local_candidate_gate.py` | `unit` | 1 | `test_local_gate_rejects_missing_changed_and_wrong_candidate` |
| `tools/tests/test_managed_delta_contract.py` | `unit` | 3 | `test_aliases_same_cursor`, `test_reject_bad_envelope`, `test_reset_allows_rewind_without_changes` |
| `tools/tests/test_managed_snapshot_contract.py` | `unit` | 3 | `test_native_projected_coordinates_and_identity_preserved`, `test_reject_mixed_snapshot`, `test_reject_invalid_initial_page` |
| `tools/tests/test_package_qgis_plugin.py` | `unit` | 2 | `test_plugin_zip_installable_deterministic_and_no_secrets`, `test_plugin_zip_refuses_missing_source_and_unreviewed_metadata` |
| `tools/tests/test_product_inventory.py` | `unit` | 5 | `test_routes`, `test_mcp_decorator_and_wrapper`, `test_mcp_literal_registry`, `test_ui_candidates` (+1 more) |
| `tools/tests/test_qgis_connector_simple_ui.py` | `unit` | 3 | `test_simple_plugin_has_one_primary_open_save_flow`, `test_simple_plugin_preserves_edit_recovery_guards`, `test_download_and_version_are_consistent` |
| `tools/tests/test_qgis_desktop_recovery.py` | `unit` | 5 | `test_atomic_draft_and_project_isolation`, `test_three_way_merge_and_conflict`, `test_browser_handoff_state_origin_and_one_time`, `test_callback_does_not_report_success_when_profile_fails` (+1 more) |
| `tools/tests/test_qgis_managed_snapshots.py` | `unit` | 11 | `test_native_save_does_not_lose_diff`, `test_deletes_are_denied_by_default`, `test_global_cursor_is_not_advanced_by_write_receipt`, `test_native_geometry_wkt_preserves_projected_coordinates` (+7 more) |
| `tools/tests/test_recovery_authority.py` | `unit` | 7 | `test_default_is_deny_all`, `test_only_unchanged_human_authority_retained`, `test_changed_or_revoked_scope_does_not_preserve_access`, `test_missing_or_changed_identity_denied` (+3 more) |
| `tools/tests/test_release_consolidation.py` | `unit` | 5 | `test_artifact_set`, `test_connector_mismatch`, `test_missing_connector`, `test_symlink` (+1 more) |
| `tools/tests/test_release_gate.py` | `unit` | 4 | `test_absent_exact_candidate_acceptance_blocks_release`, `test_static_all_true_flags_do_not_pass_release_gate`, `test_duplicate_or_missing_ids_cannot_be_certified`, `test_candidate_and_scope_mismatch_block_release` |
| `tools/tests/test_scope.py` | `unit` | 8 | `test_scope_checker_accepts_product_neutral_runtime`, `test_scope_checker_rejects_gis_or_vertical_runtime_markers`, `test_accepted_renderer_stays_inside_map_engine_adapter`, `test_business_map_ui_cannot_import_renderer_types_or_dynamic_api` (+4 more) |
| `tools/tests/test_secret_surfaces.py` | `unit` | 11 | `test_detects_synthetic_credential_shapes`, `test_exact_canaries_are_found_without_key_names`, `test_does_not_flag_hashes_safe_prefixes_or_redacted_values`, `test_cli_scans_only_selected_artifacts_and_never_prints_value_or_path` (+7 more) |
| `tools/tests/test_seed_identity_e2e.py` | `unit` | 2 | `test_unsafe_seed_never_connects`, `test_dedicated_seed_target_allowed` |
| `tools/tests/test_self_host_fixture.py` | `unit` | 3 | `test_fixture_origin_rejects_nonisolated_targets`, `test_fixture_origin_normalizes_loopback`, `test_fixture_smoke_rejects_octet_stream_module_worker` |
| `tools/tests/test_tenant_timing.py` | `unit` | 2 | `test_collects_exact_passing_measurements`, `test_rejects_incomplete_or_invalid_evidence` |
| `tools/tests/test_test_database.py` | `unit` | 4 | `test_test_database_does_not_inherit_application_database`, `test_explicit_dedicated_test_database_takes_precedence`, `test_runner_applies_core_then_official_module_migrations`, `test_unsafe_test_target_stops_before_migration_or_child_process` |
| `tools/tests/test_validate_manifest.py` | `unit` | 3 | `test_root_validation_manifest_contains_all_mandatory_categories`, `test_ci_uses_the_dedicated_database_and_reference_identity_fixture`, `test_backend_ci_is_independent_of_javascript_dependency_install` |

## E18-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e18.json`.
