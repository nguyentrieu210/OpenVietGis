# SPEC 069 — Native GIS Rendering & Interaction Engine

Parent roadmap R004, R006, R012 and R015; continuation of SPEC 065/066. IDs 067/068 are reserved by plugin/house numbering branches.

Preserve React application orchestration, MapLibre WebGL rendering, existing Rust/PostGIS API and authorization. GPU interaction overlays must not change committed geometry, CRS, revision or permission semantics. Camera-only changes should not rebuild base geometry.

Acceptance:
- E01 native symbol labels, collisions, zoom/pan, placement and label style parity.
- E02 GPU task pins and other bulk overlays; retain accessible task list and tenant scope.
- E03 selection/highlight feature state; no geometry upload solely to select; comparison view parity.
- E04 GPU vertex handles and bounded per-frame transient updates, snapping, undo/redo and changeset semantics.
- E05 cached spatial index for nearby vertices/edges, preserve snap modes, layer tolerance and CRS.
- E06 GPU rubber-band and crosshair, no React state per passive mouse move; preserve edit commands.
- E07 bounded workers, vector tiles/LOD, progressive loading; chart atlas, virtual attribute rows, measurement overlays, workers/WASM and adaptive 3D memory.
- E08 reproducible performance regression tests and acceptance on merged main.

Benchmark targets (not guarantees): target p95 frame time <=16.7 ms at 60 Hz, snap p95 <=10 ms, vertex preview p95 <=16.7 ms, selection p95 <=50 ms; 1M source features via tiles (not 1M simultaneously loaded DOM/GeoJSON objects). Record browser, device/GPU, resolution, feature counts, geometry complexity, warm/cold cache, and p50/p95/p99. Software WebGL runs are correctness checks, not desktop GPU performance evidence.
