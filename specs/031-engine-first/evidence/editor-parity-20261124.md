# Editor parity and baseline — 2026-11-24

Source baseline: main 9c71db0; R031 parent 94cfd90, identical tree to local 8cee3ff.
Read-only frontend live release marker: 14eec6889cb1f6ed6294681b82b32f43a2296aec.
This is not the main SHA; do not claim source/release equivalence before comparing
trees and manifests. No production service or data changed.

## Terra Draw parity
Real Chromium with MapLibre 6.18.0 and Terra Draw 1.40.0:
- Snap a pointer click 5 px horizontally and 4 px vertically away from an
  authorized synthetic point anchor: exact coordinate preserved.
- Session undo removes the new point; redo restores the same ID and geometry.
- With upstream ValidateNotSelfIntersecting configured, self-crossing polygon
  is rejected; simple polygon is accepted.
- Valid polygon with interior ring: rejected in this candidate. Keep the legacy
  editor for holes and multipart geometry. Full CAD snapping (midpoint,
  intersection, perpendicular), typed coordinate input, committed undo/redo,
  shared boundaries and backend permission/validation remain unproven.
- Previous draft/lifecycle smoke and 5 unit tests pass.

## Existing frontend baseline
Build using the deployment base: pnpm --filter @openvietgis/web-next exec vite build --base /rewrite/.
Serve the isolated dist directory at /rewrite/ on localhost; run the existing
smokes with OVG_TEST_URL pointed there. Initial unprefixed build was a harness
error (404 assets), not application regression.

- spatial-cache-smoke: PASS: cold 2 feature pages; unchanged F5 0; changed F5
  0 full pages, 1 feature detail and 1 history read; restricted scope 2 pages;
  no page errors. Mock API / synthetic data, not WAN or database benchmark.
- core-toolkit-smoke: initially blocked by missing /access/mine fixture. The app
  correctly denied writes. Updated only the test fixture with explicit unrestricted
  access; verify actual PUT payload, revision receipt and success notification.
  PASS map save, point create, measurement, visibility and all 13 workbenches.
- advanced-editor-smoke: not fully passing. Reached committed undo/redo; timed
  out waiting for confirmation dialogs on two runs. No claim of regression
  freedom; investigate asynchronous revision refresh and command readiness.
  Diagnostic-only edits removed. Existing runtime code remains unchanged.

## Disposition
Terra Draw is a candidate for simple WGS84 drawing; not approved for replacing
the existing editor. Existing PostGIS, GDAL, Cesium, cache and authorization
remain authoritative. G0/G1/G2 stay open.
