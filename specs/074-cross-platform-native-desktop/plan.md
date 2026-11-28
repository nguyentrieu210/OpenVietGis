# Implementation plan — SPEC 074

## Baseline
Main workspace already includes ovg-core, ovg-spatial, ovg-processing, ovg-sync, ovg-storage and API. Storage is PostGIS-specific; professional geometry and transformations include SQL paths. PR #103 plans crates/ovg-mobile-engine, crates/ovg-local-storage and crates/ovg-mobile-ffi. Its W1 checkpoint reports initial portable engine and local WGS84 XY SQLite/RTree/journal tests; managed protection, migrations and device qualification remain open. Inventory actual paths at the integration SHA before coding; planned names are not proof of files.

## Boundaries
- apps/desktop: Tauri host, OS integration, desktop window/lifecycle, platform packaging.
- apps/web-next and packages/client-sdk: one React source tree, additive transport/capability adapter (HTTP cloud vs typed IPC local).
- Reuse SPEC 073 local engine/storage; only rename/extract a platform-neutral package through an agreed atomic migration after dependency inventory. Keep mobile FFI and desktop IPC thin and separate.
- Existing ovg-storage remains PostGIS adapter. Add narrowly scoped portable traits only for actual shared use cases; no blanket rewrite or SQL translation layer.
- Reuse current sync and plugin contracts. Desktop-specific capability adapter; native runtime never bypasses authorization.
- First renderer: current Web MapLibre/Cesium where qualified on OS WebView. Native GPU wgpu is a future optional provider, not first-release dependency.

## Work waves
W0: inventory current PR #103 head, main, native crates and API parity; agree ownership and freeze cross-platform contracts; Windows/macOS/Linux toolchain and WebView spikes.
W1: Tauri shell, shared UI build, typed transport, local session lifecycle; cloud connection unchanged.
W2: Standalone storage/files/editing, capability flags, standard-format interop, recovery.
W3: Local compute/CRS, rendering and desktop UX, import/export qualification.
W4: Hybrid sync, identity, managed store and security parity with Mobile; isolated server integration.
W5: Desktop plugin adapter, packaging, accessibility, signed update and platform acceptance.
W6: Optional advanced analytics/native rendering only on evidence.

W0/W1 shell work can run parallel to 073; shared code changes land through one integration owner after 073 baseline frozen. Dependent local feature acceptance blocked until corresponding 073 contract and qualification pass. Do not create overlapping standalone 075–078 projects for already-owned work. Run targeted checks during development and integrated CI when complete. No live deployment.
