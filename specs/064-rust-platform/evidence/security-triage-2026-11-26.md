# Spec 064 security triage — 2026-11-26

Scope: Rust workspace dependency graph on the isolated `064-finalize` candidate. No production change.

## Results

- `cargo-deny 0.22.1 check licenses bans sources`: PASS. The workspace contains 224 packages; all encountered license expressions are covered by the explicit allow policy. Unknown registries/git sources are denied. `openssl-sys` is denied. Workspace crates are `publish = false`, so internal path dependencies cannot be published accidentally.
- RustSec advisory database: no vulnerability advisory was reported. One informational maintenance advisory remains: `RUSTSEC-2026-0543` (`chrono` is unmaintained; all versions affected; no safe chrono upgrade exists).
- `chrono` is a direct compatibility dependency for SQLx/PostgreSQL timestamptz and wire-exact parity code. Replacing it requires a qualified `time`-based migration rather than a blind dependency swap.

## Temporary triage

`deny.toml` records an expiring exception for `RUSTSEC-2026-0543` through **2026-12-10** only. The exception is intentionally self-expiring and `unused-ignored-advisory = "deny"` prevents stale suppressions. `.cargo/audit.toml` keeps unmaintained advisories visible as informational output.

This triage does **not** constitute owner approval. Before merge to `main`, the owner must either approve the temporary exception within the stated expiry or require the `chrono` -> `time` migration to complete and re-run all parity/security gates. Production cutover remains blocked by T019/T022 until that decision is recorded.
