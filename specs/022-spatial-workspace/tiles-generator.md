# 031 — Generate 3D Tiles from project polygons
## Specification
An editor selects a live polygon layer, a numeric metre height field, and an explicitly declared constant WGS84 ellipsoidal ground elevation. The server reads the entire authorized layer at the requested layer/content/schema revision, validates all features, and produces an immutable 3D Tiles 1.1 collection (closed extruded roofs, floor and walls, including holes), available in the existing 3D Tiles catalog. No modification to source GIS data. A failed validation, stale revision, or quota breach must produce no asset. Source provenance is retained.
## Limits and acceptance
- Only ready XY Polygon, stored EPSG:4326/3857; no datum inference; 1–3000 features / 60,000 input vertices / 128 spatial tiles; per-feature footprint extent <= 0.02 degrees; height 0.01–3000 m, base -1000–10000 m; latitude +/-84.9, longitude +/-179.99; source extent longitude <= 180 degrees. Reject null/invalid height or geometry, no silent omissions.
- Constrained polygon triangulation in PostGIS >=3.3/GEOS >=3.11, WGS84 ECEF calculations and local ENU in glTF y-up; region bounds padded conservatively. Spatial buckets, maximum 30k triangles per GLB. Root has no coarse mesh; viewport loads leaves, not a new simplification/LOD generator.
- Exact source pins checked under row lock, tenant/project permission + CSRF, existing private tile quota and immutable audit. Generation synchronous bounded with statement timeout; no external I/O. Source selection and operation status in existing GIS 3D Tiles panel; generated set selected automatically.
- Verify PostGIS holes, roof/floor/wall triangles, WGS84 control points, real generated GLB render and scoped permissions. No city-scale benchmark claim.
## Plan / Constitution check
Reuse existing API, storage and viewer; domain conversion is engine-neutral. Add provenance column with empty default for imported sets; downgrade removes provenance only (export metadata before downgrade). No source data mutation. No deployment or main merge. 
## Tasks
- [ ] Generator geometry and format
- [ ] Scoped API, provenance, UI
- [ ] Real PostGIS, browser and typecheck; evidence and convergence
References: https://postgis.net/docs/ST_TriangulatePolygon.html ; https://docs.ogc.org/cs/22-025r4/22-025r4.html
