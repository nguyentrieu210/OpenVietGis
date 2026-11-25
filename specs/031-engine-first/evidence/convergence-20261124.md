# R031 convergence checkpoint

## CAD and GIS/3D linkage
- Real production-origin CAD artifact /ovg-cad-2bc3537/ with synthetic DWG:
  historical revision opened, Save disabled, unchanged two-version fixture.
- Real CAD DXF create smoke passed: empty project opens editor; no write
  before explicit Save, correct filename and version receipt.
- Existing prior real DWG open/save v2 assertions passed. The tests for
  historical view and blank DXF now run independently.
- Fixed stale CAD source link following georeference import by invalidating
  selected-feature link lookup on successful import, including same selection.
- Updated fixture API to include the real source/layer scope and revision pins,
  use current vault navigation and Cesium footprint canvas.
- CAD georeference, CAD/GIS/3D bidirectional linked selection and revocation
  fixture smokes passed, with both fixed and ephemeral loopback parent origin.
- Cesium native Tiles smoke passed again (2,238 visible red pixels), scoped
  resource paths and permission revocation. These browser tests use mocked APIs.
- Added these three isolated browser smokes to the Engine-First CI command.

## Managed sync
On disposable real PostgreSQL/PostGIS image, extended existing authenticated
FastAPI integration test: ten alternating Web and managed desktop ChangeSets
of ten new features each. Second session resumes from its old cursor in pages
of three. Exactly 100 unique expected IDs, ten increasing cursors, no full reset,
no duplicate after replaying the same operation ID, empty delta at latest cursor.
Existing cross-binding/tenant denial, viewer/editor scopes, receipt and revoke
tests preserved. Two tests passed in 27.07 s. This is authenticated in-process
API integration and simulated disconnect, not a two-machine network test.

## CI diagnosis
GitHub Actions run 45361783776 on parent c3732b0:
backend 1,056 tests passed, 2 skipped; validate 1,073 tests passed, 2 skipped,
7 subtests passed. Both jobs stopped at stale scope-marker rules. The prior
main also had failures. Updated only exact adapter/manifest paths for Terra Draw
and exact non-API renderer label/type references for Cesium. No generic folder
exemption. Added regression to reject direct engine imports elsewhere and
vertical dependencies in the adapter. Scope tests 8/8, ruff, scope,
architecture and documentation checks passed locally.
New commit CI and final release gate remain required.

No production promotion or project writes. Keep existing full editor for holes,
multipart and CAD snapping; GeoLibre remains blocked for authoritative writes.
