# Spec 064: remaining native API work — 2026-11-27

Audited candidate: `1e0a0b24ed478e482b465bb43e54779186c8f568` (PR #69), not production.
Baseline: `contracts/inventory.json` generated 2026-11-25 (364 live routes, including one WebSocket). Static source review of Axum router registrations and relevant handlers, not a runtime parity certificate. The following is a minimum remaining-work list; a complete runtime route/behavior inventory must still be regenerated. A registered handler is not proof of complete feature parity.

## Route gaps and retained boundaries

Paths below are relative to `/api/v1`. `/P` abbreviates `/organizations/{organization_id}/projects/{project_id}`; `/O` abbreviates `/organizations/{organization_id}`.

| Engine | Method and path | Next work |
|---|---|---|
| E01 | GET /auth/login/{provider} | OIDC state/nonce/PKCE and redirect parity |
| E01 | GET /auth/callback/{provider} | OIDC callback and session lifecycle parity |
| E01 | PUT /me/avatar | Avatar upload validation and storage parity |
| E01 | POST /O/invitations/email | Invitation delivery and tenant policy parity |
| E10 | POST /P/cad/documents/{document_id}/versions/{version}/export-georef-draft | CAD engine boundary |
| E10 | POST /P/cad/documents/{document_id}/versions/{version}/import-georef | CAD engine boundary |
| E10 | POST /P/cad/documents/{document_id}/versions/{version}/inspect-georef | CAD engine boundary |
| E10 | POST /P/cad/documents/{document_id}/versions/{version}/preview-native-import | CAD engine boundary |
| E10 | POST /P/cad/export | CAD export adapter |
| E10 | POST /P/cad/import | CAD import adapter |
| E10 | POST /P/cad/inspect | CAD inspection adapter |
| E12 | GET /O/external-sources/allowlist | External source authorization and SSRF policy |
| E12 | POST /O/external-sources/allowlist | External source authorization and SSRF policy |
| E12 | GET /P/external-sources | External source catalog |
| E12 | POST /P/external-sources | External source connection |
| E12 | POST /P/external-sources/{connection_id}/invalidate-cache | Scoped cache lifecycle |
| E12 | POST /P/external-sources/{connection_id}/preview | Bounded remote request |
| E12 | POST /P/external-sources/{connection_id}/revoke | Revocation parity |
| E12 | POST /P/external-sources/{connection_id}/viewport | Scoped external viewport |
| E12 | GET /P/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export | Connector transfer adapter |
| E12 | POST /P/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-preview | Connector transfer adapter |
| E12 | POST /P/integration-hub/adapters/{adapter_key}/layers/{layer_id}/sync-push | Connector transfer adapter |
| E09 | POST /P/raster/analyze | GDAL/COG compute boundary |
| E09 | POST /P/raster/layers | Raster ingestion |
| E09 | POST /P/raster/layers/{layer_id}/terrain-preview | Terrain processing |
| E09 | GET /P/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png | Scoped raster tile serving |
| E09 | POST /P/raster/reports | Raster report generation |
| E13 | POST /P/field/packages | Offline package creation |
| E13 | GET /P/field/packages/{package_id}/snapshot | Snapshot parity |
| E13 | POST /P/field/packages/{package_id}/sync | Offline conflict and replay parity |

These 30 baseline operations were not found in the corresponding mounted native router groups during this review. Some are deliberate legacy engine boundaries; distinguish a Rust-owned authenticated control plane calling an approved engine from a legacy Python-owned HTTP endpoint. Do not implement fake handlers or change gateway routing just to increase route counts.

Additional confirmed behavioral gap: E07 `apps/api-rust/src/transfers/mod.rs` explicitly returns `FORMAT_NOT_MIGRATED` for GeoPackage and ZIP Shapefile transfers. Existing route registration therefore overstates format parity. PR #63 documents six E15 file/document engine routes retained on Python; do not treat these as accidentally missing or complete native implementations.

## Gateway and release

`infra/rust-platform/hybrid-gateway.conf` defaults to FastAPI and routes only explicitly allowlisted candidate paths to Rust; bearer requests still use the legacy path except native tiles. Do not switch defaults, disable auth checks or retire legacy services until per-method, per-credential, tenant-isolation and failure-mode parity is verified on the exact merged commit. Use targeted differential PostGIS tests for each missing boundary, then run full integrated CI. Record accepted retained engines and explicit owner approval separately.

CI on the audited candidate was in progress; Rust Platform CI PR-triggered run was skipped by duplicate-run policy. Push-triggered Rust checks must be verified independently. This report does not mark Spec 064 tasks complete.
