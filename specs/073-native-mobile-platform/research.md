# Research and architecture decisions — proposed, not measured
Baseline repository inspection: Cargo workspace already has ovg-core, ovg-spatial, ovg-processing, ovg-sync, ovg-storage, ovg-module-sdk, api-rust, house_numbering. ovg-spatial has geo-types/serde/core; processing/sync use Tokio and other crates; inspect transitive graph and functions before extracting. Existing partner JavaScript SDK is limited sandboxed iframe RPC, not unrestricted CRUD; Rust SDK is trusted official modules. Reuse SPEC 071 plugin workspace and SPEC 072 design tokens/semantics, not CSS on Flutter. E14 evidence has 27 Rust routes but integrated production parity is not asserted.

## Decision register
D01 Flutter native UI, MapLibre Native map; reject full WebView clone. Native map performance/texture integration spike on low-tier Android and iOS.
D02 Portable interfaces in Rust, FFI batched messages/handles, not one Rust server binary on phone. Test ABI, lifecycle, cancellation, panic containment, memory ownership, nonblocking UI and actual copy counts.
D03 GDAL/PROJ/GEOS baseline adapters, selected drivers and pinned assets. Compile NDK ARM64 and iOS; licenses/SBOM; compare pure Rust alternatives only if correctness, size and support win.
D04 GeoPackage/SQLite local canonical features and RTree; app metadata separate; one writer per local DB, transactions, WAL/checkpoint and export. No duplicate authoritative geometry.
D05 Rust local jobs and PostGIS server jobs share public algorithm metadata/semantics; differential tolerance and supported geometry types explicit.
D06 Mobile auth OIDC/PKCE public client; existing service credentials remain server-to-server. Verify actual API supports user mobile session exchange/refresh/revoke; additive server contract if needed; no weakening cookie/CSRF flows.
D07 Declarative signed plugins first; no runtime Dart download. Optional WASM only after platform policy/legal and sandbox evaluation; no native code loaded from marketplace.
D08 Arrow/GeoArrow batched data, GeoParquet, FlatGeobuf, DataFusion/SedonaDB, wgpu, RustGIS/OxiGeo/Navara are research candidates only, not mandatory dependencies. Compare baseline correctness, build reproducibility, APK delta, RAM, battery and latency; upstream release/commit/license and mobile ABI verify before adoption. No assumption these projects support production Flutter.
D09 GNSS external receiver BLE/USB/NTRIP and correction source qualification, no survey-grade claims. Explicit OS background permissions, recording indicator and lifecycle.
D10 AGPL workspace baseline; review plugin boundary, combined works, source offer, static/dynamic linking and dependencies, App Store/Play distribution. No relicense without contributor rights. Commercial payments and payouts follow separate approved policy/spec.

## Required experiments
- R01 Dependency graph: cargo metadata/tree per platform; prove no SQLx/Postgres/Axum in mobile library; inventory reusable pure geometry, sync contracts and processing semantics.
- R02 Native build: Android arm64 debug/release smoke with GDAL/GEOS/PROJ/GPKG; iOS simulator + real device spike, record binary size and ABI versions; no fabricated passing build.
- R03 Import fixtures: point/line/polygon/multi, holes, Z/M support or explicit reject, Unicode, nulls, invalid CRS, TAB companion files, archive bombs, corrupted data; independent GDAL/QGIS oracle.
- R04 Benchmark 10k/100k/1M features and raster blocks; compare Arrow/FFI copy paths and GPU optional against baseline with 20+ warm runs, memory/battery and rendering conditions recorded.
- R05 OIDC public client and managed offline lease; real API isolated PostGIS; confirm server version, auth capability and migration safety.
- R06 Plugin schema compatibility, signed assets, lifecycle and malicious inputs; store policy before introducing interpreted runtime or monetization.
- R07 Real Android GNSS/RTK hardware, permission/background kill/restart and camera attachment tests; iOS separately.
- R08 Release license/legal review and independent security review; every candidate dependency gets provenance and justification.

No research spike marked complete in this specification.
