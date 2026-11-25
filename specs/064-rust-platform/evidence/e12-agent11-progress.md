# Agent 11 / E12 progress (2026-11-26)

Branch: `064-e12`, isolated VPS1 worktree `/opt/ovg-064-e12`, base `1a5cb7b`.
Reference baseline: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`.

## Acceptance status

- E12-T01 / AC1: existing inventory evidence records 26 mounted routes (one SSE), 27 tracked source files and 96 mapped test IDs. This is inventory evidence only.
- E12-T02 / AC2: partial. Shared immutable COG range fixture covers 14 synthetic positive and negative cases, executed against the actual Python HTTP transport (mock DNS/TLS/socket) and Rust pure policy. No QGIS desktop / real HTTP PostGIS consumer equivalence yet.
- E12-T03 / AC2: partial. Rust candidate validates bounded ranges, response status/encoding/media, strong ETag, total, exact offset, EOF clipping, and body length. This is a pure policy module, not outbound TLS transport, provider ACL, route, or authority. No Rust E12 routes mounted. Legacy Python/PyQGIS unchanged. No retirement approved.
- E12-T04 / AC3: open. No representative candidate endpoint exists to benchmark; latency and RSS claims are not made.
- E12-T05 / AC4: open. Independent QGIS/Windows consumer, RLS/ACL, revision and E11 replay integration, full dependency/security review and rollback qualification remain necessary.

## Observed test results

| Suite | Result |
|---|---|
| Python QGIS managed snapshots, simple UI, recovery, plugin client, workspace, sync diff, three-way reconciliation | 89 passed |
| Python external provider, COG and raster controlled fixtures (excluding two real GDAL cases) | 54 passed; 2 deselected |
| Python shared range parity cases | 14 passed |
| Rust 1.85.1 standalone candidate | 5 passed (including all 14 shared range cases) |

Python 3.13.18; pinned pydantic 2.14.3/core 2.50.4 cache paths prioritized. Unit tests used `--noconftest -p no:cacheprovider` to avoid touching shared test databases; no full application integration result is implied. Rust used read-only worktree, network-disabled disposable container with 256 MB RAM and 0.5 CPUs. `rustfmt` component unavailable; full cargo workspace build and formatting not verified. Initial combined Python run exceeded command budget; suites rerun independently with results above.

## Required convergence

1. E01/E02 authorization and CRS interfaces and E03/E11 revision/cursor interfaces must converge before mounting the 26 route contracts.
2. Implement real restricted HTTPS adapter: operator allowlist, mixed-answer SSRF denial, DNS pinning, peer/TLS hostname verification, no proxies or redirects, total deadline, bounded concurrency and streamed body limits; reject duplicate headers before policy validation.
3. Port binding lifecycle, pairing credentials, profile/export, sync preview/push, conflict resolution, SSE with reauthorization. Verify exact legacy response and error envelopes, permission revocation and replay.
4. Run real PostGIS isolation and actual QGIS/Windows consumer tests, independent GDAL pixel oracles, cold/warm p50/p95/p99/RSS/errors, then independent review. Keep Python rollback until acceptance and approved ADR.
5. No cutover, deploy, or automatic task closure. No credentials or production data used.
