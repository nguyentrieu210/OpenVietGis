# 049 — Unified Terrain Engineering

## Scope
Add one project-scoped terrain analysis workbench in GIS without duplicating the
existing raster store, CAD editor or 3D renderer. Open from the 3D settings menu
or TERRAIN command. Use the existing authorized DEM-preview and raster-store APIs.

## Functional requirements
- Read an existing single-band project DEM and require an explicit vertical
  reference and acknowledgment of metre units. Preserve missing samples.
- Build an unconstrained Delaunay TIN from points and contour vertices with
  numeric elevation attributes, rejecting incomplete source layers, conflicting
  duplicate coordinates, excessive extent, vertex counts or triangle edges.
  Rasterize the supported triangles to a bounded DEM without extrapolating.
- Visualize elevation, contour segments, slope, aspect, cut/fill relative to a
  horizontal design elevation, D8 flow direction and approximate contributing
  area, and a static below-water-level mask.
- Export GeoJSON with source and reference metadata, and Float32 GeoTIFF with
  EPSG:3857 and explicit NoData. Save a derived TIN DEM via the existing
  authorized project raster endpoint; the stored raster can be opened in
  Cesium's advanced project terrain settings.
- Enforce project read/write permissions and cleanup map overlays on exit.
  Do not issue any new backend routes or migrations.

## Limitations / acceptance
These are screening/visualization tools, not survey-grade engineering outputs.
Project previews are sampled to <=128x128. Web Mercator distances and areas
use approximate latitude scale corrections; TIN uses unconstrained Delaunay,
not surveyed breaklines. Cut/fill is relative to a flat plane, not a second
design surface. D8 does not fill depressions, resolve flats or simulate rainfall;
the water-level mask is not a hydraulic flood model. NoData is not zero elevation.
Project DEM rendering uses existing local sampled mesh, not a full-resolution
quantized-mesh tile service. Full-resolution raster analysis remains available
through the existing GDAL raster tools.
