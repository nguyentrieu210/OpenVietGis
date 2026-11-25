# E15 Inventory & Contract Evidence — House numbering and land asset module

- **Spec**: `specs/064-rust-platform/engines/e15-house-numbering.md` (Parent refs: R014, R019, R051)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e15.json`
- **AC1 Status**: `verified` (`E15-T01`)
- **Architecture Candidate**: Isolated Rust module behind stable SDK with zero Core reverse dependency; retain frontend module ABI and land_asset reference module contract.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 72 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 0 |
| Client SDK / web API paths | 0 |
| Connectors / adapters / services | 64 |
| Migrations | 11 |
| Infra & operational scripts | 0 |
| Tracked source files | 79 |
| Mapped test files | 38 |
| Mapped test IDs | 101 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 45 |
| `retain` | 34 |
| `replace` | 0 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{"organization+project": 72}`
- **routes_with_project_access_guard**: 72
- **routes_with_csrf_enforcement**: 45
- **core_permissions_referenced**: none
- **extension_permissions_referenced**: none
- **official_module_isolation**: All 72 routes require project_access_guard + require_official_activation(house_numbering) + module-specific ext.house_numbering.* permissions + RLS private_access gate.

## Mounted Runtime HTTP / SSE Routes (72)

| Method | Path | Transport | Handler | Tenant Scope | Guard / Auth | Classification |
|---|---|---|---|---|---|---|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/areas` | `HTTP` | `modules.house_numbering.runtime.list_areas` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/areas` | `HTTP` | `modules.house_numbering.runtime.create_area` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/areas/{area_id}` | `HTTP` | `modules.house_numbering.runtime.delete_area` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/areas/{area_id}` | `HTTP` | `modules.house_numbering.runtime.update_area` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/assignments` | `HTTP` | `modules.house_numbering.runtime.list_assignments` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/assignments` | `HTTP` | `modules.house_numbering.runtime.create_assignment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/assignments/{assignment_id}` | `HTTP` | `modules.house_numbering.runtime.reassign_assignment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/apply` | `HTTP` | `modules.house_numbering.runtime.apply_building_assignment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/audit` | `HTTP` | `modules.house_numbering.runtime.audit_building_assignment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/catalog-recovery/apply` | `HTTP` | `modules.house_numbering.runtime.apply_catalog_recovery` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/catalog-recovery/preview` | `HTTP` | `modules.house_numbering.runtime.preview_catalog_recovery` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/correction/apply` | `HTTP` | `modules.house_numbering.runtime.apply_cluster_correction` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/correction/preview` | `HTTP` | `modules.house_numbering.runtime.preview_cluster_correction` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/prefix-recovery/preview` | `HTTP` | `modules.house_numbering.runtime.preview_prefix_recovery` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/building-assignment/preview` | `HTTP` | `modules.house_numbering.runtime.preview_building_assignment` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings` | `HTTP` | `modules.house_numbering.runtime.list_buildings` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings` | `HTTP` | `modules.house_numbering.runtime.create_building` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/lineage` | `HTTP` | `modules.house_numbering.runtime.record_building_lineage` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}` | `HTTP` | `modules.house_numbering.runtime.retire_building` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}` | `HTTP` | `modules.house_numbering.runtime.update_building` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}/documents` | `HTTP` | `modules.house_numbering.runtime.document_history` | `organization+project` | project_access_guard | `migrate` |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}/eligibility` | `HTTP` | `modules.house_numbering.runtime.set_eligibility` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}/lineage` | `HTTP` | `modules.house_numbering.runtime.list_building_lineage` | `organization+project` | project_access_guard | `migrate` |
| `PATCH` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}/plaque` | `HTTP` | `modules.house_numbering.runtime.set_building_plaque` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/buildings/{building_id}/private-households` | `HTTP` | `modules.house_numbering.runtime.read_private_households` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/documents` | `HTTP` | `modules.house_numbering.runtime.issue_document` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/documents/batch` | `HTTP` | `modules.house_numbering.runtime.issue_document_batch` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/documents/{document_id}/download` | `HTTP` | `modules.house_numbering.runtime.download_document` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/exports` | `HTTP` | `modules.house_numbering.runtime.create_export` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/exports/{artifact_id}/download` | `HTTP` | `modules.house_numbering.runtime.download_export` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/geometry-reviews` | `HTTP` | `modules.house_numbering.runtime.get_geometry_reviews` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/geometry-reviews/{review_id}/addresses` | `HTTP` | `modules.house_numbering.runtime.get_geometry_addresses` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/geometry-reviews/{review_id}/addresses/batch` | `HTTP` | `modules.house_numbering.runtime.verify_geometry_addresses_batch` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/geometry-reviews/{review_id}/addresses/{building_id}` | `HTTP` | `modules.house_numbering.runtime.verify_geometry_address` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/geometry-reviews/{review_id}/verified-links` | `HTTP` | `modules.house_numbering.runtime.get_verified_links` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/geometry-reviews/{review_id}/verified-links` | `HTTP` | `modules.house_numbering.runtime.apply_verified_links` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/imports` | `HTTP` | `modules.house_numbering.runtime.list_imports` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/imports` | `HTTP` | `modules.house_numbering.runtime.import_evidence` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/inspect` | `HTTP` | `modules.house_numbering.runtime.inspect_location` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/integrations/v1/addresses` | `HTTP` | `modules.house_numbering.runtime.integration_address_lookup` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/numbering-targets` | `HTTP` | `modules.house_numbering.runtime.numbering_targets` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/previews` | `HTTP` | `modules.house_numbering.runtime.preview_numbering` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/previews/{preview_id}/apply` | `HTTP` | `modules.house_numbering.runtime.apply_numbering` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/private-households` | `HTTP` | `modules.house_numbering.runtime.upsert_private_household` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/progress` | `HTTP` | `modules.house_numbering.runtime.progress_summary` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/progress/report.xlsx` | `HTTP` | `modules.house_numbering.runtime.progress_report` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/proposals` | `HTTP` | `modules.house_numbering.runtime.list_proposals` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/proposals` | `HTTP` | `modules.house_numbering.runtime.create_proposal` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/proposals/{proposal_id}` | `HTTP` | `modules.house_numbering.runtime.get_proposal` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/proposals/{proposal_id}/packets/{kind}` | `HTTP` | `modules.house_numbering.runtime.download_proposal_packet` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/proposals/{proposal_id}/transition` | `HTTP` | `modules.house_numbering.runtime.transition_proposal` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/reconciliation/cases` | `HTTP` | `modules.house_numbering.runtime.list_reconciliation_cases` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/reconciliation/cases/{case_id}/resolve` | `HTTP` | `modules.house_numbering.runtime.resolve_reconciliation_case` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/reconciliation/evidence/{evidence_id}/suggest` | `HTTP` | `modules.house_numbering.runtime.suggest_evidence_match` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/reconciliation/rebuild` | `HTTP` | `modules.house_numbering.runtime.rebuild_reconciliation` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/route-import/apply` | `HTTP` | `modules.house_numbering.runtime.apply_route_import` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/route-import/preview` | `HTTP` | `modules.house_numbering.runtime.preview_route_import` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/route-import/stage-geometry` | `HTTP` | `modules.house_numbering.runtime.stage_route_geometry` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/routes` | `HTTP` | `modules.house_numbering.runtime.list_routes` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/routes` | `HTTP` | `modules.house_numbering.runtime.create_route` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/routes/{route_id}` | `HTTP` | `modules.house_numbering.runtime.delete_route` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/routes/{route_id}` | `HTTP` | `modules.house_numbering.runtime.update_route` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/rules` | `HTTP` | `modules.house_numbering.runtime.list_rules` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/rules` | `HTTP` | `modules.house_numbering.runtime.create_rule` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/search` | `HTTP` | `modules.house_numbering.runtime.search_house_numbers` | `organization+project` | project_access_guard | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/survey-signoffs` | `HTTP` | `modules.house_numbering.runtime.list_survey_signoffs` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/survey-signoffs` | `HTTP` | `modules.house_numbering.runtime.sign_survey_scope` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/survey-signoffs/{signoff_id}/revoke` | `HTTP` | `modules.house_numbering.runtime.revoke_survey_signoff` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/survey/photos` | `HTTP` | `modules.house_numbering.runtime.upload_survey_photo` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/survey/sync` | `HTTP` | `modules.house_numbering.runtime.sync_surveys` | `organization+project` | project_access_guard, CSRF | `migrate` |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/surveys` | `HTTP` | `modules.house_numbering.runtime.list_surveys` | `organization+project` | project_access_guard | `migrate` |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/surveys/{survey_id}/review` | `HTTP` | `modules.house_numbering.runtime.review_survey` | `organization+project` | project_access_guard, CSRF | `migrate` |

## Component, Connector, Module & Operation Contracts

### `modules/house_numbering` (backend=`migrate`, frontend=`retain`)
- Version: `2.0.0`, API compat: `1.x`
- Permissions (8): `ext.house_numbering.read`, `ext.house_numbering.edit`, `ext.house_numbering.assign`, `ext.house_numbering.approve`, `ext.house_numbering.survey`, `ext.house_numbering.private_read`, `ext.house_numbering.export_sensitive`, `ext.house_numbering.manage`
- Commands (5): `inspect_location`, `preview_numbering`, `apply_numbering`, `submit_proposal`, `approve_proposal`
- Capabilities (13): `area_tree`, `spatial_house_registry`, `source_import`, `location_inspection`, `numbering_rules`, `bulk_numbering`, `source_reconciliation`, `approval_workflow`, `field_survey`, `progress_tracking`, `inspection_documents`, `search_and_handover`, `protected_household_data`
- Backend service/model modules: 31
- Frontend modules: 32

#### Migrations (`modules/house_numbering/migrations/versions/`)
| File | Revision | Down Revision | Tables Created | Indexes Created |
|---|---|---|---|---|
| `modules/house_numbering/migrations/versions/0001_house_numbering.py` | `hn_0001_house_numbering` | `None` | house_numbering.access_log, house_numbering.address_numbers, house_numbering.areas, house_numbering.assignments, house_numbering.buildings, house_numbering.documents, house_numbering.evidence, house_numbering.import_batches, house_numbering.previews, house_numbering.proposal_items, house_numbering.proposals, house_numbering.reconcile_cases, house_numbering.restricted_households, house_numbering.routes, house_numbering.rules, house_numbering.surveys | uq_hn_official_number |
| `modules/house_numbering/migrations/versions/0002_private_access_gate.py` | `hn_0002_private_access_gate` | `hn_0001_house_numbering` | — | — |
| `modules/house_numbering/migrations/versions/0003_performance_indexes.py` | `hn_0003_performance_indexes` | `hn_0002_private_access_gate` | — | ix_hn_buildings_parcel_active, ix_hn_buildings_route_active, ix_hn_evidence_building_recent, ix_hn_evidence_number, ix_hn_numbers_building_current, ix_hn_numbers_text_current |
| `modules/house_numbering/migrations/versions/0004_survey_gps_delegation.py` | `hn_0004_survey_gps_delegation` | `hn_0003_performance_indexes` | — | — |
| `modules/house_numbering/migrations/versions/0005_building_lineage_plaque.py` | `hn_0005_building_lineage_plaque` | `hn_0004_survey_gps_delegation` | house_numbering.building_lineage | ix_hn_lineage_predecessor, ix_hn_lineage_successor |
| `modules/house_numbering/migrations/versions/0006_route_segments.py` | `hn_0006_route_segments` | `hn_0005_building_lineage_plaque` | — | — |
| `modules/house_numbering/migrations/versions/0007_route_lines_fast_path.py` | `hn_0007_route_lines_fast_path` | `hn_0006_route_segments` | — | — |
| `modules/house_numbering/migrations/versions/0008_reviewed_route_aliases.py` | `hn_0008_reviewed_route_aliases` | `hn_0007_route_lines_fast_path` | — | — |
| `modules/house_numbering/migrations/versions/0009_geometry_review.py` | `hn_0009_geometry_review` | `hn_0008_reviewed_route_aliases` | house_numbering.geometry_reviews | — |
| `modules/house_numbering/migrations/versions/0010_address_verification.py` | `hn_0010_address_verification` | `hn_0009_geometry_review` | house_numbering.geometry_address_reviews | — |
| `modules/house_numbering/migrations/versions/0011_survey_signoffs.py` | `hn_0011_survey_signoffs` | `hn_0010_address_verification` | house_numbering.survey_signoffs | uq_hn_survey_signoff_active |

### `modules/land_asset` (`retain`)
- Version: `1.0.0`, Permissions: `ext.land_asset.read`, `ext.land_asset.inspect`
- Commands: `record_asset_inspection`
- Workflow: `modules/land_asset/workflow.py` — functions: `load_manifest`, `build_inspection_payload`

## Mapped Existing Test Coverage (38 files, 101 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `modules/house_numbering/frontend/api.test.ts` | `unit` | 2 | `keeps all calls project scoped`, `uses multipart import and scoped artifact URLs` |
| `modules/house_numbering/frontend/progress.test.ts` | `unit` | 3 | `counts a point as pending only when it is not khop and nobody handled it`, `reports done / pending / floor percent`, `a hamlet sign-off covers every road; a road sign-off covers only that road` |
| `modules/house_numbering/frontend/search.test.ts` | `unit` | 4 | `puts the exact house number first and never matches inside another number`, `ignores accents, case and separators`, `ranks a whole word above a prefix`, `returns nothing for an empty query` |
| `modules/house_numbering/frontend/tree.test.ts` | `unit` | 1 | `groups points Ấp → Đường → Hẻm with counts and pending` |
| `modules/house_numbering/tests/integration/test_batch_numbering.py` | `integration` | 1 | `test_batch_route_tree` |
| `modules/house_numbering/tests/integration/test_building_assignment_api.py` | `integration` | 1 | `test_assignment_prefers_address_over_nearest_and_never_overwrites` |
| `modules/house_numbering/tests/integration/test_catalog_recovery.py` | `integration` | 1 | `test_catalog_reviewed_alias_is_code_scoped_and_stale_safe` |
| `modules/house_numbering/tests/integration/test_cluster_correction.py` | `integration` | 1 | `test_cluster_correction_protects_distance_source_and_stale_preview` |
| `modules/house_numbering/tests/integration/test_house_numbering_batch_100.py` | `integration` | 1 | `test_document_batch_100_synthetic_buildings` |
| `modules/house_numbering/tests/integration/test_house_numbering_edge_acceptance.py` | `integration` | 6 | `test_lifecycle_scope_and_land_asset_coexistence`, `test_geometry_preview_and_proposal_guards`, `test_offline_batch_of_50_replays_and_conflicts`, `test_gps_quality_and_delegated_submission` (+2 more) |
| `modules/house_numbering/tests/integration/test_house_numbering_runtime.py` | `integration` | 1 | `test_house_numbering_us1_us3_real_postgis` |
| `modules/house_numbering/tests/integration/test_house_numbering_us4_us5.py` | `integration` | 2 | `test_house_numbering_us4_import_reconcile_and_us5_offline_progress`, `test_house_numbering_import_format_matrix` |
| `modules/house_numbering/tests/integration/test_house_numbering_us6_us7.py` | `integration` | 1 | `test_house_numbering_us6_documents_and_us7_search_exports_privacy` |
| `modules/house_numbering/tests/integration/test_integration_address_lookup.py` | `integration` | 1 | `test_approved_addresses_only_and_credential_scope` |
| `modules/house_numbering/tests/integration/test_official_runtime_contract.py` | `integration` | 1 | `test_official_runtime_contract_is_pinned` |
| `modules/house_numbering/tests/integration/test_prefix_recovery.py` | `integration` | 1 | `test_prefix_recovery` |
| `modules/house_numbering/tests/integration/test_route_recovery.py` | `integration` | 1 | `test_review_parent_suggestions_and_partial_apply` |
| `modules/house_numbering/tests/integration/test_route_segments.py` | `integration` | 1 | `test_grouped_segments_alpha_hierarchy_and_alias` |
| `modules/house_numbering/tests/integration/test_route_topology_recovery.py` | `integration` | 1 | `test_recovery_groups_and_missing_parent_using_existing_gis_geometry` |
| `modules/house_numbering/tests/integration/test_route_tree_import.py` | `integration` | 1 | `test_route_hierarchy_preview_apply_and_reimport` |
| `modules/house_numbering/tests/integration/test_survey_conflict_review.py` | `integration` | 1 | `test_survey_conflict_breakdown_and_guard` |
| `modules/house_numbering/tests/integration/test_survey_signoff.py` | `integration` | 1 | `test_signoff_requires_full_reconciliation_and_locks_points` |
| `modules/house_numbering/tests/performance/run_benchmark.py` | `performance` | 1 | `script_main` |
| `modules/house_numbering/tests/recovery/run_recovery.py` | `recovery` | 1 | `script_main` |
| `modules/house_numbering/tests/test_address_batch.py` | `unit` | 4 | `test_normalized_similar_fields_share_group`, `test_different_source_code_cannot_share_group`, `test_conflict_and_existing_decisions_cannot_batch`, `test_missing_road_requires_exact_prefix` |
| `modules/house_numbering/tests/test_address_prefix.py` | `unit` | 4 | `test_prefix_valid`, `test_prefix_invalid`, `test_child_prefix_and_number`, `test_opt_in_grouping_holds_aliases_and_rewrites_parent` |
| `modules/house_numbering/tests/test_address_suggestions.py` | `unit` | 5 | `test_missing_prefix_is_not_street_frontage`, `test_exact_child_is_ranked_above_staged_parent`, `test_wrong_road_is_conflict_even_when_close`, `test_duplicate_or_draft_not_strong` (+1 more) |
| `modules/house_numbering/tests/test_batch_numbering.py` | `unit` | 2 | `test_reserved_before_anchor_and_tied_chainage`, `test_existing_evidence_conflicting_sequence_is_reviewed` |
| `modules/house_numbering/tests/test_building_assignment.py` | `unit` | 5 | `test_address_beats_nearest_and_normalizes_vietnamese`, `test_missing_alley_is_not_equivalent_to_explicit_main_road`, `test_duplicate_road_names_and_code_conflict_require_review`, `test_geometry_only_not_high_confidence_and_locked_building_blocked` (+1 more) |
| `modules/house_numbering/tests/test_numbering_audit.py` | `unit` | 7 | `test_gap_is_only_information_and_reservations_count_as_occupied`, `test_collision_and_wrong_parity_are_blocking`, `test_wrong_order_and_tied_chainage_are_blocking`, `test_wrong_prefix_and_new_candidate_occupied_are_blocking` (+3 more) |
| `modules/house_numbering/tests/test_proposal_packet.py` | `unit` | 5 | `test_decision_pdf_is_explicitly_draft_and_bounded`, `test_register_contains_all_rows_and_valid_xlsx`, `test_plate_qr_requires_approval_and_valid_snapshot`, `test_appendix_is_separate_and_printable` (+1 more) |
| `modules/house_numbering/tests/test_route_import.py` | `unit` | 4 | `test_multilevel_out_of_order_and_local_prefix`, `test_full_prefix_parent_and_duplicate_checks`, `test_cycle_or_missing_parent_block`, `test_existing_parent_by_saved_code_and_idempotency` |
| `modules/house_numbering/tests/test_route_recovery.py` | `unit` | 4 | `test_labels_keep_full_alphanumeric_prefix_and_immediate_parent`, `test_duplicate_paths_block_both_candidates_and_children`, `test_partial_selection_requires_ancestors_and_blocks_error_rows`, `test_survey_conflicts_and_existing_links_block_review` |
| `modules/house_numbering/tests/test_route_topology.py` | `unit` | 2 | `test_missing_ancestor_levels_are_distinct_and_aggregated`, `test_explicit_merge_preserves_source_codes_and_disputed_parent` |
| `modules/house_numbering/tests/test_verified_links.py` | `unit` | 4 | `test_verified_current_is_ready`, `test_other_address_never_linked`, `test_existing_route_and_protected_number`, `test_stale_evidence_and_mismatched_address` |
| `tests/e2e/house-numbering.spec.ts` | `playwright_e2e` | 1 | `House Numbering official module lifecycle and seven UI journeys` |
| `tests/modules/test_house_numbering_domain.py` | `unit` | 17 | `NumberingTests::test_gap_before_suffix`, `NumberingTests::test_suffix_and_existing`, `NumberingTests::test_suffix_dash`, `NumberingTests::test_alley` (+13 more) |
| `tests/modules/test_reference_modules.py` | `unit` | 1 | `test_two_independent_reference_modules_validate_core_extension_model` |

## E15-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e15.json`.
