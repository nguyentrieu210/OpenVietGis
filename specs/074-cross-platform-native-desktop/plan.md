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

## Revision 2 — hybrid native data plane
Control plane: typed commands for workspace lifecycle, editing, job/cancellation, auth and capability handshake. Data plane: logical ovg:// URLs for read-only binary tiles/resources and bounded typed pages; MapLibre addProtocol adapter, Cesium resource adapter, platform-specific Tauri handler or approved scoped fallback. No open loopback port by default. No whole-layer JSON IPC. Existing browser HTTP behavior stays unchanged.
Reuse native runtime and storage from SPEC 073; derive render data from canonical revisioned sources. Separate render cache from authoritative store; bounded queues, priority, cancellation and byte budgets at every layer. Source CRS remains explicit; display tile projection never changes canonical data.
W0 compares existing MapLibre/Cesium rendering, native providers and platform transport security. Research external repositories only against exact licensed commits and a dependency/capability matrix; no vendoring unlicensed code. Keep React as the only desktop/web UI; renderer replacement is not mandatory. Optional native provider dependencies must not enter server or mobile build graphs without explicit qualification.
## Work waves
W0: inventory current PR #103 head, main, native crates and API parity; agree ownership and freeze cross-platform contracts; Windows/macOS/Linux toolchain and WebView spikes.
W1: Tauri shell, shared UI build, typed transport, local session lifecycle; cloud connection unchanged.
W2: Universal Local Data Engine (standalone). First integrate project state and shared schema controls in existing host. In parallel develop a portable single-domain transaction candidate behind tests, not an active second store. Align schema owner with 073, then qualify migration and switch all readers/editors/backup to the same authoritative store atomically. Add capability-driven providers, bounded queries and maintenance; additional native drivers are opt-in after qualification.
W3: Local compute/CRS, rendering and desktop UX, import/export qualification.
W4: Hybrid sync, identity, managed store and security parity with Mobile; isolated server integration.
W5: Desktop plugin adapter, packaging, accessibility, signed update and platform acceptance.
W6: Optional advanced analytics/native rendering only on evidence.

W0/W1 shell work can run parallel to 073; shared code changes land through one integration owner after 073 baseline frozen. Dependent local feature acceptance blocked until corresponding 073 contract and qualification pass. Do not create overlapping standalone 075–078 projects for already-owned work. Run targeted checks during development and integrated CI when complete. No live deployment.
