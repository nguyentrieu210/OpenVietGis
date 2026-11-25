# Spec 059 acceptance ledger

| Reference | SHA or artifact |
| --- | --- |
| Main and live at the R058 baseline | `4e721a9f04ea933324da1d5ff4f433e40f75dbb9` |
| First draft PR revision | `348edf9afaa7ba4d39c9ef695c8a9d413c569ef4`, tree `473fee04a17155dbbd17bae647cabeabd6eda304` |
| Local candidate with Core/module fix and acceptance tests | `bd70796266551e739b8f7055f4cea73f17ea9526` |
| QGIS Connector 1.0.0 ZIP | SHA-256 `eb4a9302728269b0afc434e7964b1c88687bcab0c764af5dcc0eb9ca7765ab22` |
| Isolated rollback dump | SHA-256 `ff4278226aa9676899e5d2a352071968c095ecd202e1aa99c3d85672a573827b` |

[Draft PR #53](https://github.com/nguyentrieu210/OpenVietGis/pull/53) is the review and CI reference. Each Actions run applies to its recorded head SHA. The QGIS job applies to connector sources and the package build. Subsequent changes cover Core/module code, tests and evidence. The QGIS connector bytes are pinned by the ZIP checksum.

## Evidence scope

PostGIS tests used a disposable local published port bound to loopback, with `openvietgis_runtime` confirmed to lack superuser and BYPASSRLS. The scale host was x86-64 Debian with two vCPUs and approximately 3.8 GiB RAM. Datasets are synthetic. Chromium used SwiftShader. The Windows desktop test used QGIS 3.44.15, the actual OGR GeoPackage provider and an isolated profile, with simulated transport. Real browser visual comparisons used disposable local PostgreSQL/PostGIS and Keycloak with the provided development realm.

| Tasks | Result and reproducible scope |
| --- | --- |
| T001–T005 | Main/live pin, R031–R047 mapping, fixture and contract profiles are in the [baseline audit](baseline-and-contracts.md). The [first hosted CI](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45615255433) recorded 42 browser passes and seven comparison failures. The retained artifact SHA-256 is `ec702627eb0b28287bf44d84baaaa4ac64766ee3014d73397fe5ed6126c3e61a`. Actual diffs show the R058 palette change and font/line wrapping. Landing is 1,611 px high against 1,573 px expected. Local Chromium previously passed the seven tests and the typed query case. The hosted visual gate remains open. |
| T006–T012 | Browser measurements below cover all three sizes. Revision-pinned MVT benchmark and RLS tests exercised the active-project policy change. Regression coverage includes the bounded browser cache recovery and CRS controls. The R007 query workbench executes typed predicates and sort on the server under its 50,000-row profile, with keyset feature pages for large-layer access. |
| T013, T014, T016, T017 | Managed snapshot overlap and aliases, operation/source receipt identity, stale cursors and scoped recovery passed focused tests. Windows native smoke built and edited 8,000 points in 0.60 s, then applied a delta and replay in 17.4 ms. It checked dirty-layer/cursor preservation, actual GeoPackage reopen, explicit delete confirmation, bounded retry and editing while a background operation was sending. |
| T015, T018, T019 | 100 alternating writes passed across Web and managed HTTP test clients with PostGIS. Tests also cover expired credentials, denied scopes, retry/replay and the separate Windows smoke. An authorized Windows QGIS ↔ WebGIS LAN target and event-to-visible trace are still required. |
| T020–T022 | Core owns geometry and passes stable source/feature identity to the plugin. Chromium covers map and plugin selection, remote revision refresh, rename, styling, and a linked edit submitted to Core. The browser transport in this suite is synthetic. |
| T023–T025 | PostGIS exercised a 30,001-point survey source with Core keyset reads, a typed number proposal, approval and packet generation, evidence import/replay and survey reconciliation. A separate managed session received `DATA_LOCKED` for a coordinate edit after sign-off. Both Web and managed deletes are blocked when a building link is active, including after module disable. A free feature remains deletable. Approved number relocation across routes, stale relinks and retirement fail closed. An authorized retirement of an unnumbered building releases the GIS delete guard. Core can access its layers with the extension disabled and the same activation can be restored. Incompatible manifest routes fail closed. |
| T026–T027 | The reference-module/SDK evidence includes Land & Asset Registry. Engine-First and canonical smoke passed Core/CAD/Cesium mode changes, pinned CAD placement, linked selection, native tile streaming and revocation. Earlier real PostGIS suites verified linked CAD documents, 3D tilesets and data portability. |
| T028–T029 | Root CI and visual diff review are tracked per candidate SHA. Runtime dependency audits reported zero advisories across 75 JavaScript dependencies and 46 Python packages. The dependency manifests and license files were reviewed in the candidate diff. The first draft passed GitHub backend, QGIS and secrets jobs. Local credential surface scan was clean. Restricted-role API and QGIS contract checks passed 87 cases on the final Core migration. Independent review is recorded separately. |
| T030 | A candidate custom-format dump restored into a second database. The fixture contained 30,006 features, an activated module and a building. Downgrades to the R058 Core ACL policy and Số nhà module schema preserved the geometry/attribute digest, record counts and runtime role. Exact R058 source then passed GIS-access and official-runtime probes on the downgraded profile. |
| T031–T034 | This ledger tracks evidence and the release blockers. Independent review and approval of promotion remain pending. Production and post-promotion checks require a completed release gate. |

## API scale, PostGIS runtime role

Run `benchmark_spatial_http.py` through the guarded database wrapper. The deterministic grid fixture SHA-256 is `ab1f79791a17af702036dc78ec2ae7bdba1a13195e161867177d37f2147e9fc2`. The table gives p95 over 20 requests after three warmups. The HTTP benchmark uses in-process transport, with one active client and full tile responses.

| Points | Candidate tile p95 | Baseline tile p95 | Candidate viewport p95 | Candidate keyset page p95 |
| ---: | ---: | ---: | ---: | ---: |
| 10,000 | 42.026 ms | 741.931 ms | 141.623 ms | 96.101 ms |
| 100,000 | 153.002 ms | 6,265.762 ms | 50,000-row profile limit | 52.176 ms |
| 1,000,000 | 1,399.255 ms | HTTP 422 under statement budget | 50,000-row profile limit | 128.609 ms |

The first 1M tile took about 3.61 s. The 100k and 1M unclustered viewport requests return an explicit profile-cap error. The large-layer map uses vector tiles, with separate revision-pinned feature details and keyset pages.

## Browser scale over HTTP loopback

Build `@openvietgis/web-next` and run the viewport script through the guarded database wrapper. It serves the built app and real API over loopback and uses a temporary synthetic web session. Chromium renders with SwiftShader at 1440 × 900 and starts at zoom 14. External basemap DNS is disabled. Each row uses one cold open and 20 reloads. Memory is the page's JavaScript heap. Tile transfer includes the response headers and body for completed spatial requests, excluding the application, basemap and session metadata. The 10k run used Window Resource Timing. Tile runs count network requests issued by the map worker and allow measured tile cancellations followed by completed requests.

```bash
pnpm --filter @openvietgis/web-next build
PYTHONPATH="$PWD/apps/api:$PWD" uv run --project apps/api python -m tools.run_with_test_database \
  python specs/059-core-realtime-acceptance/benchmark_viewport_browser.py
```

| Points | Cold map data ready | Warm p50 | Warm p95 | Cold pages | Warm max pages | Cold spatial transfer | Warm p95 JS heap |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10,000 | 6.13 s | 3.80 s | 6.29 s | 10 | 0 | 2,719.48 KiB | 59.42 MiB |
| 100,000 | 4.14 s | 6.39 s | 12.35 s | 0 | 0 | 83.90 KiB | 43.77 MiB |
| 1,000,000 | 11.89 s | 5.31 s | 6.05 s | 0 | 0 | 91.95 KiB | 42.18 MiB |

The map made up to six completed tile requests per warm reload at p95. Warm tile response transfer p95 was 1.34 KiB at both scales. The runs used the two-vCPU QA VPS with concurrent validation activity. The warm reopen results exceed the proposed 2 s acceptance target. The production LAN profile and Web ↔ QGIS event-to-visible timing still require separate measurement.

## Release conditions

- T003 and T028 require hosted screenshot evidence and full exact-candidate CI.
- T015, T018 and T019 require the approved isolated Windows-to-WebGIS network run. The 2 s target applies to a declared LAN profile.
- T032 requires an independent reviewer and observed user acceptance. T033 requires explicit approval to promote. T034 follows promotion.
- The walkthrough used synthetic data. Use of private production survey records requires separate authorization.
