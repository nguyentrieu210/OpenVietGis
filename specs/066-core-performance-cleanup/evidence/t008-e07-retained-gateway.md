# T008 partial evidence — E07 retained transfer routes

Status: **Partial; T008 remains open.** This slice preserves the working E07 routes through the staged hybrid gateway. It does not migrate GeoPackage or ZIP Shapefile parsing into Rust, close the 064 route/migration/operations matrix, accept ADR-015, or retire FastAPI.

## Candidate and change

- PR #82 source head audited: 76d624e9ef4d35e52d63f3a9cc7241ce442944a1.
- Isolated branch: codex/t008-route-parity-20261128.
- E07 gateway-test commit: 615345d9c0113705ff6e90f0bd552304d709a7da.
- Updated tests/rust-parity/hybrid_gateway_smoke.py to exercise all 11 E07 routes through Nginx with cookie and bearer credentials. It checks the selected upstream, method, raw URI/query, cookie, bearer authorization, CSRF header, content type, body forwarding, and response status. It also verifies all E07 paths continue to reach FastAPI while the Rust upstream is down.

## Validation

Run against the isolated Nginx configuration and recording upstreams:

- python3 -m py_compile tests/rust-parity/hybrid_gateway_smoke.py — PASS.
- python3 tests/rust-parity/hybrid_gateway_smoke.py — PASS: 49 cookie-route checks, 11 E07 bearer-route checks, 11 retained E07 checks after Rust-upstream failure, plus two outage checks.
- Nginx 1.26.3 configuration validation — PASS.
- git diff --check — PASS.

The existing E07 strict differential gate in GitHub Actions run 46109085094 tested merge candidate 388b0ff0a62367c48c7cea338d6527378e39c277. Its log reports Rust/Python GeoJSON round-trip PASS; GeoPackage and ZIP Shapefile FastAPI round-trips PASS on the retained Python path; archive traversal and legacy GeoJSON CRS checks PASS; E07 strict gate: 0 failures. That run predates this gateway-test commit, so it is supporting evidence for the existing behavior, not CI evidence for commit 615345d.

## E07 ownership boundary

The current Rust transfer router and codec support GeoJSON end to end. The Rust route explicitly returns FORMAT_NOT_MIGRATED for GeoPackage and ZIP Shapefile. The Python format registry has the GeoPackage and Shapefile adapters. Nginx defaults unmatched transfer requests to FastAPI, and the updated smoke covers the complete import, export, job and artifact route family for both cookie and bearer traffic.

Therefore the two formats are **available through the current FastAPI-owned gateway contract** but **not migrated to the native Rust API**. Do not report them as native Rust parity. The gateway test protects the staged retained boundary; it does not replace the existing data-backed E07 differential test or prove Rust parsing.

## Remaining T008 blockers

### Native route inventory

The retained evidence file specs/064-rust-platform/evidence/remaining-native-route-audit-2026-11-27.md records 30 baseline operations not found in mounted Rust router groups during a static review of candidate 1e0a0b24ed478e482b465bb43e54779186c8f568. That audit explicitly says it is not a runtime parity certificate and requires a regenerated inventory. The list below is the documented native-router gap set; it does **not** mean every operation is absent from the overall product. Several are still served by FastAPI.

- E01: GET /auth/login/{provider}; GET /auth/callback/{provider}; PUT /me/avatar; POST /organizations/{organization_id}/invitations/email.
- E10: POST /organizations/{organization_id}/projects/{project_id}/cad/documents/{document_id}/versions/{version}/export-georef-draft; POST the corresponding import-georef, inspect-georef and preview-native-import operations; POST /organizations/{organization_id}/projects/{project_id}/cad/export; POST /cad/import; POST /cad/inspect.
- E12: GET and POST /organizations/{organization_id}/external-sources/allowlist; GET and POST /organizations/{organization_id}/projects/{project_id}/external-sources; POST external-sources/{connection_id}/invalidate-cache, /preview, /revoke and /viewport; GET integration-hub/adapters/{adapter_key}/layers/{layer_id}/export; POST the corresponding sync-preview and sync-push operations.
- E09: POST /organizations/{organization_id}/projects/{project_id}/raster/analyze; POST /raster/layers; POST /raster/layers/{layer_id}/terrain-preview; GET /raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png; POST /raster/reports.
- E13: POST /organizations/{organization_id}/projects/{project_id}/field/packages; GET /field/packages/{package_id}/snapshot; POST /field/packages/{package_id}/sync.

The 064 E09–E12 integration evidence further states that raster and CAD/3D operations remain FastAPI-owned, E11 Rust replay/streaming routes are not exposed while Python remains authoritative, and E12 native transport/lifecycle work remains in Python. These are retained service boundaries, not confirmed product outages. The exact current candidate still needs a regenerated method/schema/auth inventory and independent behavior tests.

### Migrations and operations

- Alembic remains the schema migration authority under apps/api/migrations/versions; the checked source includes versions through 0046_map_display_config. No separate Rust migration directory was found under crates/ovg-storage. This appears to be an intentional retained migration path, but T008 still needs an exact-head migration/bootstrap/rollback comparison on the integrated candidate.
- Rust Platform CI run 46109085094 reports failures in E18 differential operations parity. The logged mismatches are Cache-Control comparisons for anonymous/foreign-tenant entitlement reads and writes, missing CSRF, insufficient member permissions, text/plain anonymous PUT, and suspended-organization denial. The operations parity gate is therefore not green.
- No exact-current-head route/migration/operations parity certificate was generated in this slice.

### ADR and release governance

ADR-015 remains Proposed; ADR-004 remains Accepted. The ADR-015 review says it is conditionally ready for phase-1 prototyping only and requires all route, security, performance, operational rollback and owner-signoff gates before transition. No ADR state or FastAPI runtime was changed.

## Environment limitation

The isolated VPS clone has no apps/api/.venv, uv, FastAPI/httpx/psycopg modules, or loopback PostGIS test listener on port 54330. The full database-backed tests/rust-parity/run.py and tests/rust-parity/e07_transfers.py were not run locally. Existing E07 Actions evidence is cited with its exact candidate SHA above; the updated Nginx smoke was run locally on the isolated clone. GitHub returned no workflow runs/status checks for the separate API-created commit 615345d.
