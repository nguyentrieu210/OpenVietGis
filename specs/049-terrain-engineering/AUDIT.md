# Acceptance evidence

- 44/44 spatial-workspace unit tests passed, including planar analytic fixtures,
  missing values, contours, volume split, flow, Delaunay interpolation and TIFF.
- Both package and web-next TypeScript checks passed; Vite production build passed.
- GDAL read exported Float32 EPSG:3857 TIFF, min 100 m, max 130 m, NoData -99999;
  existing server terrain worker returned 32x32 grid with 496 NoData samples.
- Chromium terrain-engineering fixture passed project DEM request, six analysis
  overlays, contour TIN, GeoTIFF download, mocked authorized raster save, and
  permission revocation. Mocked save does not prove an authenticated production
  user's raster write.
- Chromium terrain-model and unified-gis regression suites passed.
- Survey-terrain regression initially timed out at the revocation DOM wait
  (30 s), then passed with a 60 s wait and two observed mocked revoked-access
  responses. No claim of sub-30-second revocation rendering latency.
- No backend schema migration, real project mutation, or permission bypass.
