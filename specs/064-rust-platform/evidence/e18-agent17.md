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
  CI workflow e18-ops-safety.yml runs these independently and defines a disposable PostGIS golden fixture with geometry, row content, ACL and tenant-denial probes; execution results remain pending.
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

## Remaining mandatory release gates

1. Run backup/restore on a quiescent synthetic fixture on a disposable PostGIS cluster;
   compare actual content, privileges/ownership, artifact manifest and tenant-negative probes.
   Current helper checks row counts, per-row content hashes and table ACL/ownership, but does not certify
   every database-level privilege, artifact, cluster-level role or recovery point. It is not a production backup replacement.
2. Execute actual baseline application after candidate acknowledged write and verify API read
   through old code. Current rehearsal is a synthetic schema/RLS check only (reported as such).
3. Pin exact image digests and configure hybrid ingress with a deny-by-default migrated-path registry.
   Existing Martin gateway is not certified: auth subrequest/header behavior and tile scope need
   E05 integration tests. Do not publish it to production.
4. Run identical fixtures against Python/Rust, compare p50/p95/p99/error rates and RSS on isolated CI;
   no aggregate speedup until all required scenarios pass with equal semantics.
5. Independent security review, supply-chain audit, real restore and soak, final gates T019–T022.

No production changes, no deletion of pre-existing DBs, no claim of E18 full acceptance.
