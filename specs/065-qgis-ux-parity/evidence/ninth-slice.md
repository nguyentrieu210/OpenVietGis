# Ninth implementation slice
Base a0d20ab45382e64c867f6448d498a66f3d5d7e5e.

- EDIT-02: strict explicit coordinate parsing (including multipoint command inputs) rejects missing values and coordinates outside Web Mercator bounds; structural preflight for XY drafts before finish and vertex save.
- EDIT-04: snapping scope all visible, active/edit layer, or user-selected authorized layers; persisted scope in workspace; applied to point move, sketch click/drag and advanced editor. Existing shared pixel tolerance unchanged.
- EDIT-05: reject consecutive duplicate vertices, open polygon rings, insufficient distinct vertices and zero planar area. Not a complete topology engine; authoritative PostGIS validation remains necessary.
- EDIT-07: explicit coordinate input no longer silently altered by ORTHO; mouse drag honors snapping/ORTHO. Distance/angle locking and per-layer map-unit tolerance remain.
- TypeScript PASS; 73 Vitest + 36 Node tests PASS; production build exit 0; targeted synthetic Chromium exit 0 including persisted snapping scope. Scope-specific geometry selection tested as pure function, not end-to-end coordinate fixture.
- No authenticated Rust/PostGIS or paired QGIS acceptance.
