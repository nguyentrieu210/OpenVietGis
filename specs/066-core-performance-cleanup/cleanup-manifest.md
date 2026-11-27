# Initial cleanup candidates (not authorized deletions)
| Candidate | Required proof |
|---|---|
| apps/web and pnpm workspace entry | 062 all reachable workflows, offline and URL parity; one canonical build; no imports |
| apps/api and Python startup scripts | 064 accepted Rust ADR, route/security/migration/ops parity; no active runtime or test consumers |
| apps/mcp | native MCP protocol/tool/OAuth/tenant/session parity; client handshake verification |
| historical CI workflows | map required status checks, preserve parity/security/benchmark coverage and branch rules |
| tools/*.py | retain QGIS packaging, independent test tooling and migrations unless equivalent exists |
| repeated web-next CSS/components | reference/dependency graph, screenshots and interaction tests, 065 reconciliation |
| generated Cesium/WASM assets | license and reproducible build, content-addressed manifest |
| VPS worktrees/images/cache | active owner/release/rollback/backup inventory; separate authorization before destructive pruning |
For every path populate size, consumer, replacement, action, tests, commit and rollback. Never remove volumes, production data, backups, active worktrees or historical evidence. Do not rewrite history.
