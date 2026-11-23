# 033 — Cross-mode spatial selection
## Specification
Carry the selected project source feature between GIS, generated 3D Tiles and CAD vault. A selection is a source ID, layer ID and feature ID; when known, pin layer/content/schema revisions. CAD link is document/version/handle from immutable conversion provenance. No coordinate guessing, auto-editing or arbitrary choice among multiple links.
## Acceptance
- Reverse lookup in a selected generated tileset resolves exact source/layer/feature ID to one triangle range and current/stale state using the same permission and pin validation as triangle lookup. Unknown/imported/legacy/wrong scope fails closed.
- GIS selection on 3D entry locates only a matching current generated tile, moves camera to the feature and highlights only its triangles, within 8 tiles/250k resident triangles. User click/hide/exit aborts pending focus. No matching/stale tile is explicit, not a guessed substitute.
- A current 3D selection can return to GIS using exact pinned feature or open CAD source; normal CAD entry from a selected GIS feature resolves a unique current CAD link, multiple links require explicit selection. CAD vault can choose the linked source on GIS (even if not loaded) or 3D. No unsupported editor selection claim.
- Selection is session/project scoped; user selection replaces it; failed lookup or access loss cannot restore an old highlight. Existing GIS filters and edit blockers remain.
## Plan / Constitution
Reuse project-scoped immutable tile index and CAD provenance, existing feature read and bounded renderer, no migration/data writes. Verify on disposable PostGIS and browser replay of actual generated assets; rollback is application-only.
## Tasks
- [x] Reverse lookup and scope/pin checks
- [x] Renderer focus and cross-mode navigation
- [x] Tests, evidence and convergence

## API and selection contract
GET /3d/tilesets/{id}/feature-by-id?source_id=...&layer_id=...&feature_id=... returns the Feature 032 identity, triangle range and current/stale response. Source/layer pins are checked server-side; client validates exact IDs/revisions against the selected asset and originating GIS row. Missing or unsupported index is 404, ambiguous index is 409. No new schema or write operations. CAD source link choices revalidated using existing document/version/handle endpoints. Session selection is reset on explicit new pick and permissions loss; stale GIS navigation permits returning to GIS without old selection. Camera focus computes only the selected mesh range, prioritizes its visible tile within existing caps.

## Evidence (2026-11-23)
- 13/13 Python targeted tests passed after migration through 0034 on disposable PostGIS; exact reverse lookup for courtyard/other, wrong source/layer/ID 404, stale response and viewer/outsider checks. Real DXF georeference -> GIS -> 3D reverse lookup returns exact CAD link; separate CAD fixture run and imported ZIP reverse lookup regression both passed.
- 36/36 spatial-workspace tests pass, including priority within the eight-tile cap; package and application strict TypeScript and candidate Vite build passed.
- Chromium/SwiftShader actual SpatialWorkspace, replayed real CAD integration artifacts and GLB: GIS -> 3D exact restored selection (16,186 amber pixels), 3D -> CAD exact handle/version, CAD -> 3D and GIS, stale source returns to GIS unselected, delayed lookup cancelled by access revocation. Prior picking/hide/late-response browser regression passed. Browser fixture uses real renderer/workspace and recorded API results, not fully authenticated live production; LiveApp GIS detail loading verified by API and typecheck.
- Ruff and git diff checks passed. No migration, production deployment or main merge. CAD vault uses explicit handle, not pointer picking inside the third-party editor. Only the selected generated tileset is searched; arbitrary unindexed meshes remain display-only.
