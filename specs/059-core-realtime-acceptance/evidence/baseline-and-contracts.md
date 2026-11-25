# Spec 059 baseline and contract audit

## Pinned baseline

| Surface | Observation |
| --- | --- |
| GitHub main | `5cdb16ccb567121095acd691c5de6481e4963ded` from `git ls-remote` |
| Spec start | `ccdb23e` on `spec/059-core-realtime-acceptance` |
| Live WebGIS | `/rewrite/RELEASE_COMMIT` matches main. HTTP 200 on `/rewrite/?live=1`, SHA-256 of served HTML `efe2485383210982d5c34acb71ee6ba9ec4077f97ae0b35753995ef4d9b39db0` |
| Live API | Public proxy routes to the release directory pinned by the same SHA. The dependency probe reports API alive, database and artifact store OK, PostgreSQL 18.7 and PostGIS 3.7.0 |
| Live QGIS ZIP | 1.0.0, minimum QGIS 3.34, served SHA-256 `d45c440c3e0830ea2a864e3cf831ae39a00ac1b71dba03b742e7f1a28ed65d39`, verified against the published sidecar and release file |
| Local verification | Debian 13 VPS, 2 vCPU, 3.8 GiB RAM, Node 26.13.1, pnpm 12.21.0, Chromium Playwright 1.65.1, QGIS 3.44.15 on Windows |
| Isolated real DB | PostGIS image `sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262`, disposable `openvietgis_test` on a loopback-only ephemeral port. Owner for seeding and migrations, `openvietgis_runtime` for application integration |
| Real browser identity profile | Disposable Keycloak 26.8.2 image `sha256:7e66a96e4739239070aeaf72e4c5decf0f7f682cfc725b9f7ea2b336eaba38b3`. Use separate API, web and OIDC loopback ports with the repo realm and `configure_keycloak_e2e_redirect.py` |

Live observations were read-only on 2026-11-25 ICT. Test traffic uses synthetic fixtures. The VPS profile measures local loopback and shares host capacity with other services. Run one heavy workload at a time. Windows desktop-to-server LAN acceptance needs an authorized reachable test API and a separately declared network profile.

## Baseline refresh during implementation

Main advanced to `4e721a9f04ea933324da1d5ff4f433e40f75dbb9` via R058. The public release metadata advertised the same SHA, the dependency probes were ready, and the served WebGIS HTML SHA-256 was `13bc6e77b73f60480c56524ea1d80088b12558bb382dcde48950a1bf5e96da0e`. The QGIS ZIP checksum remained `d45c440c3e0830ea2a864e3cf831ae39a00ac1b71dba03b742e7f1a28ed65d39`.

Spec 059 synchronized this ref in `f3ce61a`. The upstream change set affected frontend code/styles and introduced no API or native connector source changes. [R058 main CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45609088640) was in progress during the refresh. Its [QGIS CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45609088597) passed.

## CI and visual baseline

- [Exact main CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45606234585) and [QGIS package and Linux native smoke](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45606234623). Backend, secrets, QGIS package and QGIS Linux desktop smoke passed. Root validation reported 42 browser passes and the same seven screenshot failures as the previous main.
- [Previous main CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45601448759/job/139161113174) at `9f31293006c0ab8b09b654bab39e575f6b89509c` lists 42 Playwright passes and seven failed screenshot cases. The first mismatch in each case was landing (110,779 pixels and 38 px taller), login (49,922), dashboard (38,366), empty map (20,491), populated map (30,967), closed technical drawer (30,783), and dirty map state (30,886). The remote artifacts contain the scan report; screenshot files must be reproduced and inspected before approving a baseline update.

## Existing contracts and open verification

| Interface | Current boundary | Spec 059 evidence required |
| --- | --- | --- |
| PostGIS and feature pages | `spatial.py`, `spatial_repository.py` and the typed client SDK use project/layer routes with content and schema pins. Keyset paging uses `after_feature_id`. `readSpatialFeatures` forwards the limit, cursor, geometry flag and response CRS. | T006–T010. Measure complete and partial snapshots, the revision-gap recovery and attribute query parity. The remote attribute table currently labels search as current-page filtering. |
| Indexed vector tiles | `professional.py` filters by stored CRS under RLS. EPSG:3857 has no render transform and EPSG:4326 transforms matching rows to tile CRS. ETags are actor, credential, tenant, layer and revision bound. Restricted tile responses are private and no-store. | T007/T012. Review actual execution plans at scale and restricted tile behavior. |
| Browser cache | IndexedDB key is account, organization, project and layer. Reuse follows fresh permission/layer metadata and matching source, content/schema and layer revisions. Complete rows are capped at 20k and 24 MiB per layer, 96 MiB overall, with 24-hour TTL. Restricted mode bypasses this cache. | T008/T009/T012. Exercise scope change, logout, stale pins, partial snapshot and single-layer recovery. |
| Web changes and QGIS managed session | Web app uses changesets, a project event wake-up, revision chains and a polling fallback. QGIS uses the managed bootstrap, delta, receipt and session APIs with cursor and permission epoch. Managed bootstrap/delta recheck the captured cursor/epoch before returning. | T013–T019. Use a common fixture for replay, concurrent commits, alias layers, offline reconnect and latency. |
| Permissions and CRS | HTTP scope, project GIS allowlist and restrictive feature RLS guard tenant data. R003 references include geographic VN-2000 controls and Quảng Ninh TM-3 points. Layer paging sends an explicit display CRS. | T011/T012. Record observed controls and tolerance per CRS, actual restricted DB role, and negative permission results. |
| Core and official modules | `official-module-registry.ts` loads compatible activated manifests dynamically. `studio-contract.ts` supplies selection, rows, editing callbacks and revisions. Số nhà persists stable source/feature links in its module schema. | T020–T026. Validate the linked geometry and approval invariants on activate, edit, rename, delete, disable and re-enable. Check a second reference module against the documented interface. |

## WebGIS and QGIS realtime contracts

| Operation | WebGIS | QGIS managed connector |
| --- | --- | --- |
| Open | Layer metadata and pinned feature pages or tiles. Local snapshots require fresh permission and matching revisions. | Create a managed session bound to authorized project/device. Bootstrap layer data under revision, cursor and permission-epoch pins. |
| Notice | Project-level `spatial_change` wake-up and polling. | Session-specific event wake-up and durable delta feed. |
| Apply remote change | Resolve a bounded committed ChangeSet history and read current details, or reload the affected layer if the chain is incomplete. | Apply ordered managed delta and protect dirty local layers. Cursor/epoch is validated with the delta. |
| Save and recover | Commit through the same source ChangeSet path, with a stable operation ID and receipt. | Journal a pending commit, inspect its receipt on uncertain delivery, replay only when permitted, and preserve the offline draft across restart. |
| Access change | Recheck effective permissions and scoped access before reusing a snapshot. | Recheck the project/binding/session relationship and permission epoch. The client treats a reset or denial as a distinct outcome. |

The browser SDK exposes managed-session management and the source ChangeSet API. Native QGIS has its own managed snapshot, delta and receipt adapter. The API integration test exercises both write paths against one server ledger.

## Core and module compatibility boundaries

| Path | Supported contract | Residual verification |
| --- | --- | --- |
| Official WebGIS module activation | Manifest key, exact version and 1.x compatibility, server activation status, project scope and lazy loading | Run incompatible manifest and restricted user cases on the exact candidate. |
| Module ↔ map selection | Stable source/feature keys via `StudioHost.select` and `goto`. Geometry is read from Core rows or resolved through linked feature APIs. | Check alias layers, map-to-business handoff and reverse navigation after rename, style and QGIS edits. |
| Write and approval | Geometry edits use the Core ChangeSet API. Official-module APIs own route/building link changes and business decisions. | Verify approved records through concurrent external edits and deletion or relink guards. |
| Partner SDK | `openvietgis-partner.js`, manifest compatibility and documented project permissions. | Approval, webhook and production publisher tests retain their R039 scope. |
| Property query | Core uses revision-pinned keyset feature reads. The typed query workbench has revision-bound filters, deterministic sorting and bounded offset pagination independent of the remote attribute table. Text filters are literal. | Connect dataset-level filtering and sorting in the WebGIS remote table through an explicitly supported contract. |

## Upstream traceability

| Upstream | Spec 059 coverage and residual scope |
| --- | --- |
| R031, R032, R033 | T001–T005 trace baseline, real platform parity and owner-approved promotion through T027–T034. Legacy surface retirement needs a separate approved parity decision. |
| R034, R035, R036 | T007–T014, T016–T019, T026 cover CRS, snapshot pins, SDK and revocation. Full public OpenAPI coverage and end-to-end MCP permission acceptance remain separate. |
| R037, R038 | T003, T022 and T027–T028 cover visual review and supported GIS/CAD/3D selection or portability. Broader touch, screen reader and token migration work remains in R038. |
| R039, R040 | T013–T019 and T020–T026 cover managed QGIS and the official-module path. Marketplace publication, remote SDK and webhook acceptance remain in R039. |
| R041, R042, R043, R044 | T003, T006–T012, T019 and T027–T031 cover checks on the current application. Real agent OAuth/consent flows, ticket restart history and production-size CAD/3D profiling keep their original scope. |
| R045, R046, R047 | T027–T034 cover recovery, exact candidate CI, review and approval gates. Signed production images and independent governance approvals remain tracked by their originating specs. |

## Running the targeted checks

1. Use the locked Node and Python versions from CI. Install using `pnpm install --frozen-lockfile` and `uv sync --project apps/api --frozen`.
2. Use the repository test Compose services on a dedicated workstation. On a shared VPS, provision fresh containers with loopback-only ephemeral ports and the known role-init SQL and realm files. Override `OPENVIETGIS_TEST_DATABASE_URL` and `OPENVIETGIS_E2E_{API,WEB,OIDC}_PORT`. Run the OIDC callback setup only on the disposable realm.
3. Run integration or performance tests through `uv run --project apps/api python -m tools.run_with_test_database python -m pytest`. For the visual cases use the same wrapper with `node node_modules/@playwright/test/cli.js test tests/e2e/app-shell-visual.spec.ts tests/e2e/map-workspace-visual.spec.ts --output <scratch>`.
4. Run the Chromium cached reload test using a fresh `@openvietgis/web-next` build at `/rewrite/` and `OVG_TEST_URL` pointing to a loopback static server. `node apps/web-next/spatial-cache-smoke.mjs` asserts actual browser feature-page counts against a synthetic API.
5. For native Windows smoke, run `tools/smoke_qgis_native.py` with the installed QGIS Python, an empty `OVG_QGIS_QA_PROFILE` and `QT_QPA_PLATFORM=offscreen`. Scope is native Qt and the OGR provider with simulated transport.
6. Use the R031 SQL scale benchmark in a separate `openvietgis_scale_test` database with PostGIS available. Captured SQL tile timings have a distinct scope from real HTTP and Chromium.
