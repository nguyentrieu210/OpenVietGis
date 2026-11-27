# E14 Inventory & Contract Evidence — Plugin SDK, marketplace, modules

- **Spec**: `specs/064-rust-platform/engines/e14-plugin-marketplace.md` (Parent refs: R014)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e14.json`
- **AC1 Status**: `verified` (`E14-T01`)
- **Architecture Candidate**: Rust module SDK and marketplace/extension APIs; optional gated Wasmtime; retain partner JS SDK and webhook tooling.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 27 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 12 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 17 |
| Mapped test files | 26 |
| Mapped test IDs | 39 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 12 |
| `retain` | 5 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"organization": 5, "organization+project": 15, "public_or_catalog": 1, "actor_or_platform": 6}`
- **routes_with_project_access_guard**: 19
- **routes_with_csrf_enforcement**: 15
- **core_permissions_referenced**: `PROJECT_MANAGE`, `PROJECT_READ`
- **extension_permissions_referenced**: none

## Mounted Runtime HTTP / SSE Routes (27)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/organizations/{organization_id}/extensions/manifests` | `HTTP` | `extensions.list_extension_manifests` | `organization` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/extensions/manifests` | `HTTP` | `extensions.register_extension_manifest` | `organization` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/extensions/official` | `HTTP` | `extensions.list_official_extension_catalog` | `organization` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/extensions/official/{extension_key}/register` | `HTTP` | `extensions.register_official_extension_manifest` | `organization` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations` | `HTTP` | `extensions.list_project_activations` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations` | `HTTP` | `extensions.activate_project_extension` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations/{activation_id}/deactivate` | `HTTP` | `extensions.deactivate_project_extension` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations/{activation_id}/history` | `HTTP` | `extensions.list_extension_activation_history` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/activations/{activation_id}/transition` | `HTTP` | `extensions.transition_project_extension` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/disable` | `HTTP` | `extensions.disable_project_extension` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/portable-export` | `HTTP` | `extensions.export_extension_bundle` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/portable-import` | `HTTP` | `extensions.import_extension_bundle` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records` | `HTTP` | `extensions.list_extension_records` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records` | `HTTP` | `extensions.create_extension_record` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records/{record_id}` | `HTTP` | `extensions.update_extension_record` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/records/{record_id}/history` | `HTTP` | `extensions.list_extension_record_history` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/marketplace/catalog` | `HTTP` | `marketplace.marketplace_catalog` | `public_or_catalog` | public/health | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/marketplace/install/{publisher_id}/{extension_key}/{version}` | `HTTP` | `marketplace.install_marketplace_release` | `organization` | CSRF | `migrate` |
| `GET` | `/api/v1/developer/marketplace/me` | `HTTP` | `marketplace_developers.my_developer_state` | `actor_or_platform` | authenticate_request, get_db_session, my_developer_state | `migrate` |
| `POST` | `/api/v1/developer/marketplace/publisher` | `HTTP` | `marketplace_developers.register_publisher` | `actor_or_platform` | CSRF | `migrate` |
| `GET` | `/api/v1/developer/marketplace/review-queue` | `HTTP` | `marketplace_developers.review_queue` | `actor_or_platform` | authenticate_request, get_db_session | `migrate` |
| `POST` | `/api/v1/developer/marketplace/review-queue/{submission_id}/decision` | `HTTP` | `marketplace_developers.review` | `actor_or_platform` | CSRF | `migrate` |
| `POST` | `/api/v1/developer/marketplace/submissions` | `HTTP` | `marketplace_developers.submit_release` | `actor_or_platform` | CSRF | `migrate` |
| `POST` | `/api/v1/developer/marketplace/submissions/{submission_id}/withdraw` | `HTTP` | `marketplace_developers.withdraw` | `actor_or_platform` | CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/partner-contributions` | `HTTP` | `partner_platform.list_contributions` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/extensions/partner-contributions/{manifest_id}` | `HTTP` | `partner_platform.save_contributions` | `organization+project` | project_access_guard, CSRF, PROJECT_MANAGE | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/integrations/events` | `HTTP` | `partner_platform.list_events` | `organization+project` | project_access_guard, PROJECT_READ | `migrate` |

## Component, Connector, Module & Operation Contracts

### Official Module Discovery & Core Module SDK (`migrate`)
- `apps/api/openvietgis_api/adapters/http/official_modules.py` — functions: `discover_official_module_runtimes`, `official_manifest`
- `apps/api/openvietgis_api/module_sdk.py` — functions: `prepare_module_scope`, `require_official_activation`, `set_extension_private_access`, `put_module_artifact`, `read_module_artifact`

### Plugin SDK (`retain`)
- Partner JS SDK: `packages/plugin-sdk/openvietgis-partner.js` — functions: none
- `packages/plugin-sdk/marketplace_sign.py` — classes: [], functions: ['canonical', 'private_key', 'public_key', 'signed', 'write', 'main']
- `packages/plugin-sdk/native_webhook_worker.py` — classes: ['NativeWorker'], functions: ['public_target', 'pinned_post', 'main']
- `packages/plugin-sdk/webhook_relay.py` — classes: ['NoRedirect', 'Relay'], functions: ['https_url', 'signature', 'event_body', 'main']

## Mapped Existing Test Coverage (26 files, 39 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/api/migrations/versions/0017_extension_platform.py` | `unit` | 0 |  |
| `apps/api/migrations/versions/0019_extension_history.py` | `unit` | 0 |  |
| `apps/api/migrations/versions/0027_partner_platform.py` | `unit` | 0 |  |
| `apps/api/migrations/versions/0028_marketplace_developers.py` | `unit` | 0 |  |
| `apps/api/openvietgis_api/adapters/http/marketplace_developers.py` | `unit` | 0 |  |
| `apps/api/openvietgis_api/adapters/http/official_modules.py` | `unit` | 0 |  |
| `apps/api/openvietgis_api/adapters/http/partner_platform.py` | `unit` | 0 |  |
| `apps/api/tests/integration/m3/test_extension_lifecycle_security.py` | `integration` | 1 | `test_extension_lifecycle_requires_admin_and_project_manage` |
| `apps/api/tests/integration/m3/test_independent_lifecycle.py` | `integration` | 3 | `test_extension_lifecycle_scope_and_retained_records`, `test_legacy_manifest_mutation_block_preserves_authorized_retained_data`, `test_review_assignee_current_permission_and_immutable_decision` |
| `apps/api/tests/integration/m3/test_marketplace_developer_browser.py` | `integration` | 1 | `test_developer_portal_real_browser` |
| `apps/api/tests/integration/m3/test_marketplace_developers.py` | `integration` | 3 | `test_developer_self_service_and_review`, `test_cli_publisher_proof`, `test_standalone_signing_and_safe_catalog_merge` |
| `apps/api/tests/integration/m3/test_partner_platform.py` | `integration` | 2 | `test_partner_persistent_and_marketplace`, `test_marketplace_cli_roundtrip` |
| `apps/api/tests/integration/m3/test_partner_plugin.py` | `integration` | 1 | `test_partner_manifest_security_and_activation` |
| `apps/api/tests/unit/test_extension_history_domain.py` | `unit` | 6 | `test_schema_typed_nullable_and_finite`, `test_invalid_portable_attribute`, `test_canonical_digest_preserves_semantic_numbers_and_unicode`, `test_metadata_only_version_transition_checks_effective_contract` (+2 more) |
| `apps/api/tests/unit/test_extension_metadata_boundary.py` | `unit` | 5 | `test_legacy_metadata_cannot_be_activated`, `test_invalid_active_manifest_blocks_new_data_mutations`, `test_legacy_metadata_cannot_transition_to_new_version`, `test_extension_router_has_only_metadata_and_retained_record_endpoints` (+1 more) |
| `apps/api/tests/unit/test_official_module_discovery.py` | `unit` | 4 | `test_no_modules_needed`, `test_manifest_key_mismatch_rejected`, `test_shipped_modules_discovered`, `test_unsupported_manifest_fails_before_import` |
| `packages/plugin-sdk/marketplace_sign.py` | `unit` | 0 |  |
| `packages/plugin-sdk/test_native_webhook_worker.py` | `unit` | 3 | `NativeWorkerTest::test_checkpoint_queue_retry_order`, `NativeWorkerTest::test_deny_nonpublic_or_mixed_dns`, `NativeWorkerTest::test_replay` |
| `packages/plugin-sdk/test_webhook_relay.py` | `unit` | 2 | `RelayTest::test_checkpoint_durable_queue_retry_and_redaction`, `RelayTest::test_signature_and_empty_initial_feed` |
| `tests/e2e/extension-lifecycle.spec.ts` | `playwright_e2e` | 1 | `R014 actual browser activates, edits, exports and retains extension history` |
| `tests/modules/test_extension_portability.py` | `unit` | 2 | `test_linked_house_record_history_disabled_portability_and_scope`, `test_land_asset_atomic_copy_explicit_transition_and_replay` |
| `tests/modules/test_reference_modules.py` | `unit` | 1 | `test_two_independent_reference_modules_validate_core_extension_model` |
| `tools/marketplace_release.py` | `unit` | 0 |  |
| `tools/migrate_official_modules.py` | `unit` | 0 |  |
| `tools/r015_extension_oracle.py` | `unit` | 1 | `script_main` |
| `tools/tests/test_validate_manifest.py` | `unit` | 3 | `test_root_validation_manifest_contains_all_mandatory_categories`, `test_ci_uses_the_dedicated_database_and_reference_identity_fixture`, `test_backend_ci_is_independent_of_javascript_dependency_install` |

## E14-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e14.json`.
