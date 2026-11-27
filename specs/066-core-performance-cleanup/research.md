# Source audit and decisions
Snapshot 5bfc83826161acd83b98a925e85690cebde53ae1, read with git ls-tree/git show; no live benchmark or full dependency graph yet.
| Finding | Evidence | Decision |
|---|---|---|
| Six app dirs | apps/api, api-rust, mcp, mcp-rust, web, web-next | Map runtime/test/adapter/retired role before removal |
| 458 Python API paths, 151 old web paths, 40 old MCP paths | git ls-tree -r (tracked file counts, not dead-code counts) | deletion only after parity |
| Legacy dev commands | root package.json bootstrap/dev:api/db:migrate/validate:py/validate:ts; pnpm-workspace.yaml includes apps/web | converge scripts and lockfile after transition |
| Rust workspace | Cargo.toml: api-rust, ovg-core/storage/spatial/sync/processing/module-sdk, house_numbering; apps/mcp-rust separate | assess MCP workspace join, keep explicit boundary if justified |
| Frontend eager entry | apps/web-next/src/main.tsx imports LiveApp, map-surface, Landing, MarketplacePage, QgisConnectPage, PublisherPortal; LiveApp imports multiple workbenches | route-aware lazy graph and measured startup |
| Existing cache | SpatialCache.ts/Storage/Worker, SpatialIncremental.ts, SpatialSyncCore.ts, protected-query.ts, offline SW registration | extend existing primitives; prove coherence, not duplicate framework |
| Cesium asset copy | apps/web-next/vite.config.ts recursively copies pinned 1.147.0 prebuilt distribution | measure build copy and runtime fetch; content-addressed/versioned cache |
| Existing web boundary | vite config rejects imports from retired apps/web | retain guard and complete 062 |
| 14 workflows | .github/workflows; rust-platform.yml includes Python differential parity | consolidate only after mapping required check names and evidence |
| ADR-015 proposed, ADR-004 accepted | docs/adr/ADR-015-rust-platform.md | 064 approval/parity is retirement gate, not assumption |
| VPS has multiple working directories | /opt/ovg-065-work, ovg-cad-dirty, ovg-e16-native, ovg-guest-entry, ovg-resume-check etc. | inventory owners/active tasks, no delete in this spec |
| Existing baseline references | specs/064-rust-platform/evidence; tests/rust-parity; tools/benchmark; tests/e17 | reuse, rerun on comparable candidate |
Constraints: ADR-001/002 accepted monorepo/modular monolith; constitution tenant isolation, CRS, audit, capabilities. Verify accepted ADRs and 065 merge head at implementation start. R015 release gate remains open unless independently proven.
