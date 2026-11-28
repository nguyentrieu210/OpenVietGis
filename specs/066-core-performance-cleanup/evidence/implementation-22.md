# 066 implementation slice 22 — cached tile authorization after project-grant revocation

## T023 route-level regression

- Added an isolated Rust/PostGIS parity scenario on top of PR #85's integration branch using a newly seeded member without prior GIS ACL state.
- The fixture creates a project-scoped viewer grant, requests the same MVT URL twice with the same session, revokes that grant through the FastAPI fixture, and retries the identical URL.
- The test asserts the final Rust response is HTTP 403, is not an MVT response, and does not contain cached feature bytes.
- The same scenario passed on the earlier main-based source commit db4189912c88ce28baa88095c6272880c33de1d1 (Actions run 46147112725; log: PASS E05 cached tile rechecks revoked project grant).
- Verification on the PR #85 integration stack is pending for the current candidate.

## Acceptance state

- Do not mark T023 complete until the current integration-stack run passes the route test, cache burst/singleflight check, and Rust tile-cache capacity unit tests.
- SPEC 066 P0 acceptance remains open independently of T023.