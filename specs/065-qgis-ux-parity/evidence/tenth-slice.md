# Tenth implementation slice
Base db7792c4371c6a07fa8e162f03c3a3f3ee47e3d7.

- EDIT-07: lock distance in metres and absolute north-based bearing, separately or together. Spherical forward/inverse calculation on WGS84 coordinates (mean radius 6371008.8m); no ellipsoidal surveying accuracy claim. Locks explicitly take priority over snapping/ORTHO on map clicks; direct coordinate entry remains unchanged. Continuous drag disabled during locks. Both locks permit adding a point without a map click.
- EDIT-09: display final-segment bearing with method description. Browser test adds a 100m eastward segment. Fix expanded sketch panel overlap with floating command bar.
- TypeScript PASS; 75 Vitest + 36 Node tests PASS; production build and targeted synthetic Chromium exit 0.
- Authenticated Rust/PostGIS, QGIS paired acceptance, per-layer tolerance and exact projected/ellipsoidal constraints remain outstanding.
