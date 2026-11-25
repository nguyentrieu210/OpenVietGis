# E06 Inventory & Contract Evidence — Symbology, labels, layout

- **Task**: `E06-T01` (Acceptance Criterion: `E06-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R009
- **Child Spec**: `specs/064-rust-platform/engines/e06-cartography-layout.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e06.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: Retain canonical renderer/TS UI; Rust style/layout models and revision-pinned PNG/PDF export service.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 12 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 12 |
| Classification: `replace` | 0 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 2 |
| Mapped & Triaged Source Files | 5 |
| Mapped Pytest Files | 4 |
| Mapped Pytest Test Case IDs | 7 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer` | `list_designer_api_v1_organizations__organization_id__projects__project_id__cartography_designer_get` | `list_designer` (`layout_designer`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (6 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer` | `create_designer_api_v1_organizations__organization_id__projects__project_id__cartography_designer_post` | `create_designer` (`layout_designer`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (6 IDs) |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer/{layout_id}` | `delete_designer_api_v1_organizations__organization_id__projects__project_id__cartography_designer__layout_id__delete` | `delete_designer` (`layout_designer`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (6 IDs) |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer/{layout_id}` | `update_designer_api_v1_organizations__organization_id__projects__project_id__cartography_designer__layout_id__put` | `update_designer` (`layout_designer`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (6 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/exports` | `list_exports_api_v1_organizations__organization_id__projects__project_id__cartography_exports_get` | `list_exports` (`cartography`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (6 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/exports/{export_id}/download` | `download_export_api_v1_organizations__organization_id__projects__project_id__cartography_exports__export_id__download_get` | `download_export` (`cartography`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (6 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layers/{layer_id}/style` | `get_layer_style_api_v1_organizations__organization_id__projects__project_id__cartography_layers__layer_id__style_get` | `get_layer_style` (`cartography`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (6 IDs) |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layers/{layer_id}/style` | `upsert_layer_style_api_v1_organizations__organization_id__projects__project_id__cartography_layers__layer_id__style_put` | `upsert_layer_style` (`cartography`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (6 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts` | `list_layouts_api_v1_organizations__organization_id__projects__project_id__cartography_layouts_get` | `list_layouts` (`cartography`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (6 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts` | `create_layout_api_v1_organizations__organization_id__projects__project_id__cartography_layouts_post` | `create_layout` (`cartography`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (6 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts/{layout_id}/exports` | `export_layout_api_v1_organizations__organization_id__projects__project_id__cartography_layouts__layout_id__exports_post` | `export_layout` (`cartography`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin | 3 files (6 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts/{layout_id}/exports/preview` | `preview_layout_export_api_v1_organizations__organization_id__projects__project_id__cartography_layouts__layout_id__exports_preview_post` | `preview_layout_export` (`cartography`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (6 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "sources": [
    "apps/api/openvietgis_api/domains/cartography/domain.py",
    "apps/api/openvietgis_api/adapters/http/layout_designer.py"
  ],
  "engine": "E06",
  "style_modes": [
    "single",
    "categorical",
    "range"
  ],
  "paper_sizes": [
    "A4",
    "A3",
    "A2",
    "A1",
    "A0"
  ],
  "orientations": [
    "portrait",
    "landscape"
  ],
  "print_formats": [
    "png",
    "pdf"
  ],
  "layout_designer_element_kinds": [
    "map",
    "text",
    "legend",
    "north",
    "scale",
    "rect"
  ],
  "layout_designer_dpi_options": [
    150,
    300
  ]
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0012_cartography_layouts` | `0011_processing_jobs` | `apps/api/migrations/versions/0012_cartography_layouts.py` | yes | `retain` | R009 Cartography styles, print layouts, and immutable print export manifests. |
| `0026_layout_designer` | `0025_processing_models` | `apps/api/migrations/versions/0026_layout_designer.py` | yes | `retain` | Versioned project-scoped print designer templates. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E06`: **5**

- `apps/api/openvietgis_api/adapters/http/cartography.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/layout_designer.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/cartography_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/cartography/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/cartography/domain.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Pins layout exports to E02 layer content revisions and validated CRS profiles.
- **E07**: Uses shared local_object_store artifact storage for immutable PNG/PDF print outputs.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **4 pytest files** containing **7 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/integration/m2/test_cartography_completeness.py` (primary, 1 tests): `test_print_export_rejects_incomplete_layout`
- `apps/api/tests/integration/m2/test_print_revision_pins.py` (primary, 1 tests): `test_print_revision_confirmation_rejects_stale_source_style_layout_before_publication`
- `apps/api/tests/integration/spatial/test_layout_designer.py` (primary, 1 tests): `test_layout_designer_revision_scope_and_geometry`
- `apps/api/tests/unit/test_cartography_output.py` (primary, 4 tests): `test_scale_uses_actual_aspect_preserving_transform`, `test_polygon_hole_and_multi_geometries_render`, `test_legend_overflow_rejected_before_export`, `test_all_supported_physical_page_dimensions`

## 8. Edge Cases & Parity Risks

- Revision-pinned print exports must record deterministic SHA-256 revision_manifest fingerprint and reject drifted layer revisions when pinned.
- Invalid hex colors, unknown style modes, invalid label fields, or out-of-range paper sizes/DPI must fail validation.
- Layout designer elements must enforce version=1, valid element kinds, and bounded paper/DPI (A0..A4, 150/300 DPI).
- Export artifact downloads must verify tenant/project ownership and artifact integrity.

## 9. E06-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **12** live runtime operations in `E06` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E06` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **2** owned migrations, **5** source files, and **4** pytest files (**7** test IDs) linked in `contracts/engines/e06.json`.
- [ ] Next phase (`E06-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
