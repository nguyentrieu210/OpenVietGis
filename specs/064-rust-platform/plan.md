# Implementation Plan: Rust-native Platform
**Branch**: 064-rust-platform | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)
## Summary
Build a separate Rust modular monolith from verified reference baseline 3e78ac1 (includes origin/main 1aa8dcb and 13 local commits at inspection). Existing Python remains behavioral oracle. No live change.

## Technical Context
Rust stable pinned after toolchain/dependency/license checks; Axum/Tokio/Tower/Hyper, SQLx, Serde, tracing; bounded Rayon workers. PostgreSQL/PostGIS remain canonical. GEOS/GDAL/PROJ native adapters server-side, GeoRust for portable operations with correctness tests. Martin authorized tile service; React/TypeScript, MapLibre, Cesium, existing CAD and PyQGIS adapters retained. WASM optional client kernels. GeoArrow/GeoParquet, SedonaDB/DataFusion, Wasmtime, wgpu optional gated extensions. Linux server, isolated 4 GiB test profile; no heavyweight build on live VPS with scarce disk/swap. Cargo test/fmt/clippy, existing pytest, Playwright, real PostGIS and differential benchmarks.

## Constitution Check
I platform/module isolation via SDK; II open formats and documented API; III tenant scope and RLS/cache checks; IV explicit CRS and golden control points; V transaction/revision/outbox; VI layer capabilities and common commands; VII replaceable engines; VIII workflow parity before features. ADR-015 is Proposed; ADR-004 stays Accepted until reviewed transition. Recheck each slice; no gate claimed passed in implementation.

## Project Structure (proposed)
- Cargo.toml workspace, rust-toolchain.toml
- apps/api-rust/ (HTTP, auth, application orchestration)
- crates/ovg-core/ (domain, CRS, geometry contracts)
- crates/ovg-storage/ (PostGIS, scoped transactions)
- crates/ovg-spatial/ (GEOS/GDAL/PROJ and portable kernels)
- crates/ovg-sync/ (outbox, cursors, realtime)
- crates/ovg-processing/ (bounded jobs)
- crates/ovg-module-sdk/ and modules/house-numbering/rust/
- crates/ovg-wasm/ (only portable tested kernels)
- apps/web-next/, integrations/qgis_plugin/, existing CAD adapters
- infra/rust-platform/ (isolated staging, tile ACL gateway)
- tests/rust-parity/, tools/benchmark/rust-platform/
## Execution
0 Inventory and benchmark reference, reconcile commits and identify live schema.
1 Foundation: workspace, typed errors, auth/RLS, DB pool, health, parity harness.
2 Vertical slice: project/layer/read/edit/export, revision history and CRS.
3 Tiles, bounded queries, cache, realtime replay and QGIS round trip.
4 Migrate remaining supported modules/workflows; test each independently.
5 Security, license, resource/soak, performance, backup and rollback gate.
6 Explicitly approved cutover; keep compatible rollback until observation and approval for retirement.
## Dependencies & decisions
See research.md, data-model.md, contracts/parity.md, tasks.md. Existing Spec 063 WKB work is isolated; reuse only after review and evidence. No new DB schema or public API invented before inventory.
