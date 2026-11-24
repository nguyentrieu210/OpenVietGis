# Acceptance evidence (2026-11-24)
- Public endpoint GET probes returned HTTP 200 and Access-Control-Allow-Origin: * for Mapterhorn WebP z10 and Mapzen AWS PNG z10. Mapterhorn TileJSON verified Terrarium and tileSize 512. Production origin did not return a restrictive CSP header in probe.
- Both TypeScript checks, Vite build and 38/38 spatial-workspace tests passed.
- Chromium synthetic Terrarium test: 10 Mapterhorn and 7 AWS fixture requests; height 125.5 m remains 125.5 m with 3x exaggeration; actual MapLibre WebGL raster DEM, same map canvas, tree and table; profile, source failure and alternate source, 2D and scope revocation passed.
- Existing unified-gis, earth-orbit, spatial-mode browser suites passed after label change, including CAD dirty save guard, plugin lifecycle, mobile menu and revocation.
- No claim of surveyed terrain accuracy, live authenticated user GPU benchmark, or automatic global contour/TIN. Spec/plan/tasks prepared following repository templates; no claim that unavailable specify CLI was executed.
