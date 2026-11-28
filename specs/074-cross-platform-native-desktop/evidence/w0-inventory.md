# W0 inventory — source checkpoint

Pinned inspection: main a4ae85a5e3d956de1870dcf65ff3fb21da7f0367; SPEC 073 PR #103 head a746778f7b09ad691f4fe68e21a973174b46c51a. PR #103 remains open; re-pin at integration. This document is source inspection, not a qualified dependency graph.

| Component | Observed source and direct dependencies | Ownership / disposition |
|---|---|---|
| ovg-core | Domain, CRS and errors; serde, uuid, chrono | Existing shared core |
| ovg-spatial | WKB/GeoJSON and portable transform kernel | Existing shared core; strict XY change on 073 |
| ovg-processing | Rayon, Tokio and algorithm registry; metadata is not implementation parity | Existing shared core |
| ovg-sync | Outbox, replay and field validation | Existing shared core; managed sync still gated |
| ovg-storage | SQLx Postgres, PostGIS professional and CRS operations | Server only; do not link into desktop standalone |
| ovg-mobile-engine | Separate Cargo workspace; core/spatial/processing/sync, geographiclib-rs, geo, zip, flate2; Engine session/workspace/job handles, batch/viewport/CRS/edit/survey/attributes/PMTiles | 073 owns; reuse without duplicate |
| ovg-local-storage | Separate workspace; rusqlite bundled 0.32.1, spatial, mobile engine, CSV; standard GeoPackage XY EPSG:4326, RTree, mutation/revision/journal | 073 owns; plaintext independent local only; no managed fallback |
| ovg-mobile-ffi | FRB 2.11.1, engine and store; cdylib/staticlib/rlib | Mobile adapter only, not desktop dependency |
| apps/web-next | React, MapLibre; Cesium via spatial-workspace; Vite, Tailwind | Shared existing UI; no new web fork |
| packages/client-sdk | TypeScript API transport and contracts | Shared; additive desktop adapter after W0 |

The current 073 source does not establish arbitrary CRS local editing, encrypted managed persistence, full compute parity or desktop qualification. Root Cargo workspace excludes native subworkspaces. No source changes to 073 or production in this W0 candidate.

## Integration ownership
073: portable engine, local schema, transaction journal, sync semantics, mobile FFI. 074: desktop host, desktop IPC/data-plane adapter, renderer cache and OS qualification. Shared package/schema/ABI/lock modifications require one integration owner. Before connecting desktop, record exact merged or approved integration SHA, generate locked graph for each target; deny server-only crates (sqlx, sqlx-postgres, axum, openvietgis-api-rust, ovg-storage) and Flutter bridge in desktop graph. Audit all resolved dependencies, feature flags, build scripts and native libraries. Do not imply direct manifests prove transitive absence.

## Toolchain
Root Rust 1.85.1; existing Web package.json Node 26 / pnpm 12.21.0; W0 Tauri 2 spike deliberately resolves with stable Rust on isolated runner and archives lockfiles. This is a discovery spike, not final pin. A Tauri-specific MSRV increase needs an explicit decision; no root upgrade or server build change.

## Upstream review (no code copied)
- Sphere tmshv/sphere, c6867a9d18bc9673cb113f203b2b55f70edd7920: root LICENSE lookup returned 404, license not established; architecture only, no import.
- GeoLibre opengeos/GeoLibre, 7b8f7ce171b78199057c560a0aa6152ee33ac38e: root MIT license verified; shared UI pattern. Dependency notices and exact imported files still require review before any reuse.
- RustGIS opengeos/rustgis, aea84131974b57ff6359666dbe79f88054a64bf1: Apache-2.0; workspace 0.8.0, edition 2024, MSRV 1.92, separate geom/data/analysis/render/engine crates. Incompatible with current 1.85.1 MSRV as-is; no direct dependency. Optional provider spike after toolchain, transitive license/NOTICE, ABI and geometry parity review.

T001/T002/T033 source checkpoint only; full locked native graph, integration ownership acknowledgment, reviewed provider matrix and qualification remain open.
