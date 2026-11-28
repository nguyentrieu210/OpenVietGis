# Implementation Plan — SPEC 073
**Branch**: 073-native-mobile-platform | **Date**: 2026-11-28 | **Baseline**: a4ae85a5e3d956de1870dcf65ff3fb21da7f0367
[Spec](spec.md) · [Tasks](tasks.md) · [Research](research.md) · [Contracts](contracts/mobile-boundary.md)

## Architecture decision
One app, one existing API/registry, one portable Rust kernel. Native mobile uses adapters, not a fork of Core or second backend. Flutter shell and MapLibre Native render; Rust portable geometry/CRS/IO/local store behind typed flutter_rust_bridge (C ABI at dependency boundaries). Server remains Axum/PostGIS; reuse TypeScript/partner SDK and current Rust module SDK, add Dart API client and mobile host adapter. Keep existing web plugin runtime and security. Share behavior/contracts, not necessarily identical implementation for PostGIS-dependent operations.

## Technical stack (candidate until spikes pass)
Flutter stable/Dart, Impeller, Riverpod, GoRouter; MapLibre Native Android/iOS; Rust stable aligned with current 1.85.1 or documented workspace upgrade, flutter_rust_bridge 2.x; GeoRust + GEOS, PROJ + pinned grids/db, GDAL/OGR driver allowlist; SQLite/GeoPackage/RTree, Drift for app metadata or one Rust-owned DB access boundary; MBTiles/PMTiles; Dio, generated OpenAPI clients, system browser OIDC/PKCE/secure storage. Android NDK/CMake and iOS XCFramework/toolchain pinned; deterministic lockfiles, ABI checks, CI builds. Optional Arrow/GeoArrow/GeoParquet, wgpu/WGSL, DataFusion/SedonaDB, COPC/3D, pure-Rust format adapters only after benchmark/license/device evidence. No new dependency just for brand recognition.
 
## Proposed layout (adjust after inventory; no duplicate package)
- apps/mobile-flutter/{lib,android,ios,test,integration_test}
- packages/dart-sdk/ (generated transport + hand-written façade)
- packages/flutter-plugin-sdk/ (host contract + schema-driven UI + sample)
- crates/ovg-mobile-engine/ (or existing portable crate extension; inventory first)
- crates/ovg-mobile-ffi/ (FFI boundary only)
- crates/ovg-local-storage/ (SQLite adapters, no server storage fork)
- crates/ovg-spatial, ovg-processing, ovg-core, ovg-sync (portable extraction via traits/feature flags only where needed)
- modules/house_numbering/mobile/ (domain-owned UI/contributions, shared backend)
- tests/mobile/, tools/benchmark/mobile/, infra/mobile/ (fixtures, build, release)
- Existing specs/contracts canonical; SPEC 073 contracts define additive mobile behavior, no parallel API namespace unless justified.

## Engine registry and ownership
| Engine | Scope | Dependencies |
|---|---|---|
| E01 | Baseline inventory, ABI, native build, dependency/license and performance fixtures | none |
| E02 | Portable Rust kernel, adapter traits, FFI lifecycle | E01 |
| E03 | Flutter shell, local/connected modes, accessibility, navigation | E01 |
| E04 | 2D render, map sources, layer/style, offline tile archives | E02/E03 |
| E05 | Native file IO, data validation, interoperability | E02 |
| E06 | CRS, Vietnam transformations, geometry measurement | E02/E05 |
| E07 | Editing, attributes, topology, commands | E04/E05/E06/E08 |
| E08 | SQLite/GeoPackage, crash recovery, offline storage | E02 |
| E09 | Survey/GNSS/RTK, camera, QR, tracking | E03/E06/E08 |
| E10 | Dart SDK, auth, project APIs, sync, conflicts | E02/E03/E08 |
| E11 | Plugin runtime, permissions, developer tooling | E03/E07/E10 |
| E12 | House numbering and generic reference module | E09/E11 |
| E13 | Processing, advanced data, adaptive performance | E02/E05/E06/E08 |
| E14 | Marketplace, commercial entitlement and optional enterprise profile | E10/E11/E12 |
| E15 | Search, navigation, chat/tasks reuse | E04/E09/E10 |
| E16 | Security, release, benchmark/acceptance and docs | all; security starts W1 |

## Waves
W0 inventory and spikes; W1 kernel + Flutter shell + DB; W2 map/IO/CRS; W3 edit/survey/offline; W4 connected auth/sync; W5 plugin + reference modules; W6 advanced processing and commercial/search/collaboration; W7 device qualification/release. Parallel agents use disjoint worktrees and file ownership; integration owner resolves shared manifests/ABI/DB contracts. Each wave uses targeted compile/tests; run full integrated CI once candidate assembled, fix failures, rerun affected/final gates. No blanket waiver of required checks. Never mark tasks from scaffolding alone.

## Constitution check
I Core independent of domain; II portable exports; III server RLS and managed cache; IV explicit CRS; V transactional changes/revisions; VI capability registry; VII engine adapters; VIII real workflows/benchmarks. No conflicts identified in proposed design; standalone mobile differs from forbidden historical web GuestStudio and must remain separate; no production modifications during spec authoring.

## Gates
A0 locked toolchain/license/ABI proof (Android first, iOS feasibility); A1 standalone offline workflow; A2 server sync/security parity; A3 reference plugin parity; A4 final full exact candidate/device tests, signed Android release and independent review; iOS separate promotion. Optional technologies never block core release unless mandatory capability depends on them; any reduced scope explicitly documented/approved. No changes to live Web/Server by this spec.
