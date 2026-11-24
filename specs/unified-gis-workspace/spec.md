# Unified GIS Workspace & CAD Separation

## Product contract
- GIS and CAD are top-level workspaces. The GIS map switches between MapLibre and Cesium without unmounting the GIS layer tree, attribute table, project or plugin state.
- Keep project authorization, feature identity and revision pins authoritative. CAD storage and versioned save behavior remain isolated; in-flight edits block transitions.
- 3D is a renderer of loaded GIS rows, not a separate authoritative datastore. Selected feature identity and camera transfer across modes. Default 3D renders supported point, line and polygon/multi/collection geometries with bounded entity/vertex counts (5000 rows, 100000 vertices); it does not fetch an entire remote tiled project or pretend all features are visible. Existing scene/DEM/model and native/provenance tile paths remain available.
- Advanced scene configuration stays accessible via the map settings menu; operations requiring existing 2D editor remain 2D-only. No new client write endpoint, permission bypass, or independently saved 3D edit history.
- Preserve attribution for all imagery. Replace irrelevant self-hosted Cesium ion mark with compact CesiumJS credit and info control. Reuse selected Google legacy/Sentinel imagery when supported; unofficial imagery remains optional and may be unavailable.
- Center low-zoom global view by looking down at the ellipsoid; default unconfigured scene starts at the GIS camera location with zero tilt. Preserve camera when returning directly to 2D. Existing right drag/wheel cursor zoom is retained.

## Acceptance evidence
- 38/38 unit tests, both TypeScript checks and production build passed.
- Chromium unified workspace fixture: top-level GIS/CAD, map-local 2D/3D, visible shared tree/table, automatic rendering of GIS rows, toggling vector layer without leaving 3D, CAD separation and permission revocation.
- Browser terrain/model, native tiles, multi-model selection, saved scene, model vault, and spatial-mode fixtures passed on the integrated candidate; the latter covers mobile ribbon, dirty CAD save blocking, keyboard isolation, plugin compatibility and revocation.
- Existing fixtures have been adapted to new navigation. Not all legacy CAD linkage or production DWG fixtures were rerun; they are not claimed as passed for this release.

## Remaining acceptance scope
- Full arbitrary GIS feature editing, spatial tools, shared undo/redo in 3D, and large-project native tiled vector streaming.
- Full shared renderer styles/labels, raster visibility and basemap variants, incremental updates for pinned DEM/model/extrusion scenes without returning to 2D.
- Ground-anchored custom left drag, measured globe-centering visual parity on real displays, compass ring, and hardware FPS performance benchmarks.
- Consolidate divergent GitHub main and production histories (backend/QGIS conflicts must be independently resolved). Do not force push or deploy a stale base.

## Earth-style desktop orbit update
- Mouse left drag rotates the ECEF camera pose toward the grabbed ellipsoid point, preserving altitude; off-globe drag uses virtual trackball fallback. Middle/Ctrl/Shift drag orbits the center surface point with unrestricted heading and up to 85 degrees of interactive tilt, with altitude collision guard. Right drag and wheel retain anchored dolly. Touch and pinch retain the native Cesium adapter. Click selection is forwarded only for non-drag left clicks; pointer capture loss clears the gesture.
- Saved scene API still accepts pitch up to 75 degrees; serialization retains this bound. Native hardware FPS and exact Google Earth parity are not claimed.
- Browser orbit test passed: left pan preserved altitude (300.371322 m), middle drag changed tilt to 29.79 degrees, heading rotated by 41.25 degrees and reversed while center and tilt remained stable, right drag zoom passed. Shared tree/table, CAD separation, revocation, earth navigation, model picking and saved-scene regressions passed. Both TypeScript checks, production build and 38/38 unit tests passed.
