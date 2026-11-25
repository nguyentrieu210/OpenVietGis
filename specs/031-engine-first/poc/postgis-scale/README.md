# PostGIS scale microbenchmark

Run benchmark.py with the existing API Python environment, against a fresh
disposable PostGIS database named openvietgis_scale_test on loopback only.
Set OVG_SCALE_TEST_DSN to that test DSN. The script creates a synthetic indexed
point table and refuses to overwrite an existing table. Delete the disposable
container after use. No production data, migrations, or persistent credentials.

The deterministic 10k/100k/1M point sets cover a roughly 0.2-degree square.
Measure one fixed z14 cluster tile and one z17 detail tile, five warmups and
30 measured requests per scenario. SQL shape follows professional.py MVT
clustering/detail with stored EPSG:4326 filtering and index, but does not
include tenancy predicates, RLS, HTTP, map rendering or network transport.
Results are not end-to-end scale certification. See evidence/postgis-scale.json.
