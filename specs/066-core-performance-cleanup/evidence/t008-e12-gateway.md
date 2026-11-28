# T008 partial evidence — E12 connector gateway routes

Status: **Partial; T008 remains open.** This slice dispatches implemented E12 routes to Rust only for cookie sessions. It does not complete the 064 route/migration/operations matrix, certify the adapter data contracts on the integrated candidate, accept ADR-015, or retire FastAPI.

## Scope

The hybrid gateway sends these Rust-supported E12 route families to the Rust API when the request uses a session cookie without a bearer authorization header:

- Organization external-source allowlist: GET and POST.
- Project external-source list/create: GET and POST.
- External-source lifecycle and provider operations: POST invalidate-cache, revoke, preview, and viewport.
- Integration adapter export: GET for QGIS, PostGIS, and AutoCAD.
- GeoPackage sync preview/push: POST for QGIS only.

Bearer callers, unsupported adapters, wrong methods, malformed UUID paths, and unmatched suffixes continue to FastAPI. Rust proxy errors do not fail over to Python; proxy_next_upstream off remains in force.

## Validation

On an isolated development clone based on PR #82 head 76d624e9ef4d35e52d63f3a9cc7241ce442944a1:

- python3 -m py_compile tests/rust-parity/e12_gateway_smoke.py — PASS.
- python3 tests/rust-parity/e12_gateway_smoke.py — PASS: 13 cookie requests dispatched to Rust, 13 bearer requests routed to FastAPI, 6 unsupported/malformed cases routed to FastAPI, Rust outage returned 502 without Python failover.
- Embedded Nginx configuration validation — PASS.
- git diff --check — PASS.

The standalone smoke uses local recording upstreams. It verifies method, raw URI/query, cookie/bearer, CSRF, content type, body forwarding, and selected upstream; it does not execute the handlers or prove external-provider/database behavior.

Existing database-backed E12 differential coverage is in tests/rust-parity/e12_adapter_transfer.py, e12_catalog.py, e12_profiles.py, and e01_e10_e12_wave.py. Those semantic checks were not run on this isolated smoke environment; they must pass on the integrated exact candidate in GitHub Actions.

## Changed files

- infra/rust-platform/hybrid-gateway.conf
- tests/rust-parity/e12_gateway_smoke.py
- specs/066-core-performance-cleanup/evidence/t008-e12-gateway.md
