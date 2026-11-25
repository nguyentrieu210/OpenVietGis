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
