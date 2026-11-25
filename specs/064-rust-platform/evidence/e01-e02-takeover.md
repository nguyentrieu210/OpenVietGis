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

## Atomic map save

Implemented project.edit-scoped PUT map with strict validation, stable map ID, locked revision check, operation intent fingerprint, idempotent replay, active/renderable layer reference checks, and a single transaction for map state, audit, operation and revision. Included a Python-compatible JSON canonicalization test for Unicode, float exponent spelling and large opaque JSON integers. Differential cases cover replay across runtimes, stale revisions, conflicting operation IDs, no changes, CSRF and tenant boundaries. This candidate still requires its own compilation and PostGIS CI verification; no claim of full map parity until that run passes. E01/E02 acceptance tasks remain open.

## Invitation and self-identity lifecycle; map regression corrections

Implemented invitation list/issue/revoke/check/accept, exact authenticated recipient proof, cryptographic secret digest, expiry, organization lock, owner-invite authority and no owner downgrade, membership/grant reuse, and transactional audit. Added guarded identity unlink (retaining at least one usable identity) and self-avatar deletion. Added differential positive/negative lifecycle fixtures. Map commit `667e61d` compiled and passed Clippy but failed CRS controls because arbitrary_precision globally changed JSON number equality, and failed a group/display write because the existing PostgreSQL validator did not support Spec 056 fields. Removed the global feature, reject out-of-range integer literals rather than round silently, and add forward migration 0046_map_display_config preserving original constraints and requiring a validated downgrade (no stripping of historical data). The Rust map API conservatively rejects opaque integers beyond signed/unsigned 64-bit; full Python arbitrary-integer parity is not claimed. The SQL display size check uses JSONB textual representation and is conservative. Migration has not been deployed; new runtime/CI checks are required. Previous map read commit `68133ef` passed primary checks and PostGIS parity (45674206518); audit was cancelled by superseding work. Remaining: OIDC/bootstrap, invitation email delivery, avatar upload, service authentication, GIS ACL administration, spatial catalog writes and preview/extent/viewport integration, performance and independent acceptance/security gates.

## Combined E01–E03 candidate

Reconciled 49f2c66 and 3cc926d with the six-engine integration base
6e3b74c. Map writes, map receipt reads and saved-query mutations now use the
same project edit-scope policy. The browser editing CSRF check uses the
shared identity verifier. The parity runner retains both engines' fixtures.

Added the bounded human-session catalog history read using the current
unrestricted project scope, keyset pagination and the existing database's
safe metadata constraints. The differential fixture records two actual
catalog mutations via FastAPI and compares the public history and pagination
from both runtimes. It also checks foreign-tenant, scoped-actor and anonymous
denial. The candidate needs a new GitHub Actions run before promotion.
Valid service credentials on catalog reads still require the Python route.
