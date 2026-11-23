# Studio renderer boundary — v1 preflight

Parent roadmap: R004 (map workspace), R012 (CAD precision), Spec 020. This is an incremental implementation checkpoint, not an approved replacement of the legacy MapEngine contract or a release certification.

## Current boundary

- `apps/web-next/src/map-surface.ts` is the only Studio TypeScript file that imports MapLibre, its CSS or worker module. Explicit constructors/types are exposed to Studio tools through a compatibility surface. Worker setup, renderer-specific canvas detection and dependency imports live there.
- GIS tools, raster, layer properties, layout/export, edits and commands use that local boundary instead of importing the renderer package directly.
- The compatibility surface still has the native-shaped map API. A fully replaceable engine requires neutral interfaces for map state, style, source, overlay, input events and export; this checkpoint does not imply that work is complete.
- No native renderer objects or private attributes become part of module/plugin contracts.
- The existing legacy adapter, explicit standalone 3D adapter and isolated 3D fixture have specific permitted paths. No parent-folder wildcard enables arbitrary renderer imports. Existing vertical/domain markers remain prohibited even inside these paths.
- Renderer-specific stylesheet selectors remain presentation implementation details. No asset, API, migration or live tenant data changed.

## Verification

- Exact-path scope checker: PASS.
- Scope regression tests: 7/7 PASS (including denied Studio imports, denied coordinator import, denied vertical import inside renderer adapter).
- Frontend TypeScript + production bundle: PASS.
- Architecture, documentation and API contracts: PASS.
- Targeted loopback Chromium fixture (`renderer-boundary-smoke.mjs`, `/app`): PASS, real workspace canvas, command line, measurement, right-click repeat, Escape, and no JavaScript errors. All API data were synthetic; this is not production acceptance.

Full browser suite and independent production release gates remain open.
