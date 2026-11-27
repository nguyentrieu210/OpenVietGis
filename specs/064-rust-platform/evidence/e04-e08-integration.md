# E04–E08 integration evidence

## Scope and provenance

This work starts from `064-integration` at `6e3b74c`. It includes the E04 request validation and query benchmark follow-up (`d98db54`), E05 bounded deterministic MVT encoding (`ca43a14`), and E07 complete multipart-boundary validation and transfer parity (`2701365`). No schema or data migration was added.

| Engine | Staging implementation | Qualification target | Open work |
| --- | --- | --- | --- |
| E04 | Rust typed queries and saved-query CRUD | `tests/rust-parity/e04_queries.py`, `rust-e04-benchmark.yml` | Comparative evidence for the combined ref, service principals on shared authentication, consumer/rollback exercise. |
| E05 | Rust viewport, display extent and EPSG:4326 MVT candidate. Legacy 3D tilesets and the existing `/pro/tiles` endpoint stay on FastAPI. | `e05_render.py` covers response and tile authorization behavior. `rust-e05-benchmark.yml` measures viewport at 1k/10k/30k and 1/10/20 clients. | Combined-ref results, trusted VN-2000 proof, 100k/1M tests and edit-to-visible timing. |
| E06 | FastAPI cartography, layout designer, and PNG/PDF exports under the hybrid owner decision. | `e06-e08-retention.yml` uses existing format, revision-pin and layout tests under the restricted PostGIS role. | Staging route and browser consumer check with the hybrid gateway. |
| E07 | Rust candidate for GeoJSON. FastAPI handles the public transfer route family until format-aware routing and state handoff are qualified, including GeoPackage, ZIP Shapefile, and retained external adapters. | `e07_transfers.py` checks GeoJSON transfer/ACL/replay on both runtimes and verifies retained format roundtrips on FastAPI. | Fresh PostGIS parity, representative large-transfer metrics and end-to-end gateway routing. |
| E08 | FastAPI algorithm registry, jobs and model DAGs. The professional geometry, schema and topology endpoints are qualified separately under E03. | `e06-e08-retention.yml` covers stale inputs, durability, idempotency, cancellation where covered by the existing suite, and DAG behavior. | Staging consumer and recovery exercise. |

Frozen E06/E08 operation inventories capture the initial proposal. Current routing and acceptance follow the owner’s hybrid decision in `engine-scope.md`.

## E05 measurement

Both services use the same seeded loopback PostGIS and viewport input. The readiness probe and three first requests do not clear shared database or OS caches. The report contains statuses, individual measurements, p50/p95/p99 and app RSS per service. The 70% concurrent p95 target is evaluated only for successful comparable responses, with response parity. The known FastAPI 30k HTTP 503 ceiling is reported without a latency ratio and the Rust candidate must still succeed. A green job is scoped to the measured scenarios.

## Large-source MVT qualification

The native MVT route uses the same authorization, source checks and revision pins as JSON viewport. MVT applies its own per-tile budget without counting the actor's entire visible source. JSON viewport and display extent keep their 50k source ceiling. The SQL includes an explicit native bbox predicate for the GiST path on the approved database profile. The exact geometry check remains in place. Statement timeout stays at 2 seconds, including when the plan still scans a broad tile.

The E05 workflow runs the JSON comparison and the MVT scale benchmark as independent steps against one commit-pinned build. `mvt-scale.json` compares successful local z17 MVT requests at 1/10/20 clients on real 30k, 100k and 1M point sources. It records latency, response size, tile fingerprints and per-process RSS. Single world-tile probes require the exact first 10,000 generated feature IDs, the truncation header and the 4 MiB response bound. Separate JSON probes require rejection above 50k. The report checks FastAPI's local feature IDs against the Rust selection and records a latency ratio. Native MVT and the legacy professional route differ in buffer selection and encoding metadata, so the ratio is separate from the JSON viewport performance gate.

This benchmark covers bounded tile delivery. SC-004 still requires a replayable 20-client edit-to-visible measurement and reconnect verification. Fresh GitHub Actions artifacts are needed for all performance claims.

## Verification at handoff

Python syntax and Git whitespace checks passed. The handoff requires Rust builds, PostGIS differential tests and comparative benchmarks to run in GitHub Actions. Publish and qualify the combined ref before widening staging traffic. Earlier engine-specific artifacts do not establish combined coverage.
