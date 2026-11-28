# T008 partial evidence — E10 CAD/georeferencing security and revision parity

Status: **Partial; T008 remains open.** The E10 Rust routes and handlers were already present at PR #82 head 76d624e9ef4d35e52d63f3a9cc7241ce442944a1. This change adds differential coverage for tenant isolation and stale revisions; it does not close the full 064 route, migration, operations, or ADR gates.

## Change

- Branch: `agent/066-e10-cad`.
- Test commit: `e5d96e68d12d920fe55b5f25e59635c3fc3e3729`.
- Changed file: `tests/rust-parity/e10_cad_dxf.py`.
- Added oracle-vs-Rust negative cases for a foreign tenant against `inspect-georef`, `import-georef`, `export-georef-draft`, and `preview-native-import`.
- Added stale content revision, preview fingerprint, and candidate SHA rejection cases. Expected responses are 409 and are compared against FastAPI.
- Existing route code continues to enforce the current project edit scope, CRS validation, document/version pins, source revisions, preview acknowledgement, and bounded CAD payload behavior. No route implementation or authorization rule was weakened.

## Validation

- `python3 -m py_compile tests/rust-parity/e10_cad_dxf.py` — PASS.
- `git diff --check` — PASS.
- `E10 CAD/DXF parity` against isolated PostGIS — **15 named parity cases PASS**, plus existing content, digest, and entity-count assertions.
- Test database: disposable container from pinned image `postgis/postgis@sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262`; loopback port 54331; temporary parity database dropped by the harness.
- Rust candidate binary used for this run: CI-fix commit `29eaf10574d541221339d130f57b6bb0a70dc7ef`. The test commit is isolated and has not been integrated with that code commit, so this is not an exact integrated-head acceptance certificate. Re-run E10 parity on the combined PR #82 candidate before closure.

## Remaining T008 gates

This E10 slice does not certify remaining route groups, Alembic/Rust migration and rollback parity, operations parity on the integrated candidate, ADR-015 review/decision, independent review, or final release.
