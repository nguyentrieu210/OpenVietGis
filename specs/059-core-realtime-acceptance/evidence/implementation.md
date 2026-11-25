# Spec 059 candidate verification

Product change: `341e67d`. Browser smoke fixtures and failed CI visual artifact retention: `a8efe0d`. Feature branches and source versions are pinned in the [baseline audit](baseline-and-contracts.md).

## Browser cache and revision gaps

| Case | Chromium with synthetic API |
| --- | --- |
| Cold open, two 100-feature layers | Two feature-page requests |
| Warm reload at unchanged revisions | Zero feature-page requests and two cache hits |
| Warm reload after one feature edit | Zero feature-page requests, one changeset history read and one feature detail read |
| Warm reload with expired snapshots | Two feature-page requests after aging the entries beyond the 24-hour TTL |
| Reload after entering restricted scope | Two feature-page requests. Full-layer cache is bypassed |
| Revision gap, one 21,000-row layer and one unchanged 100-row layer | Baseline read 20 pages for the changed layer. Candidate read 21 pages, preserved all 21,100 displayed rows and made zero unrelated feature-page requests |

The browser uses IndexedDB, real Chromium and a local built WebGIS. The synthetic API isolates the browser request-count checks. Native application-to-application timing and revoked-scope cross-tab behavior remain separate acceptance cases.

## Scoped backend and QGIS

- 26 focused tests passed with a real disposable PostGIS database. Includes spatial read pin locking, scoped feature authorization, RLS, native CRS support, managed QGIS sessions and recovery, and client-side snapshot/delta contracts.
- The managed API test exercises 100 alternating feature creations across Web and managed QGIS API clients. It asserts exact IDs per layer and alias, identical replay receipt, stale cursor recovery and credential revocation. Transport is FastAPI TestClient.
- R003 verified the 10,000-feature reference workload with real PostGIS and a restricted database role. Three geometries, native and projected reads, 100 measured requests per scenario after five warmups. Report timings are full API responses in a local in-process harness.
- Native Windows smoke on QGIS 3.44.15 passed. An 8,000-point QGIS layer build, snapshot and native edit took 1.15 s. Delta and replay took 14.5 ms in the same run. The OGR GeoPackage provider reopened offline edits, native Save journaled before network work, the dirty layer/cursor survived refresh, and edits made while sending were preserved. The transport tests are simulated.
- Candidate QGIS plugin package passed the repository checksum check. SHA-256 `6fd992d6453f49cb325f9facd4f6d0ead96d1befb34ebd6bfeee2f1ea1deff6a`. Live package identity is recorded separately in the baseline.
- R003 coordinate tests reran against PostGIS 3.7.0 and PROJ 9.9.0. The tests cover EPSG:4326 and 3857 and the published geographic VN-2000 and Quảng Ninh TM-3 control fixtures. See [R003 control evidence](../../003-spatial-catalog-crs/vn2000-control-evidence.md) for source pairs, operation-specific tolerances and geographic scope.

## Core and Số nhà

- Both Chromium smoke scripts passed. Core opened with layers, attribute table and point inspector and made zero Số nhà API or lazy chunk requests. Activation loaded the module-specific chunk and API, enabled territory filtering and supported navigation from a search result to a Core selection. Returning to Core and disabling the module preserved the GIS layers.
- QGIS-style WebGIS workspace smoke covered layer selection independent of visibility, attribute filtering, symbology, ordering, resizing, plugin group visibility and the mobile full-map layout.
- The mocked compatibility and scope responses were aligned with the current contract. Production UI controls and business rules were preserved.
- Architecture, docs, accepted runtime scope, API contract checks, frontend typecheck and the WebGIS build passed. Five focused Node unit tests and 84 client SDK tests passed.

## Query paging, module regression and restore

- The attribute query HTTP boundary now uses the same limit and offset definitions as the query domain. Three real PostGIS query-integration tests passed. A revision-bound continuation accepts offset 50,000 and returns an empty page beyond the result set. Offset 50,001 is rejected. The WebGIS remote-table filter still applies to the current page.
- A separate 41-test PostGIS run passed for reference modules, module portability, Số nhà runtime and survey sign-off, CAD links/documents, 3D tilesets, recovery tooling and GIS scope. It included the two independent reference module contracts for Số nhà and Land & Asset Registry.
- The tile scope-change regression warms a tile, narrows the same account to the other feature, then verifies the old ETag cannot return a cache hit. The allowed tile remains readable under `private, no-store` and the restricted tile is empty.
- Dump and restore in the disposable PostGIS profile preserved migration `0042_qgis_pairing_scope`, all 27 feature IDs and geometry fingerprints, nine house records and the restricted runtime role. Dump SHA-256 `7cc282c1b3c69e4b7b5d8decb3a75f7b598b3e53c4dcce0448a55939ef5a6942`. Full release rollback rehearsal remains open.
- Local real PostGIS/Keycloak visual reproduction passed five of seven existing baselines, including the landing page and all four map-workspace cases. The login diff captured the provider-loading state, and the dashboard authentication step timed out after navigating to Keycloak. The focused login visual test now waits for the sign-in link and passed with a three-second synthetic provider delay. Hosted visual acceptance still needs the exact candidate and the host's actual screenshot differences.

## Release constraints

The exact baseline CI validated backend, secret scan, QGIS packaging and Linux QGIS desktop smoke. Root frontend validation recorded 42 browser passes and seven screenshot differences. This remains a release blocker while the candidate comparison is open.

The release gate also requires the actual Windows QGIS ↔ WebGIS session over an authorized isolated test network, offline/reconnect and approval cases, and a measured LAN p95. Those results are independent of the passing native smoke and managed API tests. Live promotion requires the explicit approval in T033.

## Scale baseline

The R031 indexed SQL microbenchmark completed all three sizes on disposable PostGIS 3.7.0. Each case used 30 timed requests after five warmups. The table reports p95 in milliseconds.

| Synthetic points | Cluster tile z14 | Detail tile z17 | Cluster tile bytes | Detail tile bytes |
| ---: | ---: | ---: | ---: | ---: |
| 10,000 | 20.521 | 2.565 | 3,240 | 102 |
| 100,000 | 74.045 | 2.761 | 12,964 | 523 |
| 1,000,000 | 219.429 | 7.303 | 18,537 | 5,417 |

Scope is SQL and local tile bytes. The R003 10k HTTP integration reported a p95 ranging from 79.736 to 297.486 ms for native 100-row pages and 596.752 to 1,312.298 ms for 100-row pages transformed to EPSG:3857, across points, lines and polygons.

For HTTP scale measurements on a fresh synthetic test database, use the dataset generator in this spec. Run the migrations via the guarded test wrapper. The benchmark measures one client's first request and p50/p95 after warmup for a fixed viewport, a 100-row feature page and an MVT cluster tile. Its CPU and peak RSS refer to the Python process. Browser rendering, network latency and database memory each require separate measurements.

```bash
PYTHONPATH="$PWD/apps/api:$PWD" uv run --project apps/api python -m tools.run_with_test_database \
  python specs/059-core-realtime-acceptance/benchmark_spatial_http.py
```

## Real HTTP scale results through the restricted role

The benchmark authored a deterministic EPSG:4326 synthetic grid over three sizes. The fixture SQL digest is `ab1f79791a17af702036dc78ec2ae7bdba1a13195e161867177d37f2147e9fc2`. Each successful request used the captured content and schema pins. The measured API response timings use in-process HTTP test transport and real PostGIS. The repeated tile requests ask for a complete response.

| Synthetic rows | Viewport p95 | 100-row feature page p95 | Cluster tile z14 p95 |
| ---: | ---: | ---: | ---: |
| 10,000 | 769.767 ms, 25,243 bytes | 58.987 ms, 28,376 bytes | 741.931 ms, 4,061 bytes |
| 100,000 | 422 at the 50,000-row profile limit | 51.768 ms, 28,376 bytes | 6,265.762 ms, 14,048 bytes |
| 1,000,000 | 422 at the 50,000-row profile limit | 75.019 ms, 28,376 bytes | HTTP 422 from the spatial execution or statement-budget guard; no p95 |

The million-row page was measured separately against the same disposable fixture. Both endpoint measurements kept the restricted runtime role. The 100k z14 cluster plan under `openvietgis_runtime` used a sequential scan on `spatial.reference_features`. The comparison plan for the same spatial selection under the test-database owner used the GiST index `ix_reference_features_geometry`. The plan gap is an open performance issue. The SQL-only p95 and the API p95 describe different scopes.

## Verification after the R058 main sync

Source candidate `f3ce61a4535781ea4bec345720cac09a461c70e3` includes `4e721a9`. Frontend typecheck and build passed. All three Chromium smoke scripts passed again on that build: cache expiry and revision gap recovery, Core/plugin lifecycle, and QGIS-style dock and layer interactions. Five focused sync/performance Node tests and three CRS display tests passed. The latest sync brought no backend or native connector source changes. Hosted full candidate CI, actual desktop/WebGIS LAN latency and release approval remain open.
