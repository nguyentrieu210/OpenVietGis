# Tasks — SPEC 073
Revision 2: 78 stable parent tasks and 238 execution leaves in [execution-matrix.md](execution-matrix.md). All tasks open. No task is accepted on code presence alone. Each task must record exact commit, files, tests/fixtures, result and remaining blockers. Parallelize independent engines after contract freeze; integration owner controls shared ABI/API/schema. Targeted tests during development; full integrated CI at convergence.

## W0 — Baseline and feasibility
- [ ] T001 [E01] [FR-003,029] Pin main; inventory routes, SDK, plugin schemas, existing portable functions and cargo dependency graph; map every reused/replaced adapter to evidence.
- [ ] T002 [E01] [FR-003,026] Lock Flutter/Dart/Rust/NDK/SDK and native dependency provenance; record MSRV/ABI decisions and build manifest.
- [ ] T003 [E01] [FR-005,007] Build Android ARM64 spike GDAL/OGR/GEOS/PROJ/SQLite; test driver list, real file IO and control point; iOS feasibility on macOS.
- [ ] T004 [E01] [FR-004,021] Run MapLibre Flutter native map and Rust bridge spike; test frame, RAM, texture lifecycle, copy budget and cancellation.
- [ ] T005 [E16] [FR-025,030] Record threat model, SBOM/license and platform policy; select physical devices, fixture checksums and independent oracle/tolerances.
- [ ] T006 [E10] [FR-015,028] Inventory actual API auth/PKCE/refresh/revoke and sync routes; document gaps; no invented route or downgraded auth.
- [ ] T007 [E11] [FR-016,017] Inventory existing SPEC 071 manifest, registry, signing, permissions, UI slots; additive compatibility plan.
- [ ] T008 [E01] [FR-029] Gate A0: review spikes and approve native baseline; block dependent work only on proven incompatibilities.

## W1 — Portable kernel, shell, storage
- [ ] T009 [E02] [FR-003] Extract reusable pure types/algorithms/contracts into portable dependency graph; preserve server builds.
- [ ] T010 [E02] [FR-003,021] Define engine session/workspace/layer/job handles, typed errors, async cancellation, explicit ownership, panic containment.
- [ ] T011 [E02] [FR-021] Generate FFI bindings, batch WKB/typed buffer paging, size caps and lifecycle tests; measure actual copies.
- [ ] T012 [E03] [FR-001,002] Create Flutter app and shared design tokens (SPEC 072), map-first shell, local mode without signup.
- [ ] T013 [E03] [FR-001,027] Build projects/tools/plugin/account navigation, bottom sheets, vi/en, accessible touch/focus and permission-denied flows.
- [ ] T014 [E03] [FR-002] Separate local workspace namespace from managed projects, explicit connect/upload intent, restore last local workspace.
- [ ] T015 [E08] [FR-012] Implement local GeoPackage/SQLite repository, RTree, stable IDs, schema migration and one-writer transaction boundary.
- [ ] T016 [E08] [FR-012,025] Implement atomic staged writes, WAL/checkpoint, quota, recovery/export and corrupted DB handling; isolate metadata.
- [ ] T017 [E08] [FR-012,014] Implement local change journal, durable outbox and atomic receipt/cursor state; inject kill/restart faults.
- [ ] T018 [E16] [FR-026,029] Run focused native/unit/Flutter tests and existing Web/Server regression for changed shared crates.

## W2 — Rendering, file IO, CRS
- [ ] T019 [E04] [FR-004] Integrate native map with gestures, scale/north, camera lifecycle, basemap registry and attribution.
- [ ] T020 [E04] [FR-004,021] Implement layer visibility/order/style/labels, feature hit-test and selection; capability warnings for unsupported styles.
- [ ] T021 [E04] [FR-021] Implement indexed viewport paging, tile/LOD rendering and bounded cache; benchmark 10k/100k/1M source fixtures.
- [ ] T022 [E04] [FR-013] Support licensed offline map download/resume/quota/eviction and user MBTiles/PMTiles; test corrupt archives.
- [ ] T023 [E05] [FR-005,006] Implement OS share-in/file picker, staging, zip limits and companion file resolver.
- [ ] T024 [E05] [FR-005] Read/write qualified SHP, TAB/MIF/MID, GeoPackage, GeoJSON, KML/KMZ and GPX with schema/CRS provenance.
- [ ] T025 [E05] [FR-005] Implement coordinate CSV/XLSX and bounded DXF subset; explicit field/geometry loss report; DGN gated.
- [ ] T026 [E05] [FR-005,006] Export to portable formats, share-out and round-trip oracle; atomic cancellation and partial-file cleanup.
- [ ] T027 [E06] [FR-007] Implement explicit source/storage/display CRS registry and transformation adapter with pinned PROJ assets.
- [ ] T028 [E06] [FR-007] Add Vietnam regional VN-2000 pipelines with independently approved control points and reject unqualified datum conversions.
- [ ] T029 [E06] [FR-007,010] Point/batch transform, geodesic/projected measure, units/axis/bearing; compare to independent oracle.
- [ ] T030 [E05] [FR-005,022] Qualify GeoTIFF/COG tiled/block access, metadata and safe range requests; label unsupported drivers.
- [ ] T031 [E16] [FR-026,030] Record IO/CRS/renderer fixtures and device performance evidence; no feature claim without qualified driver.

## W3 — Editing, field survey and offline
- [ ] T032 [E07] [FR-008] Draw and edit Point/Line/Polygon and qualified multis; transient overlay and geometry validation.
- [ ] T033 [E07] [FR-008,021] Implement spatially indexed snapping, touch vertex handles, hit-test tolerance and cancellation.
- [ ] T034 [E07] [FR-008] Implement undo/redo, split/merge, commit/cancel and revision-aware audit; explicit unsupported geometry.
- [ ] T035 [E07] [FR-009] Typed attributes/defaults/constraints, virtualized table, filter/search/select and reusable forms.
- [ ] T036 [E09] [FR-010] GPS accuracy/provider, permission state, waypoint and bearing/distance navigation; mock and real device tests.
- [ ] T037 [E09] [FR-010] Track recording segments, background lifecycle and OS notification, pause/resume, export GPX and crash recovery.
- [ ] T038 [E09] [FR-011] Camera attachments and QR with private paths, consent, metadata policy, hash and bounded queue.
- [ ] T039 [E09] [FR-011] GNSS external receiver protocol adapters, NMEA parsing, RTK status, NTRIP corrections and real receiver tests.
- [ ] T040 [E08] [FR-012,013,025] Offline data lifecycle, quotas, secure managed cache hooks, local export and sign-out separation.
- [ ] T041 [E16] [FR-026] Gate A1: real Android airplane-mode import/edit/survey/export/reopen workflow, independent format/CRS comparison.

## W4 — Connected API, identity and sync
- [ ] T042 [E10] [FR-028] Generate Dart client from current OpenAPI, add typed facade and compatibility tests with existing TypeScript/Server contract.
- [ ] T043 [E10] [FR-015] Implement mobile public client PKCE, system browser, redirect verification and secure key storage; additive server support only as required.
- [ ] T044 [E10] [FR-002,025] Implement trusted server identity, org/project/layer discovery and tenant-scoped session teardown.
- [ ] T045 [E10] [FR-014] Download authorized packages, verify signatures/hash, scope/encryption/expiry, resume and storage limits.
- [ ] T046 [E10] [FR-014] Incremental cursor/revision/ACL epoch and tombstone reconciliation; bounded pagination and full bootstrap fallback.
- [ ] T047 [E10] [FR-014] Stable operation IDs and payload hashes, retry/receipt lookup, conflict UI and audited resolution; no silent overwrite.
- [ ] T048 [E10] [FR-014,025] Online revocation, offline lease expiry/clock rollback, scoped cache/outbox lock/purge and no local file deletion.
- [ ] T049 [E10] [FR-024] Integrate existing realtime events, backpressure, reconnect and project switching; no duplicate subscriptions.
- [ ] T050 [E16] [FR-026,025] Gate A2: isolated real PostGIS cross-tenant, concurrent Web/mobile edits, 100 crash/retry cases, no duplicate/lost acknowledged writes.

## W5 — Plugin host and reference workflows
- [ ] T051 [E11] [FR-016,028] Version additive mobile manifest schema with existing registry; capability discovery, SDK compatibility and signature/assets checks.
- [ ] T052 [E11] [FR-017] Implement bounded declarative UI/form/dashboard/map-tool host; action broker with strict schema and capability checks.
- [ ] T053 [E11] [FR-016,017,025] Implement activation/project scope, permissions, device consent and lifecycle cleanup; deny unauthorized API/device/native access.
- [ ] T054 [E11] [FR-016] Discover installed/eligible plugins using existing marketplace, mark unsupported versions and handle offline expiry.
- [ ] T055 [E11] [FR-028] Publish SDK docs/sample/plugin generator or validated template and contract test harness; no separate duplicate marketplace.
- [ ] T056 [E12] [FR-018] Implement mobile house-numbering workspace, survey/photos/proposals using existing routes and data IDs.
- [ ] T057 [E12] [FR-018] Reference review/approval, offline allowed workflow, conflict/permission checks and Web visibility.
- [ ] T058 [E12] [FR-018] Second generic sample plugin validates no domain dependency in Core, installation/disable and data retention.
- [ ] T059 [E16] [FR-025,026] Gate A3: malicious manifests, cross-tenant attempts, signature tamper, lifecycle and two reference plugin workflows.

## W6 — Analysis, advanced engines and commercial integration
- [ ] T060 [E13] [FR-020] Port local buffer/clip/dissolve adapters, match algorithm metadata and explicit projection/geometry scope.
- [ ] T061 [E13] [FR-020] Port difference/intersection/union/reproject/spatial join within qualified geometry/type limits; differential oracle.
- [ ] T062 [E13] [FR-020,021] Jobs progress/cancel, resource budget, output provenance, choose local vs server by capability/data size.
- [ ] T063 [E13] [FR-022] Benchmark Arrow/GeoArrow, GeoParquet/FlatGeobuf and optional alternative Rust drivers; adopt only if justified.
- [ ] T064 [E13] [FR-022] Evaluate DataFusion/SedonaDB, optional wgpu compute, COPC/terrain/3D streaming behind feature gates; record unsupported profiles.
- [ ] T065 [E14] [FR-019] Map server-authoritative license/entitlement to existing project activation; implement additive versioned contract only after review.
- [ ] T066 [E14] [FR-019,025] Signed offline entitlement if approved; expiry/revocation/clock tests; preserve authorized Core export, no payment bypass.
- [ ] T067 [E14] [FR-019] Validate app distribution/AGPL/third-party license policy; commercial checkout/payout remains separate approved spec.
- [ ] T068 [E15] [FR-023] Reuse search/reverse/routing provider APIs, local offline search and route export; attribution/privacy.
- [ ] T069 [E15] [FR-024] Reuse chat/tasks/notifications; scoped subscriptions, offline and revoke behavior; no backend duplication.
- [ ] T070 [E16] [FR-030] Compare advanced feature benefit and footprint; gated experiments do not masquerade as production support.

## W7 — Full convergence and release
- [ ] T071 [E16] [FR-030] Benchmark physical Android reference profiles cold start, frame p95/p99, snap latency, RSS, thermal, battery and large dataset; report raw evidence.
- [ ] T072 [E16] [FR-025] Security review hostile files/manifest, FFI ownership/panic, auth, TLS, encrypted managed cache and scope; remediate findings.
- [ ] T073 [E16] [FR-026] Test OS lifecycle, low-memory/low-storage, background permission and upgrade/rollback without data loss.
- [ ] T074 [E16] [FR-026,029] Run full integrated Flutter/native/Rust/Web/PostGIS and QGIS parity regression on exact candidate; fix failures.
- [ ] T075 [E16] [FR-026,028] Complete vi/en docs, SDK quickstart, format/CRS/capability matrix, source offer, license/SBOM and known limits.
- [ ] T076 [E16] [FR-026] Build reproducible signed Android ARM64 release, verify package identity and distribution compliance; controlled device rollout.
- [ ] T077 [E16] [FR-026] iOS native build on macOS, physical device, permission/background/store policy; separately qualify signed iOS artifact.
- [ ] T078 [E16] [FR-026,029,030] Map every FR/SC to evidence, independent review, reconcile branch/main, final release gate A4. No live server deployment by default.

## Evidence and task close rules
Each implementation evidence record includes target platform/profile, exact SHA, command, input/fixture digest, expected/observed result, negative cases, device details and review. Map SC-001–010 to gates A0–A4 and separate iOS I1 using [acceptance-matrix.md](acceptance-matrix.md). T039 RTK, advanced features and T077 iOS have separate qualification; no implied Android dependency on T077. Parent close requires all required leaves, exact evidence and reviewed disposition of optional leaves. Optional research may close with reviewed rejection; required supported capabilities may not be waived silently. No merge/release claim without actual gate results.


## W0 execution status (2026-11-28)
See [execution ledger](evidence/execution-ledger.md) and [W0 baseline evidence](evidence/w0-baseline-checkpoint.md). Candidate graph and matrix audit passed on db2ef12; T001 remains in progress pending complete inventory/review. No acceptance checkbox changed.


### W0 continued evidence
See [execution ledger](evidence/execution-ledger.md), [auth/sync/plugin inventory](evidence/w0-auth-sync-plugin-inventory.md), and [threat model](evidence/w0-threat-model.md). Candidate qualification workflow and baseline auth/signature tests implemented; no mandatory parent accepted. Device/FFI proof, independent review and A0 remain open.


### W1 implementation status
See [W1 implementation checkpoint](evidence/w1-implementation-checkpoint.md) and [execution ledger](evidence/execution-ledger.md). Portable engine and local repository initial native tests passed; Flutter/FRB, shared CRS/geometry regression, device and recovery evidence outstanding. No parent or leaf accepted by this update; A0 open.


### W2 implementation evidence
See [W2 checkpoint](evidence/w2-implementation-checkpoint.md) and [execution ledger](evidence/execution-ledger.md). Host native candidate 25 tests passed at 29380f4; full format, device rendering and earlier gate acceptance remain open.


### W3 implementation evidence
See [W3 checkpoint](evidence/w3-implementation-checkpoint.md). Host focused 29 tests PASS at b370310. New grouped edits/media and Flutter test pending. No formal task closure.


### W4 implementation evidence
See [W4 checkpoint](evidence/w4-implementation-checkpoint.md). Candidate scoped Dart SDK passes targeted tests. Separate encrypted managed repository and PostgreSQL identity candidate require further qualification; no formal task closure.
