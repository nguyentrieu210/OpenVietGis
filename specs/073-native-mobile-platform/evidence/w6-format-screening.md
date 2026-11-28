# W6 host vector screening and adoption decision
Measured in the isolated assistant Linux x86_64 environment with Python 3.13.5, GDAL 3.11.4 / pyogrio 0.12.1, geopandas 1.1.2, Shapely 2.1.2. Reproduce using tools/benchmark/mobile/columnar/screen_formats.py. Synthetic WGS84 XY points, id and constant name; single run per format in separate process. RSS includes imports, generation and full read. File size is actual main file bytes after writer closed. This is NOT Android/iOS, Rust/FFI copy-count or full fidelity evidence.

| Rows | Format | Bytes | Write s | Read s | Bbox s | Peak RSS KiB |
|---:|---|---:|---:|---:|---:|---:|
| 10000 | GeoJSON | 1414931 | 0.0540 | 0.1131 | 0.0668 | 133152 |
| 10000 | GPKG | 1429504 | 0.0588 | 0.0637 | 0.0055 | 139408 |
| 10000 | FlatGeobuf | 1387896 | 0.0286 | 0.0620 | 0.0037 | 134044 |
| 100000 | GeoJSON | 14265942 | 0.4743 | 0.5947 | 0.6390 | 176984 |
| 100000 | GPKG | 13991936 | 0.3562 | 0.2322 | 0.0262 | 185680 |
| 100000 | FlatGeobuf | 13867896 | 0.2999 | 0.1994 | 0.0205 | 179892 |

Assertions passed for count, stable ID set, EPSG and last point X. Bbox returned 1010 and 10100 rows respectively. No repeated trials, cold-cache controls, mixed polygons or network test.

Decision: retain GeoPackage for mutable local authoritative data; FlatGeobuf candidate for read-only interchange, not adopted as authoritative storage. GeoJSON bounded interchange only. Arrow/GeoArrow/Parquet unavailable in this environment (no pyarrow, GDAL Arrow/Parquet driver absent). No copy-count measurement, Rust driver comparison or mobile qualification; T063 remains open. DataFusion/SedonaDB/wgpu and terrain/COPC/3D remain disabled until independently measured device memory, runtime and licensing evidence exists; do not add speculative dependencies. T064 open.
