# Plan
1. Preserve the existing pinned MapLibre 6.18.0 engine and its renderer, do not introduce a fourth engine or duplicate GIS store.
2. Add isolated SurveyTerrain component owning raster-dem terrain and hillshade sources, map event subscriptions, relief options, loaded DEM sampling and a local illustrative profile.
3. Integrate with SpatialWorkspace's existing mode coordinator: GIS 2D / Địa hình 3D / Địa cầu on map; GIS / CAD on Topbar. Leave existing CAD and Cesium transition guards intact.
4. Use public source endpoints with native attribution. Read MapLibre's pinned drawn-elevation API to distinguish unloaded data from genuine 0 m. No unverified vertical transformation.
5. Exercise synthetic Terrarium tiles in real Chromium/WebGL, missing data, alternate provider, no map remount, layer/table retention, exaggeration normalization, profile and revocation. Confirm actual providers separately via HTTP/CORS. Run TypeScript, build and existing globe/CAD regression tests.
6. Commit isolated changes, reconcile current live base, deploy atomically with prior release rollback and HTTP exact-byte verification.
