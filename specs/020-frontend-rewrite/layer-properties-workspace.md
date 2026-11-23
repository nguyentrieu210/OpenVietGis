# GIS Core layer properties, snapping and workspace settings

Implemented 2026-11-23. The layer dialog has information, source, symbology, labels, fields, filter and display tabs. Geometry style preview and categorical color keys use actual loaded attributes; the tree expands into category counts and symbol swatches. Filter predicates apply to loaded authorized features without server mutation. Label overlays use Unicode text, viewport culling, collision cells and a hard 180-label limit. Per-layer zoom range controls actual MapLibre rendering.

Personal settings are scoped by account, organization and project, stored in browser localStorage: dock width/open state, browser dock, legend, OSM visibility, snapping mode/tolerance and layer view preferences. Shared map SAVE retains the existing revisioned server configuration (base style, layer order and visibility); advanced renderer, filter and labels are personal until a shared backend schema is added. No provider credentials required for OSM visibility; satellite provider is not added.

Snapping is opt-in. Candidate features are identified from rendered authorized layers near a click (tolerance in screen pixels) and snapped to original loaded geometry by feature key; supports vertices/points and edges, including marker drag-end. Explicit command coordinate arguments are not altered. No automatic data writes: the existing changeset confirmation and authorization path is retained.

Verification: TypeScript and production build passed; Playwright fixture checks for all seven properties tabs, categories, filter, labels, persistence over navigation, OSM toggle, and exact vertex coordinate in mock create changeset. Core/plugin regression and all former 11 workbench tests passed. Data on the live database was not changed.
