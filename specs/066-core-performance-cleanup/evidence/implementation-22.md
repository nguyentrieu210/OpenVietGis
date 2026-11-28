# 066 implementation slice 22 — cached tile authorization after project-grant revocation

Candidate source commit: db4189912c88ce28baa88095c6272880c33de1d1.

## T023 route-level regression

- Added an isolated Rust/PostGIS parity scenario using a newly seeded member, without prior GIS ACL state.
- The fixture creates a project-scoped viewer grant, requests the same MVT URL twice with the same session, revokes that grant through the FastAPI fixture, and retries the identical URL.
- The test asserts the final Rust response is HTTP 403, is not an MVT response, and does not contain cached feature bytes.
- GitHub Actions run 46147112725, job “PostGIS Parity (FastAPI oracle vs Rust)”: Run differential parity PASS. The exact log line is: PASS E05 cached tile rechecks revoked project grant.
- The same run’s Verify private tile cache authorization and current revisions step also passed, covering existing warm-hit identity, concurrent single-leader behavior, stale revisions, retirement, and session revocation.

## Limitations

- The overall PostGIS job is red because the independent E07 transfer parity script raises NameError: name 'name' is not defined in tests/rust-parity/e07_transfers.py::unsafe_archive.
- The Rust format job is red on an untouched formatting difference in crates/ovg-storage/src/viewport.rs:655; its workspace unit-test step was skipped. The inline Rust tile-cache capacity tests therefore still need execution on a formatting-clean candidate before T023 can be marked complete.
- This evidence closes the project-grant revocation route-test gap only. It does not claim full T023 acceptance or SPEC 066 acceptance.