# E01/E02 takeover — 2026-11-26

- Base branch: `064-e01-e02`, remote commit `4e1e862f75792ce94cb65c05db30e693f75f63ca`.
- Worktree: `/opt/ovg-064-e01-e02` (isolated, sparse checkout). The prior Windows worktree could not be reached; two reported uncommitted files were not imported or overwritten.
- Inventory: `contracts/engines/e01.json` (54 HTTP operations) and `e02.json` (22). Existing inventory evidence is not equivalent to implementation or parity.
- Added E01 GET resource grants: human tenant membership, `authorization.manage_grants`, tenant-filtered RLS transaction, public serialization including revoked grants. No mutation or credential secret exposed.
- Added E01 POST ownership-transfer: CSRF, organization lock, current Owner and target row locks, promote before demote, atomic success audit; authorized tenant denial audit commits without mutation. No new schema or dependency.
- Expanded `tests/rust-parity/run.py`: grant list positive/negative cases; populated grant list and audit comparison; owner transfer round trip, missing CSRF, member denial, fixture role restoration, audit evidence. The suite uses an isolated fixture DB, never production.
- Checks actually executed: `rustfmt --edition 2021 --check` for changed Rust files (1.85.1), `python3 -m py_compile tests/rust-parity/run.py`, `git diff --check` — all passed. Compilation and runtime parity are separate gates, not yet verified at the time of this checkpoint.

## Remaining acceptance gaps

E01: OIDC login/callback, bootstrap initialization, invitation lifecycle, avatar mutations/identity unlink, grant creation/revocation, GIS ACL administration/reviews/export, service principal and credential lifecycle/authentication. Existing human-only endpoints must still prove exact wire, lock, tenant and error parity; valid service credentials are not implemented on the candidate. E02: source/layer mutations, geometry preview, map persistence/history/operation lookup, spatial changes, display extent and viewport (coordinate with the dedicated viewport agent). Existing read and CRS ports also require end-to-end differential verification. E01/E02 benchmark, dependency/security review, independent integration and rollback gates are open. No engine completion or deployment is asserted.

## Inherited CI findings

GitHub Actions run 45670685353 on the pre-takeover commit `4e1e862`: PostGIS parity succeeded, but Clippy failed on `nonminimal_bool` in the project workspace filter and Cargo Audit failed on RUSTSEC-2026-0459 (`libsqlite3-sys` 0.30.1) and RUSTSEC-2023-0071 (`rsa` 0.9.10); `chrono` also has RUSTSEC-2026-0543 unmaintained warning. The boolean expression was simplified without changing behavior. The shared dependency findings remain release blockers until dependency reachability and upgrade/removal are verified. No advisory is ignored and no CI gate is disabled.

## Follow-up implementation

Added tenant-locked grant create/revoke, service principal list/create/revoke, credential metadata/issue/revoke. Credential generation uses OS-provided 256-bit entropy, URL-safe token encoding and SHA-256 digest storage; the plaintext is returned only on issuance with Cache-Control: no-store. Permission ceiling must be a subset of currently granted permissions. Revocations are idempotent. New differential lifecycle checks exercise denied member/foreign-tenant access, CSRF, duplicate names/grants, invalid scope/ceiling, credential metadata and repeated revocation. Service credential authentication on protected GIS routes remains a separate open implementation item. The preceding commit `e6c007c` passed the GitHub Clippy/workspace test and PostGIS parity jobs (run 45673193884); the new implementation requires its own CI. Rustfmt, Cargo locked metadata, Python syntax and git diff checks ran for this change. Cargo Audit is still an open gate.

## Map read and receipt parity

The grant/service-principal/credential administration commit `843e732` passed Format, Clippy, Workspace Tests, Architecture Isolation and PostGIS Parity (GitHub Actions run 45673816153). Added project map state, bounded immutable history and actor-owned operation receipt reads. Map identity is UUIDv5 using the project namespace. GIS-scoped accounts receive only RLS-visible layers and no default viewpoint; history and receipt access fail closed. Receipt access rechecks project.edit, active lifecycle and organization/project lock order. Added empty, persisted, foreign-tenant, scoped-map and receipt differential fixtures; these require fresh CI. Map PUT, catalog writes and the other previously listed gaps remain open. No production changes.
