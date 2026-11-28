# SPEC 074 — OpenVietGIS Cross-Platform Native Desktop

Status: Draft; implementation and acceptance open.
Targets: Windows x64 first, macOS Apple Silicon and Linux x64 separately qualified. Windows ARM64, Intel Mac and Linux ARM64 require separate qualification.
Dependency: SPEC 073 / PR #103, specifically E02 portable runtime, E08 local repository, E10 sync, E11 plugin contracts. Do not duplicate them. At review snapshot PR #103 was open and W1 partial; no assumed merge or acceptance.

## Product
A desktop GIS application with standalone, connected and hybrid modes. Reuse the React workspace in Tauri 2 for first release; embed portable Rust services locally. Preserve existing Cloud behavior. Desktop release is not a claim of a native GPU renderer; a renderer replacement needs a separate measured decision.

## Requirements
- D01: Packaged desktop app launches standalone without account, network, Docker or PostgreSQL.
- D02: Connected mode uses existing authorized Cloud APIs, client SDK and server identity policies. No embedded service credentials, no direct cloud database access.
- D03: Desktop bridge exposes typed, allowlisted commands, bounded payloads, paged data, cancellation and opaque handles. No arbitrary SQL, shell, filesystem or unrestricted FFI.
- D04: Reuse SPEC 073 portable engine, local storage schema/transactions, CRS/geometry contracts and sync protocol. A single owner for shared engine, schema and lockfiles.
- D05: Local standard GeoPackage for standalone data; managed synchronized workspace uses the same qualified private encrypted store as Mobile, with lease and scope enforcement. SQLite metadata is not a second authoritative feature store.
- D06: Open/create/save local projects, layer display, attribute table, selection, editing, undo/redo and stable revision history. File picker grants narrowly scoped access. Unsupported formats or operations fail explicitly.
- D07: Import/export qualified vector, raster, CAD and point-cloud formats per capability matrix; no parity claim without format fixtures and roundtrips.
- D08: Local GIS processing uses portable compute providers with explicit CRS provenance and tested topology parity. Server-only algorithms clearly marked; never silently upload local data.
- D09: Hybrid sync uses existing authenticated protocol with durable outbox, stable operation IDs, revisions, tombstones, conflict resolution, receipts and ACL/lease checks; no silent last-write-wins.
- D10: Desktop workspace includes menu, dock/panels, shortcuts, multi-monitor behavior, drag/drop, print/export and accessibility. Reuse Web UI where feasible without introducing a second fork.
- D11: Plugin manifest and capability model reuse existing registry; Desktop adapter only. Untrusted downloaded native code disabled; optional WASM host requires sandbox/security/license qualification.
- D12: Native file access, secure OS key storage, system-browser auth with approved redirect/PKCE, per-window origin isolation, CSP, signed update verification and bounded resource usage.
- D13: Platform-specific packaging, signing, notarization, SBOM, upgrade and migration recovery; release only for qualified OS/architecture combinations.
- D14: Cloud schema/API compatibility preserved; no production deployment/migration in this SPEC without separately scoped release decision.
- D15: DuckDB Spatial/GeoParquet, native GPU renderer and advanced drivers optional, after benchmarks and dependency licensing review. Not a second editable source of truth.

## Acceptance
A1: Clean Windows install, no network/account/server; open, edit, save, restart and export real fixture.
A2: Same workspace opened on qualified Mobile/Desktop has equivalent canonical geometry, CRS and revisions; recovery tests prove no loss of acknowledged edit.
A3: Isolated real PostGIS server and two clients: reconnect, duplicates, conflicts, revocation, expired lease and deletion all pass; existing API regression unchanged.
A4: Real device/OS fixture roundtrips, keyboard/accessibility, hostile-file and IPC security checks; unsupported capability clearly reported.
A5: Signed reproducible release artifacts, OS-specific qualification, rollback tested; no unsupported platform advertised.
A6: Shared dependency graph excludes server-only crates from local engine; exact SHA, logs and fixtures attached.

Non-goals: replacing PostGIS, rewriting Web UI, duplicated mobile storage/sync, arbitrary native plugins, unrestricted filesystem, guaranteed full QGIS/AutoCAD parity, mandatory custom GPU engine in first release.
