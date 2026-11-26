# E06 Rust-Native Parity Evidence

- **Parent Spec**: Spec 064 (`064-rust-platform`), Reference: R009
- **Child Spec**: `specs/064-rust-platform/engines/e06-cartography-layout.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e06.json`
- **Branch**: `064-e06-rust-native`

## 1. Ported E06 Endpoints (12 / 12)

All 12 live HTTP operations in `E06` (`cartography.py` + `layout_designer.py`) are implemented Rust-native in `apps/api-rust/src/cartography/` and `crates/ovg-storage/src/cartography.rs` inside RLS-enforced `ProjectRead` (`TenantTransaction`) scopes:

| # | Method | Path | Handler | Status |
|---|---|---|---|---|
| 1 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layers/{layer_id}/style` | `get_layer_style` | Ported |
| 2 | `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layers/{layer_id}/style` | `upsert_layer_style` | Ported |
| 3 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts` | `list_layouts` | Ported |
| 4 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts` | `create_layout` | Ported |
| 5 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts/{layout_id}/exports/preview` | `preview_layout_export` | Ported |
| 6 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/layouts/{layout_id}/exports` | `export_layout` | Ported |
| 7 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/exports` | `list_exports` | Ported |
| 8 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/exports/{export_id}/download` | `download_export` | Ported |
| 9 | `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer` | `list_designer` | Ported |
| 10 | `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer` | `create_designer` | Ported |
| 11 | `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer/{layout_id}` | `update_designer` | Ported |
| 12 | `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/cartography/designer/{layout_id}` | `delete_designer` | Ported |

## 2. Control Plane & Print Renderer Capabilities

- **Symbology & Labels**: `single`, `categorical`, and `range` style specs with hex color validation, stroke/opacity bounds, auto-generated `legend_items`, `spatial.source_fields` label validation, and revision-incrementing upserts.
- **Print Layout & Revision Pinning**: Supported CRS (`4326`, `3857`, `4756`, `5899`), `A0..A4` paper sizes, `portrait`/`landscape` orientation, 150/300 DPI (`A0..A2` restricted to 150 DPI), bounded unique layer sets (`1..=20`), Web Mercator `ST_TransformPipeline` + `require_web_domain` checks (`4326 <-> 3857`), 2500-feature limit (`PRINT_FEATURE_LIMIT`), deterministic SHA-256 `revision_fingerprint` (matching Python `ensure_ascii=False` canonical JSON), and stale pin rejection (`409 STALE_PRINT_REVISION`).
- **Pure-Rust PNG/PDF Renderer**: Zero-dependency RGB canvas (`apps/api-rust/src/cartography/renderer.rs`) supporting Point, LineString, Polygon with holes (even-odd scanline fill), MultiPoint/MultiLineString/MultiPolygon, categorical/range/single fills, labels, north arrow, scale bar, legend overflow checks (`PRINT_LEGEND_OVERFLOW`), attribution/scale disclosure, `pHYs`-tagged PNG, and PDF 1.4 output persisted atomically via `transfers::store::Store` with SHA-256 verification.
- **Layout Designer CRUD**: Versioned `cartography.designer_layouts` (`map`, `text`, `legend`, `north`, `scale`, `rect` elements), bounded paper geometry, `map_id` reference checks, project quota (`50` under `pg_advisory_xact_lock`), optimistic `revision` concurrency (`409`), and `audit.audit_events` recording (`cartography.designer.{create,update,delete}`).
- **Security**: CSRF fail-closed (`403`), tenant/project RLS (`TenantTransaction` + `ProjectRead`), and GIS-scoped rejection (`403`).

## 3. Differential PostGIS Parity & CI Verification

- `tests/rust-parity/e06_cartography.py` (read parity: styles, layouts, exports, binary export download + SHA-256 headers, designer list, foreign-tenant denial)
- `tests/rust-parity/e06_cartography_writes.py` (write parity: styles, layouts, print preview manifest + SHA-256 fingerprint, cross-CRS `4326->3857`, unsupported transform rejection, `A2` 300 DPI rejection, stale pin `409`, real PNG + Pillow verification, real PDF container verification, style mutation pin invalidation, designer CRUD + quota 50 + audit trail + negative validation, CSRF & foreign-tenant denial)
- Wired into `.github/workflows/rust-platform.yml` (`Run E06 cartography parity`).
