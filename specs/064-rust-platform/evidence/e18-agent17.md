# E18 Agent 17 — operations safety handoff

Base: 1a5cb7b40a8a0c8117b8aa56c1522e35d01a5b3b. Scope: E18 only.
Owner's hybrid decision is binding; Python public release-capabilities, entitlements and legacy
Prometheus contract must remain routed to Python until Rust parity is independently proven.
Rust operational readiness/metrics are service-local; do not replace legacy metric names.

## Evidence and status

- T01 / AC1: inherited inventory e18-inventory.md and e18.json (8 HTTP routes, 69 source files,
  149 mapped test IDs); hybrid routing split requires integration owner review.
- T02: three local safety tests passed (mocked Docker, no DB mutation): unsafe target/source and
  long identifiers denied, existing DB collision never dropped, benchmark target guards.
  CI workflow e18-ops-safety.yml runs these independently and repeats the
  disposable PostGIS rehearsal described below.
- T03: hardening implemented: isolated containers, fresh target only, ownership-aware cleanup,
  private backup artifacts, no creation of privileged/shared cluster roles, optional migration-table
  probe fixed, free-space lower bound, per-row SHA-256 fingerprints and table ACL/ownership comparison; benchmark uses unique DB, strict loopback ports and
  name allowlist, no overwrite. Rust ops readiness DB probe capped at 2 s, updates readiness gauge.
  Existing public release/entitlement/metrics contracts stay on Python, pending explicit hybrid ingress verification.
- T04: no comparative Rust performance claim. VPS1 observed 96% root disk usage, approximately
  2.3 GiB free, 3.8 GiB RAM and 3.6/4 GiB swap used; do not build or run heavy staging here.
  Historical Python baseline exists; matched Rust benchmark, cold/warm percentiles and peak RSS pending CI.
- T05: local shell syntax, Python compilation, tests and git diff check passed.
  Rust fmt/clippy/test and independent review pending CI. No deploy or live DB changes.

## Disposable PostGIS restore

At 18:34 UTC on 2026-11-25, the pinned CI image (PostgreSQL 18.7, PostGIS
3.7.0) ran in a temporary container with networking disabled. The restore
reported `parity_status=PASS` in 8 seconds for a 16 MiB synthetic source
(1 s dump, 3 s restore, 2 s verification). Source and target matched on:

- Schema SHA-256 `3180bba4796a364173635890040e68f4d649716f92680bab5f161e89ad9a9281`
- Row-count SHA-256 `74e2f1deb5f5071d382da552c5644f4a3f2f3c3dfa0f6976d1552f04570d824b`
- Row-content SHA-256 `2602241171fe66be764bdb1401792134917999afe41dbfa044127f2837c475a6`

Under the restored non-bypass runtime role, tenant `a` saw only `1:first`
and an unmatched tenant saw zero rows. The database and temporary storage
were removed with the fixture container. The test used the script's
`--backup-dir` option because the default tmpfs did not meet its free-space
check. These timings describe a single synthetic restore.

## Remaining mandatory release gates

1. Exercise the full application database and storage recovery, including
   database-level and cluster-level privileges, artifact manifests and recovery
   point verification. The fixture covers table ACLs, RLS and database rows.
2. Execute actual baseline application after candidate acknowledged write and verify API read
   through old code. Current rehearsal is a synthetic schema/RLS check only (reported as such).
3. Pin exact image digests and configure hybrid ingress with a deny-by-default migrated-path registry.
   Existing Martin gateway is not certified: auth subrequest/header behavior and tile scope need
   E05 integration tests. Do not publish it to production.
4. Run identical fixtures against Python/Rust, compare p50/p95/p99/error rates and RSS on isolated CI;
   no aggregate speedup until all required scenarios pass with equal semantics.
5. Independent security review, supply-chain audit, real restore and soak, final gates T019–T022.

No production changes, no deletion of pre-existing DBs, no claim of E18 full acceptance.
