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
- [x] Generator geometry and format
- [x] Scoped API, provenance, UI
- [x] Real PostGIS, browser and typecheck; evidence and convergence

## API contract
POST project /3d/tilesets/generate JSON: name, layer_id, expected_layer_revision,
expected_content_revision, expected_schema_revision, height_field, base_ellipsoid_m,
acknowledge_ellipsoid (true). Numeric fields are integer or exact decimal strings per
existing catalog schema. 201 returns asset and immutable provenance; 409 stale pins,
422 invalid data/acknowledgment, 413 limits/quota, 503 unavailable/timeout. Existing
manifest/content endpoints remain portable. User selects GIS > 3D Tiles > Tạo từ GIS,
sets fields/base, generates, then opens 3D. Old assets remain immutable when GIS changes.

## Evidence and convergence (2026-11-23)
- 9/9 Python generator unit tests. 3000 synthetic rectangle buildings generated 2 tiles,
  36,000 triangles, 5.17 seconds on development VPS (mesh generation only; not end-to-end
  throughput or a production performance promise).
- Fresh PostGIS migration through 0033, 4/4 targeted integration cases: actual manual
  polygon courtyard and decimal height, independent 4978→4979 control (vertices within
  1 mm, roof area and hole retained), revision conflict and invalid height atomicity,
  project viewer/cross-tenant denial and audit, imported 3857 GeoPackage with independent
  coordinate control, and regression import tilesets + named scenes.
- 36/36 TypeScript package tests; package and web application typechecks passed.
- Chromium/SwiftShader: rendered actual API-generated GLB (14,188 classified mesh pixels),
  acknowledgement required, exact source pins and CSRF sent, selected new set, disposal,
  no external network. HTTP responses replay real integration artifacts in browser fixture,
  not a full authenticated portal deployment.
- Ruff and git diff --check passed.
- Acceptance met within bounded V1. Open issues for later increments: terrain-aware
  bases, general approved CRS transformations, per-feature picking metadata, large
  background jobs and true generated coarse LOD. No production rollout or main merge.

References: https://postgis.net/docs/ST_TriangulatePolygon.html ; https://docs.ogc.org/cs/22-025r4/22-025r4.html
