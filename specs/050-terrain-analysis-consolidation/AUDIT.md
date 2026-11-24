# Evidence
- Main/live diverged 12/44 commits, 25 conflicts. Merge candidate f95258d includes both histories. Relative to production, only renderer-neutral diagnostics text and five-line benchmark documentation changed before feature work; existing backend and QGIS guards retained.
- 47/47 unit tests, including matching-grid dual-surface cut/fill and analytic conservation, Priority-Flood acyclic routing, boundary-connected flood/no-data tests.
- Both TypeScript checks and Vite build passed.
- Chromium terrain-engineering fixture passed nine analysis overlays including dual DEM and conditioned hydrology, project DEM, contour TIN, download, mocked scoped write and permission revocation.
- Existing engineering precision limitations remain explicit. No new backend migration or mutation of a user's real project.
