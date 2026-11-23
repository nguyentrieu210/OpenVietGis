# Community QA Synthetic Fixtures (Issue #7)

Privacy-free, deterministic GIS & CAD test fixtures for independent Community QA (`docs/community/TEST_RUNBOOK.md`, Issues #4 and #5). **Contains zero production or customer data.**

## Included Files

| File | Format | CRS | Features | Purpose |
|---|---|---|---:|---|
| `synthetic_gis_4326.geojson` | GeoJSON | EPSG:4326 | 3 | Point, LineString, Polygon with interior courtyard hole |
| `synthetic_gis_3857.geojson` | GeoJSON | EPSG:3857 | 3 | Projected Web Mercator coordinates for CRS reprojection QA |
| `synthetic_invalid_geometry.geojson` | GeoJSON | EPSG:4326 | 2 | Negative tests: self-intersecting bowtie Polygon & degenerate LineString |
| `synthetic_gis_4326.gpkg` | GeoPackage | EPSG:4326 | 3 | 3 layers (`qa_points`, `qa_lines`, `qa_polygons`) for import/export & QGIS sync |
| `synthetic_gis_4326_shp.zip` | Shapefile ZIP | EPSG:4326 | 1 | `.shp/.shx/.dbf/.prj/.cpg` archive with polygon hole |
| `synthetic_cad_2d.dxf` | DXF 2D (AC1015) | Synthetic meters (declare EPSG:32648) | 3 | `POINT`, `LINE`, closed `LWPOLYLINE` for CAD import/export QA |

## Verify Checksums & Regenerate

```sh
sha256sum -c fixtures/community-qa/SHA256SUMS.txt
uv run --project apps/api python tools/generate_community_qa_fixtures.py
uv run --project apps/api pytest tools/tests/test_community_qa_fixtures.py
```
