# Plan

1. Keep existing global DEM terrain mode, GIS/CAD state and permissions.
2. Add pure TypeScript bounded terrain-analysis algorithms to spatial-workspace;
   use pinned Delaunator and validated TerrainGrid contract.
3. Reuse project DEM preview, raster import/save and advanced 3D scene.
4. Add one workbench with source/reference selection, analysis overlay, export
   and explicitly permission-gated raster save.
5. Validate numerical fixtures, GDAL GeoTIFF roundtrip, browser workflows and
   permission revocation before frontend-only atomic deployment.
6. Record remaining higher-precision features separately, without claiming
   survey-grade outputs.
