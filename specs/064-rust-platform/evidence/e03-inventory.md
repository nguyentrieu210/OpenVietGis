# E03 Inventory & Contract Evidence — Geometry, editing, topology

- **Task**: `E03-T01` (Acceptance Criterion: `E03-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R006
- **Child Spec**: `specs/064-rust-platform/engines/e03-geometry-editing.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e03.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: GEOS plus Rust kernels; server-side transaction/revision/idempotency authority.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 5 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 5 |
| Classification: `replace` | 0 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 1 |
| Mapped & Triaged Source Files | 4 |
| Mapped Pytest Files | 11 |
| Mapped Pytest Test Case IDs | 33 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/changesets/{changeset_id}` | `get_changeset_api_v1_organizations__organization_id__projects__project_id__changesets__changeset_id__get` | `get_changeset` (`editing`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 7 files (23 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/changesets/{changeset_id}/revert` | `revert_changeset_api_v1_organizations__organization_id__projects__project_id__changesets__changeset_id__revert_post` | `revert_changeset` (`editing`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | op-id | 7 files (23 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/editing/snap` | `compute_snap_api_v1_organizations__organization_id__projects__project_id__editing_snap_post` | `compute_snap` (`editing`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:allow, FOR UPDATE) | — | 7 files (23 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/sources/{source_id}/changesets` | `list_changesets_api_v1_organizations__organization_id__projects__project_id__sources__source_id__changesets_get` | `list_changesets` (`editing`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 7 files (23 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/sources/{source_id}/changesets` | `commit_changeset_api_v1_organizations__organization_id__projects__project_id__sources__source_id__changesets_post` | `commit_changeset` (`editing`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:allow, FOR UPDATE) | rev-pin, op-id | 7 files (23 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "sources": [
    "apps/api/openvietgis_api/domains/editing/domain.py",
    "apps/api/openvietgis_api/adapters/http/spatial.py"
  ],
  "engine": "E03",
  "changeset_op_types": [
    "create",
    "update",
    "delete"
  ],
  "geometry_preview_actions": [
    "split",
    "merge"
  ],
  "snapping_endpoint": "/api/v1/organizations/{organization_id}/projects/{project_id}/editing/snap"
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0009_editing_changesets` | `0008_import_export` | `apps/api/migrations/versions/0009_editing_changesets.py` | yes | `retain` | R006 Editing & Change History schema with RLS and immutable changesets. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E03`: **4**

- `apps/api/openvietgis_api/adapters/http/editing.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/editing_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/editing/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/editing/domain.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Validates edits against E02 layer schema, geometry family, explicit CRS, and catalog/content revisions.
- **E05**: Committed changesets must invalidate viewport/MVT tile caches.
- **E11**: Edits respect feature edit leases and publish realtime change events.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **11 pytest files** containing **33 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/contract/test_spatial_changes.py` (primary, 4 tests): `test_empty_history_default_bound_and_current_scope`, `test_history_preserves_actual_evidence_retirement_and_complete_keyset`, `test_history_requires_read_permission_before_repository_and_reauthorizes`, `test_invalid_history_queries_are_sanitized`
- `apps/api/tests/integration/m2/test_processing_integrity.py` (cross-ref from E08, 4 tests): `test_stale_job_fails_durably_without_repinning`, `test_clip_retains_family_and_empty_output`, `test_atomic_rollback_on_publication_failure_and_consumers`, `test_idempotency_and_null_dissolve_groups`
- `apps/api/tests/integration/m2/test_r006_r009_real.py` (primary, 4 tests): `test_r006_editing_changesets_conflicts_validation_and_revert`, `test_r006_noop_and_snap_require_current_edit_authority`, `test_r006_concurrent_identical_save_returns_one_receipt`, `test_r007_r008_r009_query_processing_and_cartography`
- `apps/api/tests/integration/m2/test_r006_schema_pins.py` (primary, 1 tests): `test_stale_schema_fails_without_publication_and_pinned_replay_is_exact`
- `apps/api/tests/integration/spatial/test_change_evidence.py` (primary, 6 tests): `test_project_history_exact_wire_values_keyset_and_retired_records`, `test_service_actor_snapshot_immutable_after_credential_and_principal_revoke`, `test_noop_stale_denied_or_dependency_conflict_never_append_evidence`, `test_failed_evidence_append_rolls_back_mutation_and_audit`, `test_concurrent_compare_and_set_has_one_winner_and_one_change` (+1 more)
- `apps/api/tests/integration/spatial/test_manual_editor.py` (cross-ref from E02, 1 tests): `test_manual_editor`
- `apps/api/tests/integration/spatial/test_processing_models.py` (cross-ref from E08, 1 tests): `test_saved_processing_graph_and_real_chain`
- `apps/api/tests/integration/spatial/test_professional.py` (cross-ref from E08, 2 tests): `test_pro_tools`, `test_projected_native_tile_is_revision_pinned`
- `apps/api/tests/unit/test_editing_domain.py` (primary, 4 tests): `test_editing_decimal_canonicalizes_before_publish`, `test_editing_timestamp_is_a_canonical_utc_string`, `test_invalid_logical_attribute_rejected_before_commit`, `test_editing_required_null_and_unknown_fields_rejected`
- `apps/api/tests/unit/test_editing_replay_and_rebase.py` (primary, 3 tests): `test_digest_is_exact_and_key_order_independent`, `test_draft_is_bounded_and_typed`, `test_rebase_is_advisory_and_detects_existence_conflicts`
- `apps/api/tests/unit/test_processing_domain.py` (cross-ref from E08, 3 tests): `test_buffer_valid_distance`, `test_invalid_buffer_parameter_is_domain_error`, `test_unknown_parameter_rejected`

## 8. Edge Cases & Parity Risks

- Stale base_content_revision or concurrent edits must return conflict without corrupting change history.
- Duplicate operation_id retries must be idempotent; internal operation_id prefixes rejected on public endpoints.
- Invalid geometry, non-2D Z/M coordinates, or geometry-family mismatch must be rejected (422 UNSUPPORTED_GEOMETRY / UNSUPPORTED_DIMENSIONS).
- SQLSTATE OV409 database lock guard (e.g. module lock trigger) must surface 409 DATA_LOCKED.
- Reverting a changeset must preserve audit attribution, before/after snapshots, and transactional atomicity.

## 9. E03-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **5** live runtime operations in `E03` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E03` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **1** owned migrations, **4** source files, and **11** pytest files (**33** test IDs) linked in `contracts/engines/e03.json`.
- [ ] Next phase (`E03-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
