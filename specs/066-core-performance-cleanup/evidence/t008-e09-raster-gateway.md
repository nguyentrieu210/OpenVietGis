# T008 partial evidence — E09 raster gateway ownership

Status: **Partial; T008 remains open.** This slice exposes existing Rust raster routes through the isolated hybrid gateway for cookie-session requests. It does not certify the full route/migration/operations matrix, accept ADR-015, retire FastAPI, or authorize a live rollout.

## Candidate and route ownership

- PR #82 base head audited: `76d624e9ef4d35e52d63f3a9cc7241ce442944a1`.
- Rust route composition: `apps/api-rust/src/routes.rs` merges `raster_policy::router()`; that router merges `raster_compute::router()`.
- Existing Rust handlers cover:
  - `POST /raster/analyze`
  - `POST /raster/layers`
  - `POST /raster/reports`
  - `POST /raster/layers/{layer_id}/terrain-preview`
  - `GET /raster/layers`
  - `GET /raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png`
  - `GET /raster/layers/{layer_id}/file`
  - `DELETE /raster/layers/{layer_id}`
- The hybrid gateway previously sent these paths to its FastAPI default even though Rust handlers existed. This slice adds exact method/path dispatch for the eight implemented routes.
- Cookie-session requests select Rust. Requests with an Authorization header continue to use FastAPI until bearer parity is separately certified. Unsupported/malformed route shapes remain on FastAPI. Rust upstream errors fail closed; Nginx does not retry these requests against Python.

## Functional differential coverage already in the repository

- `tests/rust-parity/e01_e10_e12_wave.py::check_raster_compute` generates a real projected GeoTIFF and compares FastAPI with Rust for analysis, persisted raster creation, DEM terrain preview and contour report results.
- `tests/rust-parity/e09_raster_catalog.py` compares the raster catalog and checks foreign-tenant denial.
- `tests/rust-parity/e09_raster_content.py` compares tile and file bytes/ranges, invalid/missing tile behavior, foreign access, deletion, audit rows and CSRF denial.
- These DB/GDAL differential tests must run on the integrated candidate SHA. The new local gateway smoke does not claim to exercise application handlers or PostGIS.

## Validation for this slice

- `python3 -m py_compile tests/rust-parity/e09_gateway_smoke.py` — PASS.
- `python3 tests/rust-parity/e09_gateway_smoke.py` — PASS: 8 cookie-session E09 routes dispatched to Rust, 8 bearer fallbacks to FastAPI, malformed tile fallback, cookie-session failure closed when the Rust upstream is stopped, and bearer/health requests still served by FastAPI.
- The smoke creates a temporary Nginx configuration and two local recording upstreams; its embedded `nginx -t` passed. It uses no production endpoint, database, credentials, GDAL or application process.
- `git diff --check` — PASS.

## Acceptance boundary

This proves the gateway can dispatch the currently implemented E09 Rust routes without breaking the retained bearer path or sending failed Rust requests to FastAPI. The exact candidate still requires the existing DB-backed E09 differential suite, current-head parity checks and the remaining Spec 064 route/migration/operations and ADR gates before T008 can close.
