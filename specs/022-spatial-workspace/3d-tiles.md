# 030 — Project 3D Tiles preview
## Spec / acceptance
A project editor imports an immutable local ZIP of OGC 3D Tiles 1.1 (tileset.json and static GLB relative resources). Readers can select it and navigate a viewport-streamed 3D preview; no external network, no unbounded decoding. Existing GIS/CAD and relative-height 3D scenes remain unchanged. Authenticated standard manifest and relative GLB endpoints support portability.
- Reject unsupported formats, traversal, duplicate entries, external URIs, malformed tree, invalid geographic bounds and transforms. No implicit tiling, multiple contents, textures, extensions, b3dm, or additive refinement.
- WGS84 ellipsoidal heights (EPSG:4979), ECEF transforms (EPSG:4978), glTF y-up to z-up; region bounds radians. Exclusive view, not mixed with local or orthometric scenes.
- Viewport region selection and zoom-based screen error approximation, REPLACE, max 8 resident tiles and 250k triangles; sequential fetch, cancel on movement/exit, remove offscreen GPU layers, report partial coverage/errors. Not terrain occlusion or full OGC conformance.
- Project/tenant permissions and RLS, immutable asset + audit, max 64 MiB ZIP, 128 MiB extracted, 128 GLBs, 16 sets / 256 MiB per project.
## Plan / Constitution check
Add scoped tables and validated import/read API, pure traversal/coordinate math, separate engine behind existing mode coordinator, GIS catalog and status. Core capability reusable, scoped and explicit CRS, open standard resources and audit; no engine or deployment change. No named scene pins in this increment. Downgrade removes imported tilesets: export via resource endpoints before rollback. Test validation, transforms, selection budget, real PostGIS scope and browser runtime.
## Tasks
- [x] API + migration and integration tests
- [x] Traversal, mesh conversion, engine and UI
- [x] Typecheck, regression tests, browser evidence and convergence

## Contract / usage
Under the project path, GET/POST /3d/tilesets (multipart name + file), GET /3d/tilesets/{id}/tileset.json and GET /3d/tilesets/{id}/{relative-glb-uri}. All reads require project access; POST requires project edit and CSRF. The importer checks the GLB envelope; unsupported mesh primitives and out-of-region vertices are rejected by the bounded renderer and reported in status. The underlying resources remain immutable. Select 3D Tiles in GIS, choose an imported set (explicit ellipsoid reference), then switch to 3D; returning to GIS restores the existing scene controls.

## Evidence (2026-11-23)
- 36/36 package unit/regression tests, including region/LOD cap, WGS84 control points, glTF axis order and path rejection.
- Package and web application TypeScript typechecks passed.
- Real Chromium/SwiftShader smoke: visible GLB pixels, coarse resource only at start, detailed resource on zoom, one resident tile, disposal, no external network.
- Fresh disposable PostGIS migration through 0032; integration tests test_tilesets.py and test_saved_scenes.py passed (2/2), including invalid archives, permission denial for viewer writes and cross-tenant reads.
- Ruff and git diff --check passed. No deployment or merge.
- Convergence: scoped import and preview acceptance met. Browser fixture tests engine, not a full authenticated portal session. Future work: full interoperability, texture/compressed mesh support, background tiling from project layers, progressive parent retention and benchmarking large real datasets. No city-scale performance claim.
Reference: https://docs.ogc.org/cs/22-025r4/22-025r4.html
