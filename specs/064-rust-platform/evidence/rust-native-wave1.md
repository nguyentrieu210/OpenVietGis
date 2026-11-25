# Rust-native Wave 1 evidence — 2026-11-26

## Decision and base

Wave base: `c40b8e20` (`origin/064-convergence` at wave start).

Scope decision: `0425b7d5` restores the original Rust-native OpenVietGIS
control-plane boundary. OpenVietGIS-owned API, authorization, lifecycle,
revision, orchestration, jobs, sync, module and policy logic migrate to Rust.
PostGIS/GDAL/GEOS/PROJ/MapLibre/Cesium and technically required QGIS/CAD
adapters remain replaceable engines beneath that control plane.

No production deployment, database migration, route cutover, push or legacy
retirement was performed by this wave.

## Integrated candidate

Branch: `064-rust-native-wave1`

Integrated lane commits:

- E06/E08: `e53b3bb1`, `b5efcc9b`
- E09/E10: `a2224081`
- E12/E13: `f7d304b7`, `26b883b0`
- E14/E15/E16: `051d8e77`

Delivered in this wave:

- E06: renderer-neutral Rust cartography validation for physical paper/DPI and
  single-style bounds. Rendering engines remain external.
- E08: authenticated Rust processing algorithm catalog route reusing the
  canonical `ovg-processing` registry rather than duplicating metadata.
- E09: Rust raster request policy for source choice, cross-field operation
  constraints, Sentinel rules, resampling bounds and GeoTIFF magic.
- E10: authenticated Rust V8i capability route preserving fail-closed,
  read-only, not-paired semantics.
- E12: Rust integration-hub catalog policy for all 12 Python catalog adapters,
  including permission, declared-operation and executable-operation gates.
- E13: Rust offline package lifecycle guard; revoked/expired packages cannot be
  reactivated by replay.
- E14: Rust module activation compare-and-set policy plus idempotent
  disable/reactivate revision transitions.
- E15: checked house-number sequence arithmetic; overflow returns
  `NUMBERING_OVERFLOW` instead of panicking.
- E16: Rust MCP submit/result lifecycle policy matching one-command-at-a-time,
  session expiry, two-second submit rate limit and one-shot terminal result
  consumption.

## Rust validation

On the integrated candidate, using Rust 1.85.1:

- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS.
- `cargo build -p openvietgis-api-rust --no-default-features --locked`: PASS.
- `ovg-processing`: 8 tests PASS.
- `ovg-sync`: 8 unit + 3 integration tests PASS.
- `ovg-module-sdk`: 5 tests PASS.
- `ovg-module-house-numbering`: 6 tests PASS.
- New API policy/route tests: E06 2, E08 1, E09 2, E10 1, E12 3: all PASS.
- Isolated E16 MCP policy compiled with `rustc -D warnings --test`: 3 tests PASS.
- `git diff --check`: PASS.

A full `cargo test --workspace --locked` reaches the inherited base failure:

`ovg-storage::queries::domain::tests::canonical_json_matches_python_json_dumps`

Observed mismatch is the pre-existing Python-style ASCII JSON escaping expectation
already owned by the concurrent `064-finalize` worktree. All wave tests and all
workspace tests executed before that inherited failure passed. This wave did not
edit `crates/ovg-storage/src/queries/domain.rs` to avoid overlapping ownership.

## Python oracle checks

Using the existing pinned API environment:

- `test_processing_domain.py`
- `test_cartography_output.py`
- `test_v8i_capabilities.py`
- `integration_hub/test_catalog.py`

Result: 45 PASS.

Additional reference suites:

- offline snapshot + offline Core operation namespace
- E14 manifest parity
- House Numbering address prefix

Result: 34 PASS.

MCP retained bridge reference:

- `apps/mcp/test_live_bridge.py`

Result: 6 PASS.

## Open gates

Wave 1 is an implementation checkpoint, not engine acceptance or cutover.

Still required:

1. Merge the independently owned E02/finalize and E05 convergence fixes only
   after their own gates are green.
2. Add DB-backed Rust repositories and exact HTTP parity for the E06/E08-E16
   surfaces that are still policy/domain-only.
3. Exercise real PostGIS tenant/RLS, QGIS/PyQGIS, GDAL raster, CAD/Windows and
   browser consumers where applicable.
4. Run cross-runtime differential fixtures, failure/revocation/restart tests,
   resource benchmarks and rollback rehearsal on the integrated exact SHA.
5. Keep FastAPI routes as oracle/rollback until each engine-specific acceptance
   gate is independently green.

## E08 DB-backed vertical slice

After the initial wave checkpoint, E08 progressed from static registry ownership
to a real RLS-bound PostGIS vertical slice:

- Rust owns GET /processing/jobs, GET /processing/jobs/{job_id}, and
  POST /processing/jobs/{job_id}/cancel.
- The SQLx repository reads/writes the canonical processing.jobs table inside
  the already authorized ProjectRead transaction.
- Cancel uses FOR UPDATE, preserves JOB_TERMINAL, requires CSRF and
  project.edit, and commits atomically.
- Create/retry remain on FastAPI until the geometry publication transaction is
  ported as one unit; the wave intentionally does not split that transaction.

Real isolated PostGIS differential harness tests/rust-parity/e08_jobs.py passed:

- list jobs;
- get queued and failed jobs;
- missing job;
- foreign-tenant denial;
- limit bounds 0/201;
- cancel running job;
- terminal cancel conflict with exact code/message envelope;
- missing-CSRF fail-closed.

The harness found and corrected one real Rust HTTP envelope mismatch during
development (JOB_TERMINAL initially used the catalog detail envelope).
The final run is green against FastAPI on the same migrated database.

## Wave 2 incremental DB-backed parity

### E06 cartography read side

Commit: `d0a2da69` (integrated checkpoint with E13).

Rust now owns:

- `GET /cartography/layers/{layer_id}/style`
- `GET /cartography/layouts`

Both read canonical RLS-protected cartography tables inside the same authorized
project transaction. Differential parity on isolated real PostGIS passed:

- get existing layer style
- missing style -> `{"style": null}`
- list layouts
- foreign-tenant denial

### E08 durable processing jobs

Code commit: `84ae8c96`; evidence follow-up: `42cffe31`.

Rust now owns:

- `GET /processing/jobs`
- `GET /processing/jobs/{job_id}`
- `POST /processing/jobs/{job_id}/cancel`

The create/retry paths remain on FastAPI until the complete geometry publication
transaction is ported atomically. Differential parity on isolated real PostGIS
passed list/get/missing/foreign-tenant/limit validation/cancel/terminal conflict
and CSRF fail-closed cases.

### E13 offline package online control

Commit: `d0a2da69`.

Rust now owns:

- `GET /field/packages`
- `POST /field/packages/{package_id}/revoke`

The list path preserves the Python writable-package policy: `project.edit`,
creator-only visibility, and online layer/source active-ready revalidation.
Revoke is idempotent via `COALESCE(revoked_at, now)`. Differential parity on
isolated real PostGIS passed list/foreign-tenant/revoke/idempotency/missing/CSRF
cases.

### E09 raster catalog read side

Commit: latest E09 checkpoint on `064-rust-native-wave1`.

Rust now owns `GET /raster/layers` using the canonical RLS-protected raster
catalog. The Python wire merge order is preserved exactly: metadata may override
`id` and `name`, while canonical `byte_size` and `created_at` win after
metadata unpacking. Differential parity on isolated real PostGIS passed:

- raster list order and wire shape
- metadata key-collision precedence
- foreign-tenant denial

### E10 CAD document read side

Rust now owns:

- `GET /cad/documents`
- `GET /cad/documents/{document_id}/versions`

The native DWG/DXF engines and file bytes remain below the Rust control plane.
The Rust repository reads the canonical RLS-protected immutable CAD version
ledger and preserves the Python latest-per-document and descending history
ordering. Differential parity on isolated real PostGIS passed:

- latest document revision selection
- version history ordering
- missing-document 404 envelope
- foreign-tenant denial
