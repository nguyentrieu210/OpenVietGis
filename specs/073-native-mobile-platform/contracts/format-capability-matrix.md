# Initial format qualification matrix v2
E05/E06. Target capabilities, NOT current support. Every driver must pass independent fixtures before advertised; read/write/edit are separate. Import means editable copy in local canonical store, not in-place modification of source.
| Format | Release target | Read | Write | Edit | Constraints |
|---|---|---|---|---|---|
| GeoPackage | Android A1 | yes | yes | copy/native local | point/line/polygon/multi XY, typed fields, CRS, stable IDs where representable |
| GeoJSON | A1 | yes | yes | copy | RFC7946 WGS84 export, explicit conversion, JSON/null fields subset |
| SHP | A1 | yes | yes | copy | .shp/.shx/.dbf/.prj/.cpg, encoding, field truncation/loss warning, split geometry families |
| TAB / MIF MID | A1 | yes | qualified write | copy | resolve TAB companions and encoding, compare style/fields/CRS loss |
| KML/KMZ | A1 | yes | qualified write | copy | archive bounds, WGS84, no remote network links, styles subset |
| GPX | A1 | yes | yes | copy | waypoint/route/track, elevation/time if present, WGS84 |
| CSV/XLSX | A1 | coordinate columns | qualified export | copy | explicit x/y/CRS/delimiter/encoding, formulas not executed, no inferred CRS |
| DXF | A1 qualified subset | yes | qualified subset | copy | supported 2D entities, units/CRS manual if absent, handles/loss report |
| MBTiles | A1 | yes | no target | no | raster/vector tile schema, TMS/XYZ explicit, attribution |
| PMTiles | A1 | qualified read | no target | no | supported version, range/local read, no blind network |
| GeoTIFF/COG | A1 basic / advanced A4 | qualified read | later | no | geotransform/nodata, block/range, no entire raster in RAM |
| DGN V7/V8, GTP, MDB, QGZ | later/experimental | unqualified | unqualified | no | driver/license/format evidence before enable |
| LAS/LAZ/COPC, 3D Tiles, terrain | experimental | gated | not required | no | LOD/streaming and resource proof; no default capability |
Default geometry XY; Z/M, curves, mixed collections, topology, style, attachments and field constraints preserved only in verified profile or reject/warn before write. No silent loss or invalid CRS. Export uses explicit confirmation with per-field/geometry loss report. Use independent GDAL/QGIS roundtrip: count, attributes incl Vietnamese Unicode/nulls, geometry/topology tolerance, CRS, bounding box, stable IDs where supported. Corrupt/zip bomb/traversal, missing companions, oversized features, cancellation and atomic output cleanup. 
## CRS fixture freeze
For each enabled EPSG/region/pipeline: source/target axis/unit, grid checksum, transformation area, official/independent control-point source, tolerance in meters/degrees separately, expected results, datum and version. Freeze tolerances BEFORE measurements; distinguish geographic geodesic vs projected planar distance/area. WGS84/WebMercator cannot substitute survey-grade VN-2000. Unsupported HN-72/VN-2000 regional datum transforms explicitly blocked, no invented parameters.
