# Feature 048 — Unified Survey Terrain

## Product goal
Give field-survey GIS users a real relief view without changing their active GIS map, layer tree, attributes, basemap, project, permissions or business module. Keep Cesium's globe/DEM/models/3D Tiles separately available under the same GIS workspace; CAD remains a top-level workspace.

## User stories and acceptance
- US1 (P0): From GIS, enable Địa hình 3D directly on the existing MapLibre canvas. Terrain and hillshade use an on-demand Terrarium raster DEM. Returning to 2D removes owned terrain sources and restores zero pitch, without remounting the map or fetching the entire project.
- US2 (P0): Global DEM via Mapterhorn (512px WebP) by default, and explicit selectable Mapzen/AWS (256px PNG) fallback. Limit both to native z12 global data, overzoom as necessary; preserve original source attribution. No API key, proxy, bulk prefetch, or private GIS data sent to the DEM provider beyond normal map tile requests. External service availability is not guaranteed.
- US3 (P0): Read approximate unexaggerated elevation at pointer or map center, with 1x, 1.5x, 2x, 3x relief exaggeration and hillshade toggle. A missing tile must not be represented as 0 m. Source failure has an explicit status; users can change source or return to 2D.
- US4 (P1): Two map-center points define a local illustrative elevation profile with 33 samples of drawn DEM; preserve missing segments as gaps and identify sample count and approximate horizontal length. Do not claim survey-grade or calibrated VN-2000 vertical datum accuracy.
- US5 (P0): Globe remains independently selectable; editing transactions and permission revocation must not retain an unauthorized terrain surface. Entering blocked edit/workbench state returns to ordinary 2D so precise 2D editing workflows remain unchanged. No new server writes, schemas, changesets, or endpoints.

## Non-goals / limits
Global DEM is for overview, not survey-grade measurement. It is not contour/TIN reconstruction, automatic contour vector generation, project DEM tile ingestion, accurate slope/volume/hydrology, photogrammetry, BIM, or complete vTools parity. Existing project raster/contour tools and Cesium advanced scenes are retained. Those analytical capabilities require their own correctness/vertical-datum tests.

## Source contracts
- https://maplibre.org/maplibre-gl-js/docs/examples/3d-terrain/
- https://mapterhorn.com/data-access/ and https://mapterhorn.com/attribution/
- https://registry.opendata.aws/terrain-tiles/ and https://github.com/tilezen/joerd/blob/master/docs/attribution.md
