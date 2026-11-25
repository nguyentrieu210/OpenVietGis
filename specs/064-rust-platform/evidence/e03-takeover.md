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

## Service-auth verification at 0eb1a5a

CI [45674243065](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45674243065):
format, strict Clippy, architecture isolation, workspace tests, no-default-
features tests passed. PostGIS parity: 128 pass, 11 E02 failures; all 69
E03 cases pass, including 12 new bearer tests. Credential revocation, live
grant revocation, permission ceiling, cross-tenant denial, actor
attribution, invalid bearer precedence, Unicode digest and revert replay
were tested with synthetic service credentials.

Repeat benchmark (same methodology, candidate 0eb1a5a; raw artifact in
that run): editing p95 c20 265.441 ms Python / 96.668 ms Rust;
snap p95 c20 203.667 / 164.813 ms; peak RSS 188.59 / 17.50 MiB;
zero unexpected errors. Editing p95 c10 147.041 / 195.969 ms
(regression). Do not claim uniform speedup. The new topology fixtures
require another CI run; independent release/security gate remains open.

Dependency reachability: offline `cargo tree --locked --offline -p
openvietgis-api-rust --edges normal -i rsa` and the same command for
`libsqlite3-sys` both returned "nothing to print" on the Linux candidate.
These lockfile advisories therefore do not occur in the active normal
API dependency graph for that target. This is scoped evidence, not a
waiver: do not suppress the workspace audit or infer other targets are safe.
`chrono` is active and needs a shared maintenance decision.

The 0eb1a5a benchmark/parity artifact is
[15050852754](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45674243065/artifacts/15050852754)
(archive SHA-256 fa5389d4d58e94e004d18320919a26fb69a24c43451b296ac468819738627f9e).
The tested branch and integration c16a34b produced a conflict-free
`git merge-tree` dry run; no other branch was changed.

## Final E03 code/test candidate: 2b12dda

CI [45674622700](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45674622700):
**77/77 E03 HTTP parity checks pass**. Added eight real GEOS/concurrency/lease
tests: invalid self-intersection, atomic rejection, open ring, concurrent
Python/Rust replay of a polygon with a hole, active lease denial, expired
lease acceptance, short line rejection, valid line replay. Workspace tests,
strict Clippy, formatting, architecture isolation and no-default-features
tests pass. Overall HTTP parity 136 passed / 11 failed, all failures E02
missing feature endpoints on the isolated E03 branch.

Repeat candidate benchmark (same run, 0 unexpected errors):
editing p95 c20 Python 303.898 ms / Rust 209.803 ms; p99 321.403 / 213.770;
snap p95 c20 509.046 / 182.902 ms; peak RSS 188.50 / 18.27 MiB.
Editing p95 c10 171.390 / 220.967 ms remains a regression. Results are
host-specific; no blanket speedup or deployment acceptance is asserted.
Raw evidence [artifact 15052050676](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45674622700/artifacts/15052050676)
archive SHA-256 e82854d4208d7831cc953c69bfd88894fd5f0314659d6e595eb5e52ae1871607.

**Agent handoff status: code and E03-specific tests complete; release
acceptance BLOCKED pending E01/E02/E05/E11 integration and independent
security/consumer review.** E03-T01 through T04 are complete. E03-T05
remains open because the parent AC4 requires unresolved gaps to block
acceptance. No test exclusions, advisory ignores, production deployment
or cross-branch changes have been made.
