# Execution matrix — SPEC 073 Revision 2

78 stable parent IDs remain in tasks.md; every leaf below is a separately assigned implementation/check unit. All open. Paths are proposed ownership roots, not claims of existing files; W0 freezes exact paths and owners. A leaf may start only after its parent dependencies complete (except reviewed independent spike); leaves inside a parent run in order unless owner explicitly separates them. Shared server identity/registry, cargo workspace, lockfiles, schemas and FFI generated files have one integration owner; no simultaneous edits to these files. Optional features T039/T064/T066/T077 follow separate capability/profile gates; mandatory Android work is not blocked by optional I1. Each leaf records exact candidate, path, test IDs and evidence; parent closes only after all required leaves pass and optional leaves have explicit disposition.

### T001 — E01 | FR-003,029 | SC-009,010
Depends: none. Owner paths: `specs/073*/evidence; Cargo.toml (read)`.
- [ ] T001.1 inventory Rust symbols and dependencies.
- [ ] T001.2 inventory API/SDK and fixtures.
- [ ] T001.3 Verify: inventory coverage check; baseline SHA and graph recorded. Attach evidence.

### T002 — E01 | FR-003,026 | SC-009,010
Depends: T001. Owner paths: `infra/mobile/toolchain; build scripts`.
- [ ] T002.1 lock native and Flutter toolchain.
- [ ] T002.2 record dependency hashes/MSRV/licenses.
- [ ] T002.3 Verify: reproducible clean setup; lockfile validation. Attach evidence.

### T003 — E01 | FR-005,007 | SC-009,010
Depends: T002. Owner paths: `infra/mobile/native; tests/mobile/abi`.
- [ ] T003.1 build Android native dependencies.
- [ ] T003.2 exercise IO/CRS and 4K/16K ELF alignment.
- [ ] T003.3 iOS feasibility separately.
- [ ] T003.4 Verify: release native load, driver probe, control point; 16KB runtime. Attach evidence.

### T004 — E01 | FR-004,021 | SC-009,010
Depends: T002. Owner paths: `apps/mobile-flutter spike; crates/ovg-mobile-ffi spike`.
- [ ] T004.1 prove MapLibre source transport.
- [ ] T004.2 measure bridge batches and lifecycle.
- [ ] T004.3 Verify: physical device map/FFI cancellation and copy baseline. Attach evidence.

### T005 — E16 | FR-025,030 | SC-009,010
Depends: T001. Owner paths: `specs/073*/evidence; tools/benchmark/mobile`.
- [ ] T005.1 freeze threat model and fixtures.
- [ ] T005.2 freeze device and resource budgets.
- [ ] T005.3 license/store inventory.
- [ ] T005.4 Verify: fixture checksum and independent oracle review. Attach evidence.

### T006 — E10 | FR-015,028 | SC-006,007
Depends: T001. Owner paths: `apps/api-rust/src (read); specs/073*/contracts`.
- [ ] T006.1 inventory human/service credential routes.
- [ ] T006.2 inventory sync revision/receipt/lease.
- [ ] T006.3 freeze additive auth contract.
- [ ] T006.4 Verify: route coverage and token-type negative contract. Attach evidence.

### T007 — E11 | FR-016,017 | SC-007,008
Depends: T001. Owner paths: `packages/plugin-sdk (read); extensions validation (read)`.
- [ ] T007.1 inventory existing v1 signatures and validator.
- [ ] T007.2 freeze v2 envelope and capability schema.
- [ ] T007.3 Verify: v1 golden manifest and cross-language signature plan. Attach evidence.

### T008 — E01 | FR-029 | SC-009,010
Depends: T003, T004, T005, T006, T007. Owner paths: `specs/073*/evidence`.
- [ ] T008.1 review spike results and graph.
- [ ] T008.2 approve or explicitly block dependent capabilities.
- [ ] T008.3 Verify: A0 signed review and exact SHA. Attach evidence.

### T009 — E02 | FR-003 | SC-003,006,010
Depends: T008. Owner paths: `crates/ovg-core; ovg-spatial; ovg-processing; ovg-sync`.
- [ ] T009.1 extract portable types and traits.
- [ ] T009.2 retain server adapters.
- [ ] T009.3 add dependency guard.
- [ ] T009.4 Verify: cargo tree excludes server dependencies; server regression. Attach evidence.

### T010 — E02 | FR-003,021 | SC-003,006,010
Depends: T009. Owner paths: `crates/ovg-mobile-engine`.
- [ ] T010.1 implement handle generations and scope.
- [ ] T010.2 implement bounded jobs/cancel/dispose.
- [ ] T010.3 Verify: stale handle, panic, cancellation and lifecycle tests. Attach evidence.

### T011 — E02 | FR-021 | SC-003,006,010
Depends: T010. Owner paths: `crates/ovg-mobile-ffi; packages/dart-sdk/native`.
- [ ] T011.1 generate typed bridge.
- [ ] T011.2 implement bounded batches/release semantics.
- [ ] T011.3 Verify: ABI/copy count/overflow/parallel close tests. Attach evidence.

### T012 — E03 | FR-001,002 | SC-001,009
Depends: T008. Owner paths: `apps/mobile-flutter/lib/shell; app bootstrap`.
- [ ] T012.1 implement Flutter project and theme.
- [ ] T012.2 implement map-first guest/local shell.
- [ ] T012.3 Verify: widget test and fresh install offline. Attach evidence.

### T013 — E03 | FR-001,027 | SC-001,009
Depends: T012. Owner paths: `apps/mobile-flutter/lib/navigation; l10n`.
- [ ] T013.1 implement navigation and bottom sheets.
- [ ] T013.2 add vi/en/accessibility/permission UX.
- [ ] T013.3 Verify: 360px layout, touch/focus, denied permission test. Attach evidence.

### T014 — E03 | FR-002 | SC-001,009
Depends: T012. Owner paths: `apps/mobile-flutter/lib/workspaces`.
- [ ] T014.1 implement distinct workspace IDs and selection.
- [ ] T014.2 explicit connect/upload and restore.
- [ ] T014.3 Verify: cross-mode collision and restart test. Attach evidence.

### T015 — E08 | FR-012 | SC-001,006,007
Depends: T009. Owner paths: `crates/ovg-local-storage/schema; repository`.
- [ ] T015.1 implement schema/IDs/RTree.
- [ ] T015.2 transactional write and migrations.
- [ ] T015.3 Verify: SQLite integrity, migration and spatial index oracle. Attach evidence.

### T016 — E08 | FR-012,025 | SC-001,006,007
Depends: T015. Owner paths: `crates/ovg-local-storage/recovery`.
- [ ] T016.1 implement quotas/WAL/atomic staging.
- [ ] T016.2 corruption recovery and cleanup.
- [ ] T016.3 Verify: fault injection, disk full and interrupted export. Attach evidence.

### T017 — E08 | FR-012,014 | SC-001,006,007
Depends: T016. Owner paths: `crates/ovg-local-storage/journal`.
- [ ] T017.1 implement feature+outbox ACID.
- [ ] T017.2 receipt/cursor ACID and operation hash.
- [ ] T017.3 Verify: kill before/after commit; replay idempotency. Attach evidence.

### T018 — E16 | FR-026,029 | SC-009,010
Depends: T011, T013, T014, T017. Owner paths: `tests/mobile/kernel; tests/mobile/shell`.
- [ ] T018.1 run native unit and Flutter smoke.
- [ ] T018.2 run impacted existing Rust/Web tests.
- [ ] T018.3 Verify: record exact candidate and targeted CI results. Attach evidence.

### T019 — E04 | FR-004 | SC-002,003
Depends: T011, T013. Owner paths: `apps/mobile-flutter/lib/map`.
- [ ] T019.1 integrate native map and camera lifecycle.
- [ ] T019.2 implement source attribution and gestures.
- [ ] T019.3 Verify: rotation/background/source selection tests. Attach evidence.

### T020 — E04 | FR-004,021 | SC-002,003
Depends: T019, T015. Owner paths: `apps/mobile-flutter/lib/map/layers; native map adapter`.
- [ ] T020.1 implement layer styles/visibility.
- [ ] T020.2 selection and feature state.
- [ ] T020.3 Verify: style capability and hit-test fixture. Attach evidence.

### T021 — E04 | FR-021 | SC-002,003
Depends: T020. Owner paths: `crates/ovg-mobile-engine/viewport; apps/mobile-flutter/lib/map/cache`.
- [ ] T021.1 implement indexed tiles/batches and epochs.
- [ ] T021.2 implement adaptive cache/LOD.
- [ ] T021.3 Verify: 10k/100k/1M fixture, stale response and RSS. Attach evidence.

### T022 — E04 | FR-013 | SC-002,003
Depends: T019, T016. Owner paths: `apps/mobile-flutter/lib/map/offline; native tile adapters`.
- [ ] T022.1 implement archive readers.
- [ ] T022.2 implement licensed download/resume/eviction.
- [ ] T022.3 Verify: corrupt archive, quota, cancel and attribution. Attach evidence.

### T023 — E05 | FR-005,006 | SC-001,004
Depends: T011, T016. Owner paths: `apps/mobile-flutter/lib/import; crates/ovg-mobile-engine/io/staging`.
- [ ] T023.1 implement OS share-in and picker.
- [ ] T023.2 implement bounded ZIP and companions.
- [ ] T023.3 Verify: traversal/bomb/missing companion and cancellation. Attach evidence.

### T024 — E05 | FR-005 | SC-001,004
Depends: T023. Owner paths: `crates/ovg-mobile-engine/io/vector`.
- [ ] T024.1 qualify GPKG/GeoJSON/GPX.
- [ ] T024.2 qualify SHP/TAB/MIF/KML and companion/encoding.
- [ ] T024.3 Verify: per-driver independent roundtrip and loss fixtures. Attach evidence.

### T025 — E05 | FR-005 | SC-001,004
Depends: T023. Owner paths: `crates/ovg-mobile-engine/io/tabular; io/cad`.
- [ ] T025.1 qualify CSV/XLSX coordinate mapping.
- [ ] T025.2 qualify 2D DXF subset.
- [ ] T025.3 Verify: axis/units, formulas, unsupported entities and roundtrip. Attach evidence.

### T026 — E05 | FR-005,006 | SC-001,004
Depends: T024, T025. Owner paths: `crates/ovg-mobile-engine/io/export; apps/mobile-flutter/lib/export`.
- [ ] T026.1 implement export staging/loss report.
- [ ] T026.2 implement share-out and cancellation.
- [ ] T026.3 Verify: QGIS/GDAL independent output comparison. Attach evidence.

### T027 — E06 | FR-007 | SC-005
Depends: T011, T003. Owner paths: `crates/ovg-mobile-engine/crs`.
- [ ] T027.1 pin PROJ DB/grids and CRS registry.
- [ ] T027.2 implement transform validation.
- [ ] T027.3 Verify: axis, units, missing grids and unsupported transforms. Attach evidence.

### T028 — E06 | FR-007 | SC-005
Depends: T027, T005. Owner paths: `crates/ovg-mobile-engine/crs/vietnam; tests/mobile/crs`.
- [ ] T028.1 freeze Vietnam pipeline controls.
- [ ] T028.2 implement qualified regional transforms.
- [ ] T028.3 Verify: independent control-point tolerances and reject unsupported region. Attach evidence.

### T029 — E06 | FR-007,010 | SC-005
Depends: T027. Owner paths: `crates/ovg-mobile-engine/measure`.
- [ ] T029.1 implement geodesic/projected measures.
- [ ] T029.2 batch coordinates and bearing.
- [ ] T029.3 Verify: oracle fixtures for distance/area/units. Attach evidence.

### T030 — E05 | FR-005,022 | SC-001,004
Depends: T023, T027. Owner paths: `crates/ovg-mobile-engine/io/raster`.
- [ ] T030.1 qualify tiled GeoTIFF/COG.
- [ ] T030.2 implement safe ranges and nodata.
- [ ] T030.3 Verify: block-read memory/metadata and hostile URL tests. Attach evidence.

### T031 — E16 | FR-026,030 | SC-009,010
Depends: T021, T022, T026, T028, T029, T030. Owner paths: `tests/mobile/formats; tools/benchmark/mobile`.
- [ ] T031.1 run IO/CRS oracles.
- [ ] T031.2 collect map device benchmark.
- [ ] T031.3 Verify: record capability matrix and no unsupported driver claim. Attach evidence.

### T032 — E07 | FR-008 | SC-001,003,005
Depends: T020, T017, T027. Owner paths: `apps/mobile-flutter/lib/edit; crates/ovg-mobile-engine/edit`.
- [ ] T032.1 implement drawing and preview.
- [ ] T032.2 validate topology and commit command.
- [ ] T032.3 Verify: geometry fixture and preview nonmutation. Attach evidence.

### T033 — E07 | FR-008,021 | SC-001,003,005
Depends: T032, T021, T029. Owner paths: `apps/mobile-flutter/lib/edit/snap; native spatial index`.
- [ ] T033.1 implement indexed vertex/edge queries.
- [ ] T033.2 implement touch handles and stale cancellation.
- [ ] T033.3 Verify: snap correctness and p95 fixture. Attach evidence.

### T034 — E07 | FR-008 | SC-001,003,005
Depends: T032, T017. Owner paths: `crates/ovg-mobile-engine/edit/history`.
- [ ] T034.1 implement undo/redo and split/merge.
- [ ] T034.2 implement revision audit and recovery.
- [ ] T034.3 Verify: undo/redo, cancel, invalid topology and crash. Attach evidence.

### T035 — E07 | FR-009 | SC-001,003,005
Depends: T032, T015. Owner paths: `apps/mobile-flutter/lib/attributes; native schema adapter`.
- [ ] T035.1 implement typed fields and constraints.
- [ ] T035.2 implement forms/filter/table.
- [ ] T035.3 Verify: field validation, virtual list and schema fixtures. Attach evidence.

### T036 — E09 | FR-010 | SC-001,005,009
Depends: T013, T029, T015. Owner paths: `apps/mobile-flutter/lib/survey/gps`.
- [ ] T036.1 implement GPS quality/provider and permission.
- [ ] T036.2 implement waypoint/bearing.
- [ ] T036.3 Verify: mock provider and physical-device recording. Attach evidence.

### T037 — E09 | FR-010 | SC-001,005,009
Depends: T036, T017. Owner paths: `apps/mobile-flutter/lib/survey/tracks`.
- [ ] T037.1 implement background OS lifecycle.
- [ ] T037.2 implement segments/export/recovery.
- [ ] T037.3 Verify: screen-off permission, kill/resume and GPX oracle. Attach evidence.

### T038 — E09 | FR-011 | SC-001,005,009
Depends: T013, T016. Owner paths: `apps/mobile-flutter/lib/survey/media`.
- [ ] T038.1 implement photo/QR consent and metadata.
- [ ] T038.2 implement durable encrypted blob/hash queue.
- [ ] T038.3 Verify: permission denial, orphan cleanup and privacy. Attach evidence.

### T039 — E09 | FR-011 | SC-001,005,009
Depends: T036, T028. Owner paths: `apps/mobile-flutter/lib/survey/gnss`.
- [ ] T039.1 implement NMEA receiver adapter.
- [ ] T039.2 implement NTRIP and fix metadata.
- [ ] T039.3 Verify: real receiver and mock invalid/correction-loss tests. Attach evidence.

### T040 — E08 | FR-012,013,025 | SC-001,006,007
Depends: T016, T017, T022, T038. Owner paths: `crates/ovg-local-storage/security; apps/mobile-flutter/lib/offline`.
- [ ] T040.1 implement scoped keys/encrypted DB.
- [ ] T040.2 implement WAL/temp/backup checks and lifecycle.
- [ ] T040.3 Verify: at-rest leakage, logout and independent local file test. Attach evidence.

### T041 — E16 | FR-026 | SC-009,010
Depends: T031, T033, T034, T035, T037, T038, T040. Owner paths: `tests/mobile/standalone`.
- [ ] T041.1 run fresh airplane-mode local workflow.
- [ ] T041.2 independently verify exports.
- [ ] T041.3 Verify: A1 physical Android acceptance; RTK T039 separate qualification. Attach evidence.

### T042 — E10 | FR-028 | SC-006,007
Depends: T006, T008. Owner paths: `packages/dart-sdk/api`.
- [ ] T042.1 generate current OpenAPI bindings.
- [ ] T042.2 implement facade and version negotiation.
- [ ] T042.3 Verify: contract golden and generated diff guard. Attach evidence.

### T043 — E10 | FR-015 | SC-006,007
Depends: T042, T006. Owner paths: `apps/api-rust/src/identity (shared owner); packages/dart-sdk/auth`.
- [ ] T043.1 implement reviewed server human mobile credential.
- [ ] T043.2 implement PKCE/refresh/revoke and secure store.
- [ ] T043.3 Verify: wrong issuer/redirect, service confusion, reuse and web CSRF regression. Attach evidence.

### T044 — E10 | FR-002,025 | SC-006,007
Depends: T043, T014. Owner paths: `apps/mobile-flutter/lib/connected`.
- [ ] T044.1 implement trusted server selection and project discovery.
- [ ] T044.2 implement scope teardown.
- [ ] T044.3 Verify: same IDs across servers, account switch and revocation. Attach evidence.

### T045 — E10 | FR-014 | SC-006,007
Depends: T044, T040. Owner paths: `packages/dart-sdk/sync/packages; crates/ovg-local-storage`.
- [ ] T045.1 implement signed download/staging.
- [ ] T045.2 verify expiry, encryption and atomic swap.
- [ ] T045.3 Verify: tamper, partial package, storage full and resume. Attach evidence.

### T046 — E10 | FR-014 | SC-006,007
Depends: T045, T017. Owner paths: `packages/dart-sdk/sync/delta; crates/ovg-local-storage/journal`.
- [ ] T046.1 implement ordered delta/cursor and tombstones.
- [ ] T046.2 implement gap bootstrap and rebase.
- [ ] T046.3 Verify: duplicate/gap/schema conflict and cursor atomicity. Attach evidence.

### T047 — E10 | FR-014 | SC-006,007
Depends: T046. Owner paths: `packages/dart-sdk/sync/commits; apps/mobile-flutter/lib/conflicts`.
- [ ] T047.1 implement stable receipt/retry state machine.
- [ ] T047.2 implement conflict UI/rebase.
- [ ] T047.3 Verify: unknown ACK, expired receipt and no duplicate mutation. Attach evidence.

### T048 — E10 | FR-014,025 | SC-006,007
Depends: T045, T043. Owner paths: `packages/dart-sdk/sync/lease; crates/ovg-local-storage/security`.
- [ ] T048.1 implement mandatory protected-data lease.
- [ ] T048.2 implement clock/reboot/revoke lock and key lifecycle.
- [ ] T048.3 Verify: expired/rollback/reboot, purge and no local loss. Attach evidence.

### T049 — E10 | FR-024 | SC-006,007
Depends: T044, T046. Owner paths: `packages/dart-sdk/realtime; apps/mobile-flutter/lib/connected/events`.
- [ ] T049.1 implement scoped subscriptions and reconnect.
- [ ] T049.2 implement backpressure/cancellation.
- [ ] T049.3 Verify: switch project, duplicate and mid-stream revoke. Attach evidence.

### T050 — E16 | FR-026,025 | SC-009,010
Depends: T047, T048, T049, T041. Owner paths: `tests/mobile/connected; tests/rust-parity`.
- [ ] T050.1 run isolated PostGIS and concurrent Web edits.
- [ ] T050.2 100 crash/retry cases.
- [ ] T050.3 Verify: A2 permission and data integrity evidence. Attach evidence.

### T051 — E11 | FR-016,028 | SC-007,008
Depends: T007, T042. Owner paths: `packages/flutter-plugin-sdk/contracts; extensions validator (shared owner)`.
- [ ] T051.1 implement additive v2 envelope.
- [ ] T051.2 implement signing, asset and trust rotation.
- [ ] T051.3 Verify: v1 golden unchanged, cross-language v2 and tamper tests. Attach evidence.

### T052 — E11 | FR-017 | SC-007,008
Depends: T051, T035. Owner paths: `packages/flutter-plugin-sdk/runtime`.
- [ ] T052.1 implement typed bounded UI.
- [ ] T052.2 implement action broker and schema validation.
- [ ] T052.3 Verify: invalid graph, queue/timeout and unknown capability tests. Attach evidence.

### T053 — E11 | FR-016,017,025 | SC-007,008
Depends: T052, T048. Owner paths: `packages/flutter-plugin-sdk/security; apps/mobile-flutter/lib/plugins`.
- [ ] T053.1 implement project activation and permissions.
- [ ] T053.2 consent/lifecycle/cleanup.
- [ ] T053.3 Verify: cross-project, revoke, device denial and no token access. Attach evidence.

### T054 — E11 | FR-016 | SC-007,008
Depends: T051, T053. Owner paths: `apps/mobile-flutter/lib/plugins/catalog`.
- [ ] T054.1 integrate catalog discovery and install states.
- [ ] T054.2 compatibility/offline expiry UX.
- [ ] T054.3 Verify: unsupported host version and revoked manifest tests. Attach evidence.

### T055 — E11 | FR-028 | SC-007,008
Depends: T052, T053. Owner paths: `packages/flutter-plugin-sdk/examples; docs`.
- [ ] T055.1 provide generator/template and sample.
- [ ] T055.2 publish capability/action docs and test harness.
- [ ] T055.3 Verify: clean first plugin quickstart <=30 min. Attach evidence.

### T056 — E12 | FR-018 | SC-008
Depends: T053, T038. Owner paths: `modules/house_numbering/mobile`.
- [ ] T056.1 implement map workspace and forms.
- [ ] T056.2 connect survey/photo/proposal APIs.
- [ ] T056.3 Verify: reference workflow uses same IDs and permissions. Attach evidence.

### T057 — E12 | FR-018 | SC-008
Depends: T056, T047. Owner paths: `modules/house_numbering/mobile/workflows`.
- [ ] T057.1 implement review/approval and authorized offline actions.
- [ ] T057.2 verify Web visibility.
- [ ] T057.3 Verify: conflict, revocation and approved-only publication. Attach evidence.

### T058 — E12 | FR-018 | SC-008
Depends: T055, T054. Owner paths: `modules/mobile_reference_sample`.
- [ ] T058.1 implement generic example with no Core imports.
- [ ] T058.2 exercise disable/data retention.
- [ ] T058.3 Verify: architecture guard and end-to-end install. Attach evidence.

### T059 — E16 | FR-025,026 | SC-009,010
Depends: T057, T058, T054, T050. Owner paths: `tests/mobile/plugins`.
- [ ] T059.1 run hostile signature/manifest and broker tests.
- [ ] T059.2 run both reference workflows.
- [ ] T059.3 Verify: A3 integration evidence. Attach evidence.

### T060 — E13 | FR-020 | SC-003,004,005,010
Depends: T029, T026, T017. Owner paths: `crates/ovg-processing/local_adapters`.
- [ ] T060.1 implement buffer/clip.
- [ ] T060.2 implement dissolve and provenance.
- [ ] T060.3 Verify: independent oracle with parameter/CRS limits. Attach evidence.

### T061 — E13 | FR-020 | SC-003,004,005,010
Depends: T060. Owner paths: `crates/ovg-processing/local_adapters`.
- [ ] T061.1 implement difference/intersection/union.
- [ ] T061.2 implement reproject/spatial join.
- [ ] T061.3 Verify: topology, attributes, null and CRS differential tests. Attach evidence.

### T062 — E13 | FR-020,021 | SC-003,004,005,010
Depends: T061, T010. Owner paths: `crates/ovg-mobile-engine/jobs; apps/mobile-flutter/lib/jobs`.
- [ ] T062.1 implement local/server selection and progress.
- [ ] T062.2 implement cancellation/atomic output.
- [ ] T062.3 Verify: bounded jobs, cancel-before/after commit and no partial output. Attach evidence.

### T063 — E13 | FR-022 | SC-003,004,005,010
Depends: T021, T024. Owner paths: `tools/benchmark/mobile/columnar; evidence`.
- [ ] T063.1 benchmark Arrow/GeoArrow and GeoParquet/FlatGeobuf.
- [ ] T063.2 compare Rust driver candidates.
- [ ] T063.3 Verify: measured RAM/copy/latency/size and adopt/reject record. Attach evidence.

### T064 — E13 | FR-022 | SC-003,004,005,010
Depends: T030, T021, T005. Owner paths: `tools/benchmark/mobile/advanced; evidence`.
- [ ] T064.1 evaluate DataFusion/SedonaDB/wgpu.
- [ ] T064.2 evaluate terrain/COPC/3D.
- [ ] T064.3 Verify: measured device feasibility and explicit capability gating. Attach evidence.

### T065 — E14 | FR-019 | SC-007,008,009
Depends: T051, T043. Owner paths: `apps/api-rust/src/extensions (shared owner); packages/dart-sdk/entitlement`.
- [ ] T065.1 define reviewed entitlement model.
- [ ] T065.2 implement server authority separate from permission.
- [ ] T065.3 Verify: expired/revoked/unpaid and cross-tenant tests. Attach evidence.

### T066 — E14 | FR-019,025 | SC-007,008,009
Depends: T065, T048. Owner paths: `packages/flutter-plugin-sdk/entitlement`.
- [ ] T066.1 implement optional signed offline entitlement.
- [ ] T066.2 enforce expiry and data export boundary.
- [ ] T066.3 Verify: clock rollback, revoke and authorized Core export. Attach evidence.

### T067 — E14 | FR-019 | SC-007,008,009
Depends: T005, T065. Owner paths: `specs/073*/evidence/licensing; infra/mobile`.
- [ ] T067.1 review AGPL/SDK rights and store policies.
- [ ] T067.2 document payment boundary.
- [ ] T067.3 Verify: legal/policy review record, no checkout claim. Attach evidence.

### T068 — E15 | FR-023 | SC-001,007
Depends: T019, T036, T044. Owner paths: `apps/mobile-flutter/lib/search; packages/dart-sdk/search`.
- [ ] T068.1 implement local coordinate/feature search.
- [ ] T068.2 reuse remote routing and export.
- [ ] T068.3 Verify: offline search, provider outage, consent and attribution. Attach evidence.

### T069 — E15 | FR-024 | SC-001,007
Depends: T049. Owner paths: `apps/mobile-flutter/lib/collaboration`.
- [ ] T069.1 integrate chat/tasks.
- [ ] T069.2 implement notification scope and offline behavior.
- [ ] T069.3 Verify: reconnect, revoke and cross-project tests. Attach evidence.

### T070 — E16 | FR-030 | SC-009,010
Depends: T062, T063, T064, T067, T068, T069. Owner paths: `specs/073*/evidence/advanced`.
- [ ] T070.1 review adoption outcomes and footprint.
- [ ] T070.2 reconcile profile flags.
- [ ] T070.3 Verify: capability matrix and reviewed optional rejections. Attach evidence.

### T071 — E16 | FR-030 | SC-009,010
Depends: T059, T070. Owner paths: `tools/benchmark/mobile; tests/mobile/performance`.
- [ ] T071.1 run cold start/frame/snap/RSS profiles.
- [ ] T071.2 run battery/thermal/soak.
- [ ] T071.3 Verify: physical-device raw p50/p95/p99 and budgets. Attach evidence.

### T072 — E16 | FR-025 | SC-009,010
Depends: T059, T070. Owner paths: `tests/mobile/security; specs/073*/evidence/security`.
- [ ] T072.1 review hostile data and FFI.
- [ ] T072.2 review auth/encryption/supply chain and remediate.
- [ ] T072.3 Verify: independent findings and regression evidence. Attach evidence.

### T073 — E16 | FR-026 | SC-009,010
Depends: T059, T070. Owner paths: `tests/mobile/lifecycle; infra/mobile/migration`.
- [ ] T073.1 test lifecycle and low resources.
- [ ] T073.2 test migration/rollback.
- [ ] T073.3 Verify: device fault and no acknowledged data loss. Attach evidence.

### T074 — E16 | FR-026,029 | SC-009,010
Depends: T071, T072, T073. Owner paths: `tests/mobile; tests/rust-parity; apps/web-next tests`.
- [ ] T074.1 run integrated exact candidate suite.
- [ ] T074.2 fix failures and rerun.
- [ ] T074.3 Verify: final CI and QGIS oracle. Attach evidence.

### T075 — E16 | FR-026,028 | SC-009,010
Depends: T074. Owner paths: `docs/mobile; packages/flutter-plugin-sdk/docs`.
- [ ] T075.1 complete vi/en docs and SDK.
- [ ] T075.2 publish driver/CRS limitations and source offer.
- [ ] T075.3 Verify: clean quickstart and SBOM/NOTICE check. Attach evidence.

### T076 — E16 | FR-026 | SC-009,010
Depends: T075, T003. Owner paths: `infra/mobile/release/android`.
- [ ] T076.1 build signed AAB/APK and inspect 4K/16K splits.
- [ ] T076.2 verify upgrade/device rollout.
- [ ] T076.3 Verify: artifact digest, signed package and alignment evidence. Attach evidence.

### T077 — E16 | FR-026 | SC-009,010
Depends: T075. Owner paths: `infra/mobile/release/ios`.
- [ ] T077.1 qualify native iOS device/simulator.
- [ ] T077.2 check store policy and signed artifact.
- [ ] T077.3 Verify: I1 separate gate, does not block Android. Attach evidence.

### T078 — E16 | FR-026,029,030 | SC-009,010
Depends: T076. Owner paths: `specs/073*/evidence/release`.
- [ ] T078.1 reconcile all FR/SC/task evidence and candidate.
- [ ] T078.2 independent final review.
- [ ] T078.3 Verify: A4 Android release decision, I1 tracked separately. Attach evidence.

Total: 238 execution leaves across 78 parent tasks. Parent dependencies apply to each leaf; no implied acceptance from task generation.


## W0 execution status (2026-11-28)
See [execution ledger](evidence/execution-ledger.md) and [W0 baseline evidence](evidence/w0-baseline-checkpoint.md). Candidate graph and matrix audit passed on db2ef12; T001 remains in progress pending complete inventory/review. No acceptance checkbox changed.


### W0 continued evidence
See [execution ledger](evidence/execution-ledger.md), [auth/sync/plugin inventory](evidence/w0-auth-sync-plugin-inventory.md), and [threat model](evidence/w0-threat-model.md). Candidate qualification workflow and baseline auth/signature tests implemented; no mandatory parent accepted. Device/FFI proof, independent review and A0 remain open.
