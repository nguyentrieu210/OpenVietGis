# Thirty-ninth implementation slice (GitHub only)

- DSM: strict CSV shape, duplicate header rejection, bounded input, delimiter detection, identifier/precision preservation, cancellable remote WFS/OGC/ArcGIS loading, response byte/feature bounds, URL path fixes and tests. Noninteger and noncanonical numeric CSV attributes remain strings for explicit import inspection. Server/provider acceptance pending.
- Shell: request generation invalidates stale authentication responses; explicit session expiry; temporary map focus hides dock panels without unmounting or modifying saved layout; floating panels ignore hidden zero-size bounds. Browser verification pending.
- 3D: model file read must retain current project and selected feature source/content/schema revisions.
- Spreadsheet join: exact unique lookup keys, skip blank/null, preserve terminal blank cells; tests added. Persistent relations/forms remain.
- Metadata: read-only source/schema/layer revisions, lifecycle and permissions. No diagrams/actions/map tips claim.
- Source commits: `d45f02e`, `ed5ea00`, `678fc4a`, `a0d7637`, `2e6ab20`, `08d92d9`, `52691d7`, `3589aad`, `afb392c`, `38849f4`, `e481194`, `6e9fdc5`, `63bbd9a`, `1f8defb`.
- New tests written but not executed; no typecheck, build, QGIS comparison, merge or live deployment. All 101 acceptance tasks remain open. PROC-10 temporal/mesh/point clouds still unverified; static GLTF and terrain mesh are not equivalent to mesh dataset providers.
