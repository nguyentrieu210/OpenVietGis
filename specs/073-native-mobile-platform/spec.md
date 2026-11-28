# SPEC 073 — OpenVietGIS Native Mobile Platform
**Feature Branch**: `073-native-mobile-platform` | **Created**: 2026-11-28 | **Status**: Draft / implementation not started
**Baseline**: a4ae85a5e3d956de1870dcf65ff3fb21da7f0367
**Parent**: R002–R014, R015 for release; extends SPEC 013, 014, 064, 065, 066, 069, 070 (search and layer workspace), 071, 072. No replacement of existing Web/Server architecture.
**Input**: One Flutter GIS app with vTools-style independent field usability, professional offline GIS, existing OpenVietGIS SDK/Marketplace integration and reusable Rust kernel. Android ARM64 first, iOS second; desktop/WASM reuse is a boundary goal, not a desktop release commitment.

## Product modes
- Standalone: open map immediately; no account or organization required for local files, survey, editing, analysis and export. No local web SPA revival; native mobile profile only. Personal local workspace is distinct from server tenant data.
- Connected: select trusted OpenVietGIS server, authenticate, select authorized org/workspace/project, download layers and sync edits with revisions.
- Business: activate an existing authorized plugin in a project; open its workspace on demand; no permanent domain toolbar; close returns to Core. Single app, shared license/activation semantics, optional enterprise branding later.

## User stories (independently testable)
### US1 — Explore and survey (P0)
Open map, locate user, search place/coordinate, inspect scale/CRS/accuracy; choose map source; measure; record waypoint/track and navigate to saved point. Offline map and local workspace work in airplane mode.
### US2 — Open local data (P0)
Receive zipped SHP or TAB dataset via OS share/file picker; resolve companion files, choose explicit CRS if missing, render and inspect features, edit and export portable copy; no upload unless explicitly requested. Handle corrupt inputs safely.
### US3 — Professional field editing (P0)
Draw Point/Line/Polygon and supported multi types, capture by GPS, edit vertices, snapping, undo/redo, split/merge with explicit capabilities, attribute schema and validation; commit atomically; resume after crash.
### US4 — Vietnam surveying (P0)
Display WGS84 and configured VN-2000, convert point/batch and verify independent control points. Record source, axis, units, region/pipeline and accuracy. Unapproved datum shifts fail closed. Phone GNSS is not survey-grade; RTK support records fix status and quality.
### US5 — Offline and synchronization (P0)
Download scoped package, edit offline, preserve outbox through restart, reconnect, resolve conflicts, see receipts and synchronize Web/other clients; revoke access online and lock expired offline data. No cross-project leakage.
### US6 — Plugin workspace (P0)
Discover installed/eligible plugins from existing registry, use signed declarative mobile contributions, activate authorized workspace; use house-numbering reference plugin for survey, photo, proposal and review; uninstall/disable without losing authorized Core data.
### US7 — Professional analysis (P1)
Run supported buffer/clip/dissolve/difference/intersection/union/reproject/spatial join locally on bounded data or submit server jobs, with provenance, progress/cancel, explicit CRS and matching semantics where advertised.
### US8 — Advanced survey and data (P1)
RTK/NTRIP/NMEA via supported BLE/USB/network adapters, image attachments, QR, background tracks, COG/terrain and streaming point-cloud/3D via gated engine profiles.
### US9 — Developer and commercial ecosystem (P1)
Partner builds a declarative plugin using SDK samples, submits existing signed catalog workflow, installs on Web/Mobile, tests permissions and compatibility. Commercial activation is server-authoritative and independently audited; payment/revenue settlement is separate follow-on, not a claim of readiness.
### US10 — Operations (P0)
Install signed Android artifact, migrate local schema, operate low-memory/low-battery, report diagnostics without sensitive data; verify signed release and recovery. iOS has its own native build and real-device release gate.

## Functional requirements
- FR-001 One native Flutter shell, map-first 360px+ touch UX, bottom sheets, compact toolbar, vi/en, accessible controls; no web app embedded as main map.
- FR-002 Three modes and clear local vs server workspace boundaries; no account for local workflows; explicit connect/upload/publish; no legacy web guest/local SPA resurrection.
- FR-003 Shared portable Rust interfaces reuse existing ovg-core/spatial/sync/processing contracts; no Axum, SQLx/Postgres, tenant secrets or server daemon in native kernel; retain server adapter behavior.
- FR-004 MapLibre Native rendering, camera, gestures, north/scale, layer order, feature hit-test, selection, style/label and editing overlays; capability-gate unsupported style elements; GPU rendering not duplicated.
- FR-005 Local import/export with explicit format/driver capability matrix: SHP, TAB/MIF/MID, GPKG, KML/KMZ, GeoJSON, GPX, CSV/XLSX coordinates, DXF subset; MBTiles read, PMTiles read, GeoTIFF/COG; DGN and additional formats only after driver/license/fixture qualification. Read/write flags separate; preserve fields, CRS and supported types; report loss before export.
- FR-006 File picker/share-in/share-out, zip/path traversal/decompression bounds, private staging, cancellation, no arbitrary file paths or implicit network fetch, safe filenames.
- FR-007 CRS source/storage/display separation, PROJ pinned database/grid provenance, explicit VN-2000 region/parameters, independent control points, no silent fallback. Coordinate order/units and geodesic vs projected measurement explicit.
- FR-008 Geometry edit lifecycle with stable IDs, snapping by screen tolerance with CRS-aware search, validity, revision, undo/redo, transaction recovery; preview never mutates committed data. No destructive fix silently.
- FR-009 Attribute editor, typed schema/default/required/constraints, selection/filter, virtualized table and schema-driven forms; server validates synced mutations.
- FR-010 GPS accuracy, timestamp, provider, waypoint, track segments, distance/area/bearing, offline point navigation, optional elevation; background tracking explicit user session/permission and visible OS notification where required.
- FR-011 Camera/photo attachment, QR and GNSS external adapter capability/permissions; no default location disclosure, strip/export metadata only on explicit choice, bounded upload queue, RTK fix state and corrections provenance.
- FR-012 SQLite/GeoPackage transactional local repository, RTree, schema migration, outbox durability, atomic receipt application, storage quotas and corruption recovery; separate app metadata and interoperable export.
- FR-013 Offline maps from licensed sources or user-provided archives; download estimate/cancel/resume/eviction, attribution, tile integrity; no bulk downloading public OSM endpoints against policy.
- FR-014 Versioned incremental sync using existing APIs, scoped encrypted managed packages, cursor/revision/ACL epoch, stable operation IDs, idempotency, conflict UI, retry/backoff, tombstones, receipt reconciliation; no silent last-write-wins.
- FR-015 OIDC public client authorization-code + PKCE using external browser, verified redirect, secure keystore, refresh/revoke; reuse server identity, no service token in app, no arbitrary trusted origins or insecure TLS.
- FR-016 Existing Plugin Registry/manifest/activation/SDK canonical; additive mobile contribution schema and capability discovery; unsupported plugins explicitly marked, signed manifest/assets verified, project-scoped permissions, lifecycle cleanup.
- FR-017 Native declarative UI/form/tool/command host with bounded nodes/actions; no arbitrary Dart download, no eval, no unrestricted native FFI or WebView bridge. WASM experiment optional, no production execution unless store policy, sandbox and quota approval. Separate native compiled plugins.
- FR-018 House-numbering reference end-to-end using same backend/API/IDs/rules; no domain imports in Core; approved-only data/publish rules; plugin close and disable preserve Core data.
- FR-019 License/entitlement bound to org/plugin/version/permissions on server; signed offline lease with expiry and clock-rollback handling if offered. Core and data export stay available subject to data permissions. Payment integration/store review, publisher payout/tax/refund separate spec.
- FR-020 Local processing via approved adapters, same algorithm IDs/parameter semantics as Server; compare topology/CRS/tolerance and attributes; cancellation and bounded memory; unsupported algorithms server-only with explicit UI.
- FR-021 Large data: progressive BBOX/LOD/tile/feature paging, bounded cache, off-main-thread IO/compute, FFI batches/ownership and cancellation; never transfer entire massive dataset to Dart.
- FR-022 Advanced raster/terrain/3D/COPC and Arrow/GeoArrow/GeoParquet/DataFusion/wgpu behind capability flags; enable only measured mobile benefit, no second default renderer; unsupported mesh/DGN/format claims prohibited.
- FR-023 Search and optional navigation reuse SPEC 070 provider contract, local search offline; provider keys server-side; disclose remote coordinate query and attribution.
- FR-024 Chat/tasks/notifications use existing scoped APIs; permission revocation and event/cursor lifecycle; no separate collaboration backend.
- FR-025 Security: least privilege, tenant/data/cache separation, encrypted managed cache, offline lease, local file isolation, no secrets in diagnostics, plugin threat model, SBOM/license review; logout/revoke policy protects managed data without deleting independent local files.
- FR-026 Test matrix includes real Android low/mid-tier and iOS devices, reference datasets, negative cases, offline/kill/restart/upgrade; reproducible signed artifacts, no production rollout until gate acceptance.
- FR-027 Accessibility, localization, high contrast, permission-denial flows and touch targets; respect OS lifecycle, storage and low-memory signals.
- FR-028 Developer docs, generated Dart API bindings from current contracts, SDK samples/tests, manifest version negotiation, backwards compatibility and documented limits; reuse existing packages, no duplicate public SDK.
- FR-029 Native engine adapter feature inventory and parity map mandatory; keep current Web/Server regression gates; no unapproved replacement or feature retirement.
- FR-030 Diagnostics/performance telemetry opt-in, local-first, redacted; record p50/p95/p99, RSS, battery, thermal, data/format/CRS correctness, exact toolchain/device/commit; publish limitations.

## Success criteria (targets, not claims)
- SC-001 Android release build launches standalone, local workflow works in airplane mode including import/edit/export/reopen; independent QGIS/GDAL round-trip validates IDs/fields/geometry/CRS in declared supported profile.
- SC-002 Cold start p95 <=2s to usable map shell (cached map not network dependent) on documented mid-tier ARM64 device; benchmark 30 cold launches. 60Hz pan target p95 frame <=16.7ms at defined visible-load profile; log jank and p99, no thermal throttling hidden.
- SC-003 1M source feature indexed dataset opens by streaming, 10k/100k/1M profiles; 10M stretch target only; p95 snap <=50ms for declared visible dataset; peak RSS target <=512 MiB on 4GB reference device; no OOM on 30-minute pan/edit/track soak.
- SC-004 Read >=10GB local/remote tiled raster fixture without full loading where driver supports range/block access; 100M-point source COPC LOD is gated stretch, not initial acceptance.
- SC-005 CRS control point errors within independently specified survey tolerances for every enabled pipeline; native vs server comparisons with declared units/tolerances; all unsupported regional conversions reject. RTK validation on real hardware with fix metadata, no accuracy guarantees from phone GPS.
- SC-006 100 kill/restart/sync/retry cases no lost acknowledged edit, duplicated mutation or unauthorized replay; conflict and revoked scope tested against isolated real PostGIS.
- SC-007 2 organizations/2 users/2 projects + offline cache, plugin disable, expired lease, logout, malicious manifest/file and permission revocation tests: zero unauthorized reads/writes.
- SC-008 Same house-numbering workflow and IDs on Web/Mobile, activation rules and scoped API; reference second generic plugin has zero Core domain dependency. Developer quickstart can build first supported plugin <=30min on clean documented environment.
- SC-009 Signed Android ARM64 artifact with pinned dependencies/SBOM/license, upgrade/recovery/rollback evidence and source offer compliance; iOS separate real-device and App Store policy gate. Battery target <=10 percentage points/hour for documented screen-on tracking test on reference mid-tier device; report brightness, radios, sensor rate and thermal.
- SC-010 All mandatory tasks mapped to executable evidence on exact merged candidate, independent review resolves critical/high defects, Web/API regression passes; no unsupported advertised capability.

## Non-goals
Full QGIS/AutoCAD parity in first release, duplicate server, running PostGIS on phone, forcing all web plugins native, unrestricted downloaded code, payment settlement before commercial spec, custom GPU renderer without evidence, guaranteed survey/legal cadastral accuracy, desktop release.
