# E03 takeover — Geometry, editing, topology

Scope: branch `064-e03`, five runtime editing routes, canonical PostGIS and
R006 wire contracts. No production deployment or data mutation.

## Evidence

- E03-T01: operation inventory is in `e03-inventory.md` and
  `../contracts/engines/e03.json`: all five routes, 1 migration, 33 mapped
  baseline test identifiers.
- E03-T02/T03: GitHub Actions run
  [45673109845](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45673109845)
  at `c752ef6` records 57 E03 passes (49 original, 8 scoped ACL additions);
  total differential suite 116 passed, 11 failed (E02 feature routes not
  yet present on this isolated branch). The subsequent `e80445e` run
  repeats those HTTP checks and adds the benchmark below.
- E03-T04: comparative benchmark on GitHub Actions run
  [45673696861](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45673696861),
  job 139415455877, `e80445e`. Raw per-request evidence:
  artifact `postgis-parity/e03-benchmark.json`, and `e03-benchmark.log`.
  Candidate is a debug build, one process per engine, 15 DB connections
  max, fresh deterministic database per engine, no production traffic.
  Cold denotes first requests after process readiness, not a physical
  database-cache flush. Single run on CI host; not a 4 GiB VPS claim.

| Scenario | Concurrency | Python p50/p95/p99 ms | Rust p50/p95/p99 ms |
|---|---:|---:|---:|
| Editing | cold | 37.810 / 64.522 / 66.896 | 33.228 / 48.259 / 49.595 |
| Editing | 1 | 37.772 / 41.493 / 41.962 | 27.238 / 29.356 / 32.882 |
| Editing | 10 | 149.661 / 229.168 / 245.063 | 104.403 / 253.979 / 269.804 |
| Editing | 20 | 286.592 / 399.974 / 407.450 | 121.074 / 191.812 / 207.493 |
| Snap | cold | 15.960 / 150.628 / 162.599 | 8.260 / 17.068 / 17.851 |
| Snap | 1 | 14.900 / 17.427 / 18.773 | 8.022 / 8.977 / 9.430 |
| Snap | 10 | 108.866 / 312.821 / 345.302 | 31.442 / 71.792 / 94.232 |
| Snap | 20 | 227.241 / 300.571 / 326.777 | 57.749 / 206.421 / 257.668 |

All scenario groups recorded zero unexpected errors (expected revision
409 is not an error). Peak process RSS: Python 189.14 MiB, Rust 17.03 MiB.
Editing p95 at c20 improved by 52.0%; at c10 regressed by 10.8%. The
benchmark must be rerun for subsequent commits; this is not a release gate
pass or independent review.

## Security and convergence

- Scoped writes verify source permission before internal transaction-local
  ACL mode, then verify before and after snapshots, including replays.
  Mixed denied edits rollback atomically; read-history and revert remain
  denied for scoped users.
- Service credential changes add a tenant-pinned SHA-256 constant-time
  lookup, live credential and grant revalidation, permission ceiling, audit
  actor attribution and lease semantics. No production credential is used.
  Differential tests and CI results for this change are required before
  acceptance.
- Existing RustSec failures (workspace lockfile) include
  RUSTSEC-2026-0459 (libsqlite3-sys) and RUSTSEC-2023-0071 (rsa); chrono
  maintenance warning. Shared dependency resolution belongs to platform
  integration; do not suppress advisories on E03.
- E01/E02 feature API convergence, E05 cache invalidation, E11 consumer
  convergence and independent integration/security review are required.
  Geometry preview split/merge is inventoried under spatial route ownership
  and must be reconciled in integration. No API removal is approved.
- Local Windows worktree was inaccessible; VPS1 worktree and GitHub tree
  were checked against each other. No changes were made to other agent
  branches.
