# Sixth implementation slice
Base 7fe1e79baf3d0b56dc1015803f40b61de5af369e.

- STYLE-02/06: persisted bounded per-class graduated colors; preserve non-linear colors in supported contiguous imported ranges, reset overrides on deliberate ramp/reclassification changes. Render, legend and export share gradColor. Category/renderer changes clear stale hidden classes.
- TABLE-01: single map click or table row adds the actual selection key, Ctrl-click toggles without leaving removed focus highlighted; box selection clears stale focus. Table unchecking removes focused highlight.
- TABLE-07: personal scoped column visibility, restore after close, page TSV copy (formula-string escaping, tabs/newlines normalized). Column order/width, selected export and remote selection/revision coverage remain.
- TypeScript PASS; 65 Vitest and 36 Node tests PASS. Canonical frontend build generated; complete canonical browser smoke exit 0 (spatial-mode, core-toolkit and parity smoke) with intercepted synthetic fixtures.
- No authenticated Rust/PostGIS or QGIS paired acceptance. No full parity row accepted.
