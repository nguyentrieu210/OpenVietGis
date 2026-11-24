# Initial research and audit (partial, not runtime acceptance)
Baseline 27f0b35af966b7a46e7b17929395d9bc01515e51. Repository files inspected via GitHub.

| Component | Observed | Initial disposition |
| --- | --- | --- |
| Web-next | [package.json](../../apps/web-next/package.json): React, MapLibre, React Query, internal workspace/SDK | KEEP; no frontend rewrite |
| API | [pyproject.toml](../../apps/api/pyproject.toml): FastAPI, ezdxf, PyShp, psycopg | KEEP; audit actual imports, runtime GDAL and DB algorithms |
| Spatial workspace | [package.json](../../packages/spatial-workspace/package.json): MapLibre/React and local CAD/3D exports | KEEP coordination, compare adapters |
| CAD | [cad-editor.ts](../../packages/spatial-workspace/src/cad-editor.ts): existing dedicated-origin OpenCAD iframe relay, exact origin/channel check, 25 MiB max, read-only, save acknowledgement and timeout | KEEP/HARDEN; already integrates external CAD; verify actual WASM build and end-to-end, no redundant rewrite |
| glTF | [gltf.ts](../../packages/spatial-workspace/src/gltf.ts): custom bounded static subset, rejects remote URIs, textures/animation/extensions | COMPARE upstream; preserve safety, avoid silent support expansion |
| 3D Tiles | [tiles3d.ts](../../packages/spatial-workspace/src/tiles3d.ts): custom MapLibre mesh, viewport tile selection, cancellation and triangle budgets | COMPARE dedicated 3D engine; candidate disabled |
| Unified mode | [R022](../022-spatial-workspace/spec.md): shared project scope, dirty edit protection and lazy lifecycle | KEEP |
| Processing | [R008](../008-processing-jobs/spec.md), historical roadmap lists buffer/clip/dissolve and GDAL verification for import | AUDIT runtime; avoid replacing proven PostGIS/GDAL |
| Release | [R030](../030-v1-unified-release/spec.md): unified v1.0 | KEEP; no scope reduction |

## Candidate evaluation (not yet dependency approval)
- Terra Draw: https://github.com/JamesLMilner/terra-draw — editing candidate, check MapLibre compatibility, snapping/topology gaps, license, mobile and accessibility.
- GeoLibre: https://github.com/opengeos/GeoLibre — investigate actual publishable WASM API, algorithm coverage, bundle size, upstream maintenance, dependency licenses; no assumption that advertised algorithms are production ready.
- GDAL/PROJ/PostGIS/GEOS: inspect existing runtime first; authoritative CRS/geometry reference where supported.
- QGIS Processing: https://docs.qgis.org/ — optional isolated worker; evaluate installation footprint, GPL distribution obligations, algorithm availability and no network execution.
- CesiumJS: https://github.com/CesiumGS/cesium — compare standalone self-hosted globe/tiles, network behavior, asset authorization and footprint; no mandatory ion dependency.
- OpenCAD: inspect existing relay upstream pin, licenses and artifact checksum before changes; do not assume native DWG support or replace safe origin isolation.

For each candidate record exact upstream tag/SHA, artifact hash, release/license metadata, transitive notices, maintenance/security, sample API, benchmark, functional comparison, rejection reason and ADR when warranted. All upstream claims require verification. Initial inventory is deliberately incomplete; G0 is open.
