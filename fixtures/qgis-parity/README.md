# Spec 065 synthetic parity fixture
Run `python3 fixtures/qgis-parity/generate.py --out /tmp/ovg-qgis-parity` in an isolated checkout; then `cd /tmp/ovg-qgis-parity && sha256sum -c SHA256SUMS.txt`. Do not import into a production tenant.

Vector fixture: three contiguous 100m squares in EPSG:3857, numeric, text, boolean and null fields. Clip, point-buffer, invalid polygon, CSV and simple DXF supplied. Three small deterministic Float32 GeoTIFF rasters use EPSG:3857 with explicit NoData. The expected JSON is an analytical oracle, not a QGIS measurement or actual OVG output. QGIS 3.44 and authenticated OVG runs must independently capture outputs and screenshots; use a separate synthetic project with two separately authorized users. The scenario is a plan, not provisioned users or a passed two-user test.

Recommended comparison views: 1280×800, 1440×900, 1920×1080, mobile 390×844; record actual task time, clicks and p95 separately on matched fixtures.

Independent oracle: run `psql -X -v ON_ERROR_STOP=1 -f vector-oracle.sql` against a newly created disposable PostGIS database only. It asserts planar clip, dissolve, buffer and invalid polygon. The SQL uses synthetic coordinates, creates temporary tables and rolls back. It does not invoke the OpenVietGIS API or QGIS Desktop.
