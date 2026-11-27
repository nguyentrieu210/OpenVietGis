# Core–Plugin architecture decoupling

Status: planned / implementation in progress. Baseline main 5bfc8382; audit 2026-11-27.

## Scope
Core GIS persists independently of the official House Numbering module. Preserve public API compatibility and all existing data. Coordinate integration with 066; no production deployment until regression gates pass.

## Requirements and acceptance

### E01 — Inventory and enforce dependency direction
Architecture tests reject module imports from app internals and business imports in Core except composition root.

### E02 — Persistent GIS shell
Switch, enable, disable and refresh permissions without remounting map; retain camera, selection, drafts, layers and undo; invalidate revoked scopes immediately.

### E03 — Generic transactional edit policies
Replace business triggers with tenant-scoped Core lock and validation provider; cover insert/update/delete, import, QGIS, offline and concurrency; migrate existing signed locks atomically.

### E04 — Rust plugin runtime
Versioned async command contracts, registered routes and scoped services; every advertised command has execution and authorization tests.

### E05 — Public frontend SDK
Move StudioHost, dialogs, contribution types and stable UI primitives into versioned packages; remove reverse imports.

### E06 — Module-owned backend and persistence
Move business handlers and SQL out of Core crates, preserve API schemas and transaction semantics.

### E07 — Authorization
Server-enforced per-command grants, tenant scope, private data access and export; test deny by default and revocation.

### E08 — Stable layer bindings
Bind semantic roles to stable source/feature IDs with revisions; rename, delete and relink tests.

### E09 — Event and sync contract
Typed change events and cache invalidation without reload; durable idempotency and reconnect replay.

### E10 — Multiple contributions
Multiple active plugins with scoped overlays, deterministic priority, isolated errors and cleanup.

### E11 — Lifecycle
Install/enable/disable/upgrade/rollback contracts; retain data; fail closed on incompatible manifests.

### E12 — Data migration
Backup and restore rehearsal; verify counts, hashes, links, signoffs and audit history; no destructive migration.

### E13 — Regression gates
Core without HN build, browser switch invariants, QGIS sync, RLS, 30k-point p95 no regression relative to 066.

### E14 — Developer SDK
Document contracts and provide independent sample plugin with compatibility tests.

## Release gate
All requirements require executable evidence or explicitly documented external acceptance. No completion based on checkboxes alone.
