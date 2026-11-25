# Spec 064 Rust Platform Baseline & Comparative Benchmark Suite

This directory contains the reproducible synthetic GIS dataset generator and HTTP benchmark harness pinned for **Spec 064 (`T003` Baseline Benchmarks & `T020` Comparative Benchmarks)**.

The same runner, deterministic dataset seed, multi-tenant fixtures, and HTTP workload run unmodified against **both**:
1. The **Python/FastAPI reference backend** (`ADR-004`) — baseline recorded in `specs/064-rust-platform/evidence/baseline-benchmark.{json,md}`.
2. The **Rust/Axum candidate backend** (`ADR-015`, `apps/api-rust`) — comparative evaluation via `--target-url` against the exact same PostgreSQL/PostGIS database and dataset.

---

## Deterministic Synthetic GIS Dataset

Seeded with `--seed 20261125` and deterministic `UUIDv5` identifiers across two isolated tenant organizations (`bench-org-alpha`, `bench-org-beta`) to verify multi-tenant RLS isolation:

| Layer Key | CRS | Geometry Family | Default Tier Count | Purpose |
|---|---|---|---|---|
| `bench_4326_1k` | `EPSG:4326` (WGS 84) | `Polygon` | `1,000` | Bounded viewport bbox query, polygon serialization, GeoJSON export workflow |
| `bench_4326_10k` | `EPSG:4326` (WGS 84) | `Point` | `10,000` | Mid-scale bounded viewport bbox query (at current Python `map_viewport` ceiling) |
| `bench_4326_30k` | `EPSG:4326` (WGS 84) | `Point` | `30,000` | Large-scale spatial bbox query (`/query`), paginated features (`/features`), and viewport ceiling probe (`/viewport`) |
| `bench_vn2000_5899_1k` | `EPSG:5899` (VN-2000 / TM-3 107-45) | `Point` | `1,000` | Real-time PostGIS VN-2000 -> WGS 84 (`response_crs=4326`) reprojection & viewport benchmark |
| `bench_vn2000_3405_1k` | `EPSG:3405` (VN-2000 / UTM 48N) | `Point` | `1,000` | Synthetic EPSG:3405 PostGIS geometry storage fixture (awaiting `T010` CRS control-point activation in `spatial.valid_extent`) |
| `bench_edit_4326` | `EPSG:4326` (WGS 84) | `Point` | `50` (base) | Mutable manual layer for create/update changesets with revision precondition & `409 Conflict` testing |

Optional larger scale tiers (`100k`, `1m`) are supported via `--scale 1k,10k,30k,100k` or `--scale 1k,10k,30k,100k,1m` using PostgreSQL streaming `COPY` for fast deterministic seeding.

---

## Measured Workload Scenarios

Each scenario runs **cold** (first uncached requests after startup) and **warm** across concurrency levels **`1`, `10`, and `20`**, recording **p50, p95, p99, min, max, mean latency (ms)**, **throughput (req/s)**, **error rate (%)**, and **server process RSS / peak RSS (`VmRSS` / `VmHWM` MiB)** plus CPU time:

1. **Health & Readiness**: `GET /api/v1/health/live`, `GET /api/v1/health/ready`
2. **Auth / Bootstrap / Tenancy**: `GET /api/v1/bootstrap/status`, `GET /api/v1/me`, `GET /api/v1/me/organizations`, `GET /api/v1/organizations/{org_id}/projects`
3. **Spatial Catalog Metadata**: `GET .../layers`, `GET .../layers/{layer_id}`
4. **Bounded Viewport & Spatial Bbox Queries**:
   - `GET .../layers/{layer_1k}/viewport` (1k EPSG:4326 Polygons)
   - `GET .../layers/{layer_10k}/viewport` (10k EPSG:4326 Points)
   - `POST .../layers/{layer_30k}/query` (30k EPSG:4326 spatial `intersects` bbox query)
   - `GET .../layers/{layer_30k}/viewport` (probe documenting current Python `LIMIT 10001` ceiling behavior)
   - `GET .../layers/{layer_vn2000_5899}/features?response_crs=4326&limit=100` (VN-2000 -> WGS 84 transform)
5. **Feature Editing + Revision Conflict**:
   - `POST .../sources/{source_id}/changesets` (`create` + `update` with `base_content_revision` precondition)
   - Explicit stale-revision probe verifying `409 Conflict` (`STALE_CONTENT_REVISION`)
6. **GeoJSON Export Workflow**:
   - `POST .../exports/preview` -> `POST .../exports/confirm` (`output_format="geojson"`, `output_crs=4326`) -> `GET .../artifacts/{artifact_id}/download` on `bench_4326_1k`
7. **Multi-Tenant Negative Check (Pre-flight)**:
   - Verifies `bench-org-beta` session cannot read `bench-org-alpha` layers (`403`/`404` fail-closed).

---

## Usage

### 1. Isolated Python Baseline Capture (Spawns Temp DB + Isolated Uvicorn, Cleans Up Automatically)

```bash
PYTHONPATH=/opt/openvietgis-rust-platform-spec:/opt/openvietgis-rust-platform-spec/apps/api \
/opt/openvietgis/apps/api/.venv/bin/python \
  tools/benchmark/rust-platform/benchmark_runner.py \
  --spawn-python-isolated \
  --scale 1k,10k,30k \
  --concurrencies 1,10,20 \
  --requests-per-level 60 \
  --output-json specs/064-rust-platform/evidence/baseline-benchmark.json \
  --output-md specs/064-rust-platform/evidence/baseline-benchmark.md
```

### 2. Comparative Run Against an External Candidate Server (Rust `apps/api-rust` or Python)

```bash
# Seed once and keep DB (or reuse an existing seeded test DB + manifest):
python3 tools/benchmark/rust-platform/benchmark_runner.py \
  --target-url http://127.0.0.1:18900 \
  --server-pid <CANDIDATE_PID> \
  --db-url postgresql://openvietgis:openvietgis@127.0.0.1:54330/openvietgis_bench_064 \
  --scale 1k,10k,30k \
  --concurrencies 1,10,20 \
  --requests-per-level 60 \
  --output-json specs/064-rust-platform/evidence/rust-candidate-benchmark.json \
  --output-md specs/064-rust-platform/evidence/rust-candidate-benchmark.md
```

### Safety Guarantees

- **NEVER** targets `openvietgis_live` or any production database (explicitly refuses database names not containing `test` or `bench`).
- In `--spawn-python-isolated` mode, creates an isolated temporary database (`openvietgis_bench_064`) on `dev-postgres-test-1` (`127.0.0.1:54330`), runs the server under `openvietgis_runtime` (`NOBYPASSRLS`), and **always** stops the temporary server, removes temporary artifacts, and drops the temporary database in `finally`.