# Tasks — SPEC 074
All tasks open. Evidence includes exact SHA, OS/architecture, dataset, command/test ID and output. A checkbox is not acceptance by itself.

## W0 — architecture and qualification
- [ ] T001 Inventory main + latest PR #103, actual crate paths, dependency DAG, server-only dependencies and shared ownership.
- [ ] T002 Freeze shared storage/engine/sync/plugin contract versions; identify 073 dependencies and non-overlapping desktop paths.
- [ ] T003 Tauri WebView and React build spike on Windows, macOS, Linux; assess MapLibre/Cesium and GPU support.
- [ ] T004 Audit dependency licensing, bundling, native libraries, signing requirements and threat model.

## W1 — desktop host
- [ ] T005 Add Tauri app with pinned toolchain and single React build source.
- [ ] T006 Add typed desktop transport, capability handshake and bounded IPC; preserve HTTP transport.
- [ ] T007 Shared engine handles/cancellation, lifecycle and crash containment.
- [ ] T008 Desktop window/menu/keyboard and standalone project picker; no forced account.

## W2 — local data
- [ ] T009 Integrate qualified 073 local storage and schema; standalone standard GeoPackage roundtrip.
- [ ] T010 Implement project persistence, file grants, import/export and bounded pages.
- [ ] T011 Attribute/geometry editing, revision, undo/redo, journal and atomic write behavior.
- [ ] T012 Validate migration, disk-full, corruption, restart and backup/restore.
- [ ] T013 Enforce local vs managed workspace isolation and capability flags.

## W3 — GIS and desktop workspace
- [ ] T014 Integrate shared CRS and geometry kernels; test VN-2000 control points.
- [ ] T015 Local processing capability registry and golden PostGIS comparison; unsupported operations disabled.
- [ ] T016 Qualify vector/raster/CAD/point-cloud adapters by individual format.
- [ ] T017 Qualify map render/picking/snap, layout printing/export and multi-monitor workflows.
- [ ] T018 Benchmark memory, large datasets, frame time, cancellation and UI responsiveness.

## W4 — connected and hybrid
- [ ] T019 System-browser auth, secure credentials, trusted server selection and logout.
- [ ] T020 Reuse 073 managed local store and offline lease; prohibit unmanaged fallback for protected data.
- [ ] T021 Reuse existing sync API; durable receipt, cursor, tombstone, conflicts and idempotency.
- [ ] T022 Test two-client editing, revocation, reconnect, duplicate replay and project isolation on real isolated PostGIS.
- [ ] T023 Run existing Web/API compatibility and regression checks; no production migration.

## W5 — plugin and release
- [ ] T024 Desktop declarative plugin host/manifest capability mapping; sample vertical.
- [ ] T025 IPC, hostile input, auth, permissions, update and dependency security tests.
- [ ] T026 Windows x64 installer and signed upgrade/rollback qualification.
- [ ] T027 macOS Apple Silicon notarization and real-device qualification.
- [ ] T028 Linux x64 AppImage/deb qualification.
- [ ] T029 Accessibility, localization, documentation, SBOM and release evidence.
- [ ] T030 Full integrated CI and independent acceptance per release OS; no unchecked platform release.

## W6 — optional
- [ ] T031 DuckDB Spatial/GeoParquet read-only analytics spike; benchmark and license review.
- [ ] T032 Native GPU provider spike with measured benefit; decision before implementation.
