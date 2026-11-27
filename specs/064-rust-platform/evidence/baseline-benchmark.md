# Spec 064 Baseline Benchmark Report (Python/FastAPI Reference)

- **Generated (UTC)**: `2026-11-25T17:22:48.641373+00:00`
- **Target**: `python-fastapi-reference` (`http://127.0.0.1:18899`)
- **Git Commit**: `731d3441ec72f3e1c66513fefe0b54d616f9872b` (branch `064-rust-platform`)
- **Host**: `Milion` — `2 vCPU` (`Intel(R) Xeon(R) Platinum 8171M CPU @ 2.60GHz`), `3921.8 MiB RAM`, `Linux-6.12.41+deb13-amd64-x86_64-with-glibc2.41`
- **Python**: `3.13.18`
- **PostgreSQL**: `PostgreSQL 18.7 (Debian 18.7-1.pgdg13+2) on x86_64-pc-linux-gnu, compiled by gcc (Debian 14.2.0-19) 14.2.0, 64-bit`
- **PostGIS**: `POSTGIS="3.7.0 1fdbb1c" [EXTENSION] PGSQL="180" GEOS="3.15.0-CAPI-1.21.0" PROJ="9.9.0 NETWORK_ENABLED=OFF URL_ENDPOINT=https://cdn.proj.org USER_WRITABLE_DIRECTORY=/var/lib/postgresql/.local/share/proj DATABASE_PATH=/usr/share/proj/proj.db" (compiled against PROJ 9.9.0) LIBXML="2.9.14" LIBJSON="0.18" LIBPROTOBUF="1.5.1" WAGYU="0.5.0 (Internal)" TOPOLOGY`
- **Dataset Seed**: `20261125` | Scales: `1k, 10k, 30k` | Seed Time: `6.695 s`

## Process Resource Footprint (Isolated Single-Worker Uvicorn, `openvietgis_runtime` RLS)

| Metric | Measured Value | Spec 064 Target (`SC-005`) |
|---|---:|---|
| Idle RSS (`VmRSS`) | `163.36 MiB` | — |
| End-of-Run RSS (`VmRSS`) | `207.5 MiB` | — |
| **Peak RSS (`VmHWM`)** | **`207.5 MiB`** | `<= 1024 MiB` aggregate app RSS on 4 GiB host |
| Server CPU Time (user+sys) | `61.62 s` | — |

## Pre-Flight & Contract Findings

- **Health Endpoints**: `/api/v1/health` -> `404` (not mounted; canonical probes are `/api/v1/health/live` -> `200` and `/api/v1/health/ready` -> `200`).
- **Multi-Tenant RLS Negative Check**: Org Beta requesting Org Alpha layer -> HTTP `404` (`passed_fail_closed=True`).
- **30k Viewport Ceiling Probe (`GET .../layers/{30k}/viewport`)**: HTTP `503` — `{"code":"MAP_READ_UNAVAILABLE","detail":"Map read unavailable; retry."}` (confirms current Python `map_viewport.py` `LIMIT 10001` count-mismatch ceiling above 10k features, motivating Spec 064 US2 / `SC-004` bounded tile delivery).
- **VN-2000 EPSG:3405 Fixture Note**: `1,000` synthetic `SRID=3405` points seeded in PostGIS `spatial.reference_features`; current DB `spatial.valid_extent()` constrains `validation_state='ready'` to `(4326, 3857, 4756, 5899)`, so live VN-2000 transform benchmarks exercise `EPSG:5899 -> 4326` while `EPSG:3405` awaits `T010` control-point activation.

## Seeded Synthetic GIS Dataset Inventory

| Layer Key | CRS | Geometry | Feature Count | Validation State |
|---|---|---|---:|---|
| `epsg4326_1k` | `EPSG:4326` | `Polygon` | `1,000` | `ready` |
| `epsg4326_10k` | `EPSG:4326` | `Point` | `10,000` | `ready` |
| `epsg4326_30k` | `EPSG:4326` | `Point` | `30,000` | `ready` |
| `vn2000_5899_1k` | `EPSG:5899` | `Point` | `1,000` | `ready` |
| `vn2000_3405_1k` | `EPSG:3405` | `Point` | `1,000` | `not_ready` |
| `edit_4326` | `EPSG:4326` | `Point` | `50` | `ready` |
| `tenant_b_1k` | `EPSG:4326` | `Point` | `100` | `ready` |

## Cold Start Latency Summary (First Uncached Requests)

| Scenario | Reqs | p50 (ms) | p95 (ms) | p99 (ms) | Max (ms) | Error % |
|---|---:|---:|---:|---:|---:|---:|
| `health_live` | 3 | 4.21 | 4.84 | 4.90 | 4.92 | 0.00% |
| `health_ready` | 3 | 4.87 | 5.53 | 5.59 | 5.61 | 0.00% |
| `bootstrap_status` | 3 | 9.43 | 11.26 | 11.42 | 11.46 | 0.00% |
| `auth_me` | 3 | 23.08 | 30.19 | 30.82 | 30.98 | 0.00% |
| `list_organizations` | 3 | 20.25 | 25.57 | 26.04 | 26.16 | 0.00% |
| `list_projects` | 3 | 29.37 | 34.69 | 35.17 | 35.28 | 0.00% |
| `list_layers` | 3 | 135.37 | 145.51 | 146.41 | 146.64 | 0.00% |
| `get_layer_metadata` | 3 | 40.83 | 48.79 | 49.49 | 49.67 | 0.00% |
| `viewport_bbox_1k_polygon` | 3 | 191.69 | 295.49 | 304.71 | 307.02 | 0.00% |
| `viewport_bbox_10k_point` | 3 | 940.82 | 1940.66 | 2029.53 | 2051.75 | 33.33% |
| `spatial_bbox_query_30k` | 3 | 6269.40 | 7604.96 | 7723.67 | 7753.35 | 33.33% |
| `features_page_30k` | 3 | 78.52 | 81.28 | 81.52 | 81.58 | 0.00% |
| `features_vn2000_5899_to_4326` | 3 | 68.41 | 78.94 | 79.88 | 80.11 | 100.00% |
| `feature_edit_and_conflict` | 3 | 113.69 | 120.07 | 120.64 | 120.78 | 0.00% |
| `geojson_export_1k` | 2 | 780.37 | 836.93 | 841.96 | 843.21 | 0.00% |

## Warm Performance Across Concurrency 1, 10, and 20

| Scenario | Concurrency | Reqs | p50 (ms) | p95 (ms) | p99 (ms) | Throughput (req/s) | Error % | Peak RSS After (MiB) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `health_live` | 1 | 60 | 4.23 | 5.94 | 6.58 | 224.50 | 0.00% | 173.7 |
| `health_live` | 10 | 60 | 23.21 | 118.07 | 162.27 | 283.97 | 0.00% | 173.7 |
| `health_live` | 20 | 60 | 44.28 | 113.45 | 168.39 | 296.46 | 0.00% | 173.7 |
| `health_ready` | 1 | 60 | 5.73 | 7.79 | 8.35 | 167.53 | 0.00% | 174.4 |
| `health_ready` | 10 | 60 | 55.15 | 145.94 | 172.18 | 136.15 | 0.00% | 174.4 |
| `health_ready` | 20 | 60 | 75.56 | 257.98 | 300.67 | 183.39 | 0.00% | 174.4 |
| `bootstrap_status` | 1 | 60 | 8.00 | 11.72 | 17.26 | 117.68 | 0.00% | 175.3 |
| `bootstrap_status` | 10 | 60 | 58.83 | 199.73 | 236.12 | 123.85 | 0.00% | 175.3 |
| `bootstrap_status` | 20 | 60 | 188.69 | 359.44 | 395.38 | 84.52 | 0.00% | 175.3 |
| `auth_me` | 1 | 60 | 14.64 | 20.46 | 24.55 | 66.78 | 0.00% | 175.9 |
| `auth_me` | 10 | 60 | 124.81 | 289.90 | 363.13 | 67.95 | 0.00% | 175.9 |
| `auth_me` | 20 | 60 | 178.72 | 620.68 | 759.04 | 72.71 | 0.00% | 175.9 |
| `list_organizations` | 1 | 60 | 12.15 | 15.80 | 18.38 | 79.92 | 0.00% | 176.2 |
| `list_organizations` | 10 | 60 | 91.88 | 254.99 | 347.83 | 83.16 | 0.00% | 176.2 |
| `list_organizations` | 20 | 60 | 211.08 | 587.16 | 686.87 | 66.57 | 0.00% | 176.2 |
| `list_projects` | 1 | 60 | 18.82 | 30.14 | 34.50 | 49.29 | 0.00% | 176.8 |
| `list_projects` | 10 | 60 | 178.10 | 363.74 | 421.41 | 51.62 | 0.00% | 176.8 |
| `list_projects` | 20 | 60 | 338.99 | 554.27 | 588.02 | 50.84 | 0.00% | 176.8 |
| `list_layers` | 1 | 60 | 60.84 | 76.96 | 90.11 | 16.28 | 0.00% | 177.8 |
| `list_layers` | 10 | 60 | 526.20 | 861.66 | 984.64 | 17.75 | 0.00% | 177.8 |
| `list_layers` | 20 | 60 | 1238.57 | 1573.62 | 1620.10 | 15.49 | 0.00% | 177.8 |
| `get_layer_metadata` | 1 | 60 | 33.42 | 52.85 | 55.62 | 27.77 | 0.00% | 177.9 |
| `get_layer_metadata` | 10 | 60 | 311.09 | 727.28 | 811.59 | 27.70 | 0.00% | 177.9 |
| `get_layer_metadata` | 20 | 60 | 603.98 | 1043.07 | 1198.17 | 27.63 | 0.00% | 177.9 |
| `viewport_bbox_1k_polygon` | 1 | 60 | 165.45 | 331.62 | 409.62 | 5.39 | 0.00% | 187.4 |
| `viewport_bbox_1k_polygon` | 10 | 60 | 1329.19 | 2380.90 | 2508.24 | 6.85 | 6.67% | 187.4 |
| `viewport_bbox_1k_polygon` | 20 | 60 | 2325.34 | 3482.12 | 3701.61 | 7.66 | 0.00% | 187.4 |
| `viewport_bbox_10k_point` | 1 | 10 | 871.83 | 1710.40 | 2043.76 | 1.01 | 10.00% | 187.5 |
| `viewport_bbox_10k_point` | 10 | 10 | 2409.30 | 2553.68 | 2564.16 | 3.89 | 100.00% | 187.5 |
| `viewport_bbox_10k_point` | 20 | 20 | 2784.75 | 4626.13 | 4633.53 | 4.31 | 95.00% | 187.5 |
| `spatial_bbox_query_30k` | 1 | 5 | 5714.22 | 6023.65 | 6026.79 | 0.17 | 0.00% | 187.9 |
| `spatial_bbox_query_30k` | 10 | 10 | 29895.94 | 51514.11 | 53748.87 | 0.18 | 0.00% | 187.9 |
| `spatial_bbox_query_30k` | 20 | 20 | 49745.48 | 88675.28 | 91649.14 | 0.22 | 0.00% | 187.9 |
| `features_page_30k` | 1 | 60 | 56.38 | 73.12 | 80.27 | 17.55 | 0.00% | 188.0 |
| `features_page_30k` | 10 | 60 | 473.08 | 805.17 | 835.33 | 19.63 | 0.00% | 188.0 |
| `features_page_30k` | 20 | 60 | 872.46 | 1265.02 | 1301.19 | 20.67 | 0.00% | 188.0 |
| `features_vn2000_5899_to_4326` | 1 | 60 | 63.36 | 76.89 | 85.78 | 15.65 | 100.00% | 188.1 |
| `features_vn2000_5899_to_4326` | 10 | 60 | 544.76 | 1259.94 | 1471.95 | 14.94 | 100.00% | 188.1 |
| `features_vn2000_5899_to_4326` | 20 | 60 | 1013.17 | 2572.25 | 2614.99 | 14.78 | 100.00% | 188.1 |
| `feature_edit_and_conflict` | 1 | 40 | 77.52 | 105.43 | 107.16 | 13.71 | 0.00% | 188.1 |
| `feature_edit_and_conflict` | 10 | 60 | 314.88 | 460.23 | 504.91 | 22.10 | 0.00% | 188.1 |
| `feature_edit_and_conflict` | 20 | 60 | 642.03 | 927.73 | 993.68 | 22.06 | 0.00% | 188.1 |
| `geojson_export_1k` | 1 | 10 | 708.75 | 1118.96 | 1176.69 | 1.28 | 0.00% | 207.5 |
| `geojson_export_1k` | 10 | 20 | 4182.61 | 6906.96 | 7343.70 | 1.88 | 90.00% | 207.5 |
| `geojson_export_1k` | 20 | 20 | 9689.51 | 14525.53 | 14589.21 | 1.37 | 95.00% | 207.5 |

## Comparative Acceptance Targets for Rust Candidate (`T020` / `SC-003`)

When running this exact harness with `--target-url <rust_api>` on the same host and database:
- **Viewport p95 at 20 concurrent clients**: Rust MUST achieve `<= 70%` of Python baseline (`viewport_bbox_1k_polygon`, `viewport_bbox_10k_point`, `spatial_bbox_query_30k`).
- **Feature API p95 at 20 concurrent clients**: Rust MUST achieve `<= 100%` of Python baseline (`features_page_30k`, `features_vn2000_5899_to_4326`, `feature_edit_and_conflict`).
- **Error Rate**: Rust MUST have `<=` Python baseline error rate (`0.00%`).
- **Memory**: Aggregate Rust app RSS `<= 1024 MiB` on isolated 4 GiB profile with zero swap growth.

