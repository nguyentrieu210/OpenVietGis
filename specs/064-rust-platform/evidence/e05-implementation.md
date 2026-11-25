# E05 Implementation Evidence — Map rendering, viewport, vector tiles

- **Branch**: `064-e05` (from `064-rust-platform` @ `0757c67`)
- **Rust code**: `crates/ovg-storage/src/viewport.rs` (SQL + wire), `apps/api-rust/src/map_render.rs` (routes, error envelopes)
- **Shared files touched**: `apps/api-rust/src/lib.rs` (+1 module line), `apps/api-rust/src/routes.rs` (+1 merge line), `crates/ovg-storage/src/lib.rs` (+1 module line), `crates/ovg-storage/src/catalog.rs` (visibility only: `ProjectRead` fields, `get_source_record`, `SourceRecord.wire`, `db_error` become `pub(crate)`)
- **Parity cases**: `tests/rust-parity/e05_render.py`, called once from `tests/rust-parity/run.py::compare`
- **Python oracle**: `apps/api/openvietgis_api/{adapters/http/map_viewport.py, adapters/persistence/map_viewport.py, domains/maps/viewport.py}`

## Implemented operations

| Operation | Source | Status |
|---|---|---|
| `GET .../layers/{layer_id}/viewport` | E02 `map_viewport` route listed under E05 `cross_engine_render_and_tile_routes` | migrated (parity target) |
| `GET .../layers/{layer_id}/display-extent` | same | migrated (parity target) |
| `GET .../layers/{layer_id}/tiles/{z}/{x}/{y}.mvt` | new Rust-native API, documented in `contracts/engines/e05.json#rust_native_operations` | new, no oracle |

The 7 operations of the E05 inventory proper (3D Tiles: list/create/generate tilesets, feature, feature-by-id, `tileset.json`, tile URI) are **not** implemented: 0/7. The slice above is the 2D render path the map client uses on every pan/zoom.

### Behaviour kept from FastAPI

- Same path, query parameters (`content_revision`/`schema_revision` `^[a-f0-9]{64}$`, finite `west/south/east/north`, `limit` 1..1000 default 500), JSON shape (`context`, `items`, `complete`, `limitation` / `context`, `state`, `bounds`) and `Cache-Control: no-store` on success; error responses carry no Cache-Control, like the FastAPI exception handlers.
- Authorization: browser session (`HumanSession`), then `ProjectRead::begin(..., LayerAcl)` — the same membership, grant, organization/project status, GIS-scope checks as the catalog routes — inside one tenant transaction bound with `set_config(..., true)` under `openvietgis_runtime`. Feature rows are filtered by the restrictive `reference_features_gis_acl` policy; `openvietgis.acl_internal` is never set.
- Check order and envelopes: layer (404 `resource not found`) → source (404) → `SOURCE_NOT_READY` 409 → `STALE_READ_PIN` 409 → `UNSUPPORTED_TRANSFORMATION` 422 (no approved edge, non-renderable kind/family/dimensions, count > 50 000) → `TRANSFORMATION_OUT_OF_AREA` 422 (provenance / any row outside the display domain) → `MAP_READ_UNAVAILABLE` 503 (count disagreement, SQL failure, 2000 ms statement timeout, envelope over 2 MiB). Invalid input → the sanitized 422 `invalid request payload` envelope.
- GIS-scoped actors: expected count is the RLS-visible count, exactly like `scoped(...)` in Python, so a restricted viewer sees only permitted features and no fail-closed false alarm.
- The viewport response is a strictly ordered (`COLLATE "C"`) prefix bounded by feature limit, 20 000 positions and the 2 MiB whole-envelope budget (`build_viewport_response` port); geometries are decoded from NDR WKB without rounding and rechecked (XY domain, display latitude, no longitude wrap, exact position count).
- EPSG:3857 sources go through the same fixed `ST_InverseTransformPipeline` step and post-transform validity checks as Python.

### Intentional divergences

| # | Python | Rust | Why |
|---|---|---|---|
| D1 | `native` CTE `ORDER BY feature_id LIMIT 10001` while the ceiling is `MAX_RENDER_SOURCE_FEATURES = 50_000`; any layer with more than 10 001 features returns 503 (baseline: 30k probe → 503) | bound is `50_001`; only a snapshot above 50 000 rows fails the count check | Python bug, not replicated (spec instruction). Parity fixtures stay at 1k so the oracle is not hit by it. |
| D2 | Native rows re-run `ST_Zmflag`, `ST_NPoints BETWEEN 1 AND 4096`, `ST_IsEmpty`, `ST_IsValid` per request | Native rows check only pins, `geometry_state='present'`, SRID and type; topology checks run only after a transformation | `ck_feature_geometry_state` guarantees those predicates for every present row. Same outcomes, less work. |
| D3 | Trusted Quang Ninh fixtures (EPSG:4756/5899 edges) verified by re-materializing the fixture manifest digest | Every edge with a fixture requirement fails closed with `TRANSFORMATION_OUT_OF_AREA` | The fixture registry is not native yet (`shortcut:` comment in `viewport.rs`). Matches Python for every non-trusted fixture (incl. the seeded `vn2000_5899_1k`), diverges (422 vs 200) only for genuine trusted fixtures. |
| D4 | SQL errors inside authorization (`prepare_scope`) also map to 503 | authorization DB errors map to 500 (same as the catalog routes) | Only on database failure. |
| D5 | Byte budget uses Python `float.__repr__` (`1e-08`) | serde_json/ryu (`1e-8`) | At most a few bytes on transformed contexts; only matters for a response within bytes of the 2 MiB limit. |
| D6 | Bearer service tokens accepted | Browser session only (bearer → 401) | Same limitation as all current Rust routes (E01 service principals not native yet). |

### Vector tiles (new)

`ST_TileEnvelope(z, x, y)` in EPSG:3857; the tile area plus 64/4096 margin is converted to EPSG:4326 with the fixed analytic Web Mercator pipeline, rows are filtered by pins, `geometry_state='present'`, SRID 4326 and `ST_Intersects`, clipped with `ST_ClipByBox2D` (so the projection is always finite), projected with `ST_TransformPipeline` and encoded by `ST_AsMVTGeom`/`ST_AsMVT` (layer `features`, extent 4096, buffer 64, property `feature_id`). Zoom 0–22, `x, y < 2^z`, at most 10 000 features per tile in `feature_id` order with `x-openvietgis-tile-complete: false` when capped. `Content-Type: application/vnd.mapbox-vector-tile`, `Cache-Control: private, no-store` (no shared cache, so ACL revocation is immediate). Only native EPSG:4326 sources (others: 422 `UNSUPPORTED_TRANSFORMATION`). Unlike the viewport, a tile does not validate the whole source per request (bounded per-tile work; the table CHECK still guarantees row validity).

## Index finding (blocks the GIST bbox path under RLS)

`ix_reference_features_geometry` (GIST, `WHERE geometry_state='present'`) is **not usable by the runtime role**. Checked read-only on VPS1 `openvietgis_test` (PostGIS 3.7.0): as `openvietgis_runtime` with `enable_seqscan=off` the planner uses only `reference_features_pkey` (org, project, source) and filters `ST_Intersects` per row; as the owner role the same query uses the GIST index. Cause: PostGIS `geometry_overlaps` (`&&`) and `st_intersects` are not `LEAKPROOF`, so PostgreSQL may not evaluate them as index conditions before the RLS policy quals (`reference_features_project_scope`, restrictive `reference_features_gis_acl`). Making them usable needs a DBA decision (LEAKPROOF on PostGIS operators is a superuser, database-wide change) or a schema change; neither belongs in a request path. The Rust queries keep the index-compatible predicate (`geometry_state='present'` + `ST_Intersects`), so they will pick the index up if that changes. Consequence: viewport and tile cost is one scan of the source's rows (pkey range) per request, not a bbox probe.

## CI / test results

- Rust unit tests added (`crates/ovg-storage/src/viewport.rs`): bounds validation, pin pattern, WKB decode + display checks, feature/position limits and stable order.
- GitHub Actions `Rust Platform CI` run 45668460716 on `064-e05` @ `ff39030` (https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45668460716): fmt, architecture isolation, clippy `-D warnings`, `cargo test --workspace` and the no-default-features test **pass**. `Cargo Audit` fails identically on the base branch (`7f087df`), not caused by E05.
- PostGIS parity job (FastAPI oracle vs Rust on real PostGIS, same run): **all 31 E05 cases pass**; totals `HTTP parity 90 passed, 11 failed`, the 11 failures being the pre-existing E02 `/layers/{id}/features` cases (route not implemented in Rust yet, same on base). `e05_render.py` covers 23 FastAPI-vs-Rust differential cases (status, JSON body, Cache-Control) (viewport bbox/world/limit/empty, extent, stale pins, 6 invalid inputs, unauthenticated, tenant B 404 ×2, foreign and missing layer, manual source 422, VN-2000 untrusted 422, GIS-scoped viewport only `poly-000001`, GIS-scoped extent, GIS-hidden layer 404) and 8 Rust-only tile checks (populated, GIS-scoped no leak, empty tile, tenant B 404, stale pin 409, x out of range 422, z 23 422, unauthenticated 401).

## Benchmark

Not run: the comparative run needs the Rust binary against a seeded 1k/10k/30k PostGIS; builds now happen only in CI and the benchmark job is not wired yet (`benchmark_runner.py --target-url` also runs every non-viewport scenario, most of which Rust does not serve yet). Targets from `baseline-benchmark.md`: viewport p95 at c=20 ≤ 70 % of Python (`viewport_bbox_1k_polygon` 3482 ms, `viewport_bbox_10k_point` 4626 ms with 95 % errors), error rate ≤ Python, app RSS ≤ 1 GiB. Expected effect, unmeasured: no per-row topology recheck and no Python WKB/JSON loop; still a full per-source scan because of the RLS/LEAKPROOF finding.

## Remaining E05 gaps

1. 3D Tiles inventory (7 operations: tileset catalog, upload, generation, feature lookup, manifest and tile streaming) — not started.
2. Comparative benchmark (E05-T04) and parity fixtures above 10k features (the Python oracle 503s there, D1).
3. Trusted VN-2000 fixture provenance (D3) — needs the native fixture registry (E02).
4. GIST usage under RLS (index finding) — DBA/schema decision.
5. Tiles for projected sources, tile caching with revision-keyed invalidation and ACL revocation, PMTiles/Martin, offline packages.
6. Symbology, labels and layout belong to E06.


## Agent 04 follow-up — MVT resource bounds (2026-11-26)

The tile query now ranks by C-collated feature ID before projection/encoding and
limits the prefix to 10,000 features and 200,000 input positions. The response
fails closed with MAP_READ_UNAVAILABLE if the encoded MVT exceeds 4 MiB. The
completeness header reports prefix truncation; normal MVT clipping/quantization
may still discard geometry at a coarse zoom. Ordered aggregation makes repeated
pinned tile encoding deterministic. No changes to ACL, RLS, revision pins,
cache policy, database schema or production deployment.

Direct read-only PostGIS SQL probe using the revised query, replacing the
reference table with 101 synthetic segmentized WGS84 lines (2,237 positions
each), same CRS pipeline and tile encoder, default 200k position budget:

| Tile | Candidate limit | Candidates | Included | Bytes |
|---|---:|---:|---:|---:|
| 8/203/112 | 10,000 | 101 | 89 | 113684 |
| 8/203/112 | 10 (test limit) | 11 | 10 | 12800 |
| 8/0/0 | 10,000 | 0 | 0 | 0 |
| 0/0/0 | 10,000 | 101 | 89 | 2878 |

SQL ran in a READ ONLY transaction with rollback on the existing isolated
PostGIS test container. Python parity test syntax and git diff checks pass.
Two additional HTTP parity cases cover repeated pinned tiles and world zoom;
they are not yet executed. The direct SQL probe does not validate HTTP, RLS,
Rust compilation or comparative performance. VPS disk availability was
approximately 2 GiB and swap pressure high, so no local Rust build was started.
E05-T02 through T05 remain open. No new CI run or full benchmark claimed.
