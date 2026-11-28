# Thirty-seventh implementation slice (GitHub only)

- DOCK-03: bound source-browser scans to 50 pages / 5000 scanned source identities / 1000 visible active sources; track retired identities across pages, reject repeated/empty advancing cursors and invalid IDs, reset scan state on scope change or refresh. No drag-to-map implementation in this slice.
- DOCK-08: reject oversized or duplicate readable theme membership instead of truncating, reject control characters and ambiguous names, recheck latest project scope/restriction and preferences after async name prompt. Existing save/apply/delete remains personal.
- DOCK-07 and TABLE-03: reconciled existing source code evidence; no new functionality or acceptance claim for these rows.
- Added source-browser and map-theme tests. Source commits: `6b2b90577d2849334016470c1dd685b73af8dd3c`, `3a716a4a1e3fcf984da538f40c6a0fd69951986c`, `e9c919262e3cafc796c121baa4e49cfa772253a3`, `ae44686dd1794ff5bb4b7f6258bfac069508600e`, `003fd9162d66b70c8dc93a3318b041303a081301`, `51c3ff8875e77b45934f24636520b5de38ac2cf8`.
- Tests written but not executed. No build, merge, production deployment or task acceptance promotion. All acceptance gates remain.
