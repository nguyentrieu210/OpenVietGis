# 032 — Select generated 3D Tiles features
## Specification
Pick a visible generated building by its nearest rendered triangle, highlight that building only, inspect its source attributes, and navigate to the exact GIS feature or version-pinned CAD entity (if a real CAD conversion link exists). No edits or inferred geographic correspondence.
## Acceptance and budgets
- New generated tiles store immutable feature-ID/triangle ranges, contiguous and complete for their GLB, up to the existing 3000 features / 128 tiles. Import/legacy sets remain render-only. Sidecar index is private and counts toward asset quota. Standard external GLB/manifest remain unchanged.
- Scoped GET /3d/tilesets/{id}/feature?uri=...&triangle=... resolves only an indexed triangle in that exact project/asset and checks current layer/content/schema pins. If stale, return explicit stale state without live attributes/CAD links; never silently bind edited geometry to old mesh. Current attributes and CAD conversion provenance only (no guessed links).
- CPU hit test uses nearest projected depth with triangle index, maximum existing 250k resident triangles per click; no per-frame raycasts. Highlight only one GLB range, clear on empty selection, hide, eviction, teardown; late async responses cannot restore stale selection.
- GIS navigation re-reads the exact pinned feature and verifies scope, even when not initially in viewport; hidden layer may be revealed by explicit action, existing filters retained. CAD source navigation resolves exact document/version/handle and rechecks link before switch.
- When CAD-derived polygons have no numeric height field, the generator accepts an explicitly declared uniform height (0.01–3000 m) instead of a field. Exactly one height method; provenance records the declared value; GIS attributes are never fabricated or changed.
## Plan / Constitution check
Use existing 3D renderer, scoped immutable storage and feature/CAD contracts. No separate scene mode and no external I/O. Migration adds optional index column; rollback removes picking metadata only; regenerate old or downgraded assets as needed. Explicit feature capability and provenance, no GIS/CAD data mutation. Feature branch, test real DB, browser picking and regression.
## Tasks
- [x] Immutable index, scoped lookup & stale handling
- [x] Nearest triangle selection, per-feature highlight, GIS/CAD navigation
- [x] Integration, browser and typechecks; convergence

## Evidence (2026-11-23)
- Fresh disposable PostGIS, migration through 0034: 13/13 Python tests passed, including 9 generator unit tests and 4 integration cases. Two buildings in one tile: triangle boundaries 0/31 and 32/43 resolve the correct IDs, named attributes, stale source omits attributes/links, invalid URI and unindexed triangle rejected, viewer and outsider access checked; imported ZIP remains view-only.
- Actual DXF upload, surveyed placement and GIS conversion, declared uniform height, generation, triangle lookup returned exact document version/handle and unchanged source revision. Both/neither height methods rejected.
- 36/36 spatial-workspace tests passed, package/web strict TypeScript and isolated Vite candidate build passed. Chromium/SwiftShader rendered actual API-generated GLB (14,188 mesh pixels), clicked each building, verified only selected range highlighted, displayed attributes, GIS navigation action, empty/hide/late response clearing and teardown, no external I/O or page errors. Browser replays scoped API artifacts and exercises the renderer and feature panel, not a full authenticated portal deployment.
- Ruff and git diff --check passed. No production data, deployment or main merge. Near-plane-crossing triangles remain skipped by the bounded CPU hit test. CAD navigation identifies the version and handle in the vault; it does not command CAD editor selection or edit the original drawing.
