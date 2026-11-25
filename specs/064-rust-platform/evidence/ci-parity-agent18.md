# Agent 18 — CI and parity gate

Status: **blocked on integrated candidate and security fixes** (2026-11-26 ICT).

## Observed baseline

Candidate branch `064-ci-parity`, commit `b6dc6731eb8e3c2f670181089f1cda8917c2f352`.
[GitHub Actions run 45667321785](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45667321785): formatting, Clippy, workspace tests, architecture isolation and no-default-features API tests passed. The actual PostGIS HTTP differential suite reported **59 passed, 11 failed**; restricted database role/unscoped RLS and privileged owner rejection also passed. The 11 failures were E02 feature pagination, cursor, detail, CRS, revision and ACL tests (candidate routes return 404, including incompatible error envelopes). Do not disable `--include-features` to obtain a green run. E02 implementation/integration is a prerequisite.

Audit reported RUSTSEC-2026-0459 (`libsqlite3-sys` 0.30.1), RUSTSEC-2023-0071 (`rsa` 0.9.10) and unmaintained advisory RUSTSEC-2026-0543 (`chrono` 0.4.45); the audit action also failed to create a check run due to insufficient token permission. Dependency owners must resolve or document an explicitly reviewed disposition; no exemptions are added here. The audit CLI now requires read-only checkout permissions and preserves its exit code and JSON artifact.

## Agent 18 changes and validation

- Trigger CI on Python oracle/migrations and PostGIS compose/init changes, plus manual dispatch; bounded 30-minute jobs.
- Pin the observed audit CLI version (0.23.0); retain nonzero vulnerability exit and always upload JSON evidence, without granting check-write permission.
- Assert independent security invariants on both APIs even after normalization; six DB-free regression tests cover success, absent route, identical leaks, normalized candidate-only leaks, cache mismatch and redacted difference diagnostics.
- Local validation: six regression tests pass; Ruff check/format, workflow YAML structural check and `git diff --check` pass. Full Rust and PostGIS jobs not rerun on this commit: VPS1 had 2.3 GB free disk and approximately 456 MB free swap with parallel workloads. CI on the integrated commit must rerun all gates and preserve artifacts.

No parent or engine task is marked done based on this evidence; the historical run covers only a read slice, not all Spec 064 contract rows. Integration must verify the final candidate commit and security outcome before cutover.
