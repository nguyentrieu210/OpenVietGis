"""Generate deterministic, privacy-free synthetic GIS/CAD fixtures for Community QA (Issue #7).

Produces under fixtures/community-qa/:
- synthetic_gis_4326.geojson       (Point, LineString, Polygon with interior hole; EPSG:4326)
- synthetic_gis_3857.geojson       (Same synthetic features projected to EPSG:3857)
- synthetic_invalid_geometry.geojson (Intentional self-intersecting bowtie Polygon + degenerate LineString)
- synthetic_gis_4326.gpkg          (GeoPackage with Point/Line/Polygon+hole layers; EPSG:4326)
- synthetic_gis_4326_shp.zip       (ESRI Shapefile ZIP with .shp/.shx/.dbf/.prj/.cpg; EPSG:4326)
- synthetic_cad_2d.dxf             (Valid 2D ASCII DXF with synthetic POINT/LINE/LWPOLYLINE layers)
- manifest.json                    (Expected feature counts, geometry types, CRS, schema, SHA-256)
- SHA256SUMS.txt                   (Standard sha256sum file)
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "fixtures" / "community-qa"


def _lonlat_to_3857(lon: float, lat: float) -> list[float]:
    r = 6378137.0
    x = math.radians(lon) * r
    y = math.log(math.tan(math.pi / 4.0 + math.radians(lat) / 2.0)) * r
    return [round(x, 3), round(y, 3)]


def _transform_coords(coords: object) -> object:
    if (
        isinstance(coords, list)
        and len(coords) >= 2
        and isinstance(coords[0], (int, float))
        and isinstance(coords[1], (int, float))
    ):
        return _lonlat_to_3857(float(coords[0]), float(coords[1]))
    if isinstance(coords, list):
        return [_transform_coords(c) for c in coords]
    return coords


def _valid_features_4326() -> list[dict[str, object]]:
    return [
        {
            "type": "Feature",
            "id": "qa-pt-001",
            "properties": {
                "code": "SYN-PT-001",
                "name": "Synthetic Survey Benchmark A",
                "category": "control_point",
                "elevation_m": 12.5,
            },
            "geometry": {"type": "Point", "coordinates": [105.8501, 21.0285]},
        },
        {
            "type": "Feature",
            "id": "qa-ln-001",
            "properties": {
                "code": "SYN-LN-001",
                "name": "Synthetic Corridor Axis 1",
                "category": "road_centerline",
                "elevation_m": 11.2,
            },
            "geometry": {
                "type": "LineString",
                "coordinates": [
                    [105.8490, 21.0280],
                    [105.8505, 21.0288],
                    [105.8520, 21.0294],
                ],
            },
        },
        {
            "type": "Feature",
            "id": "qa-pg-001",
            "properties": {
                "code": "SYN-PG-001",
                "name": "Synthetic Parcel With Courtyard Hole",
                "category": "parcel_with_hole",
                "elevation_m": 10.8,
            },
            "geometry": {
                "type": "Polygon",
                "coordinates": [
                    [
                        [105.8495, 21.0275],
                        [105.8515, 21.0275],
                        [105.8515, 21.0295],
                        [105.8495, 21.0295],
                        [105.8495, 21.0275],
                    ],
                    [
                        [105.8501, 21.0281],
                        [105.8501, 21.0289],
                        [105.8509, 21.0289],
                        [105.8509, 21.0281],
                        [105.8501, 21.0281],
                    ],
                ],
            },
        },
    ]


def _invalid_features_4326() -> list[dict[str, object]]:
    return [
        {
            "type": "Feature",
            "id": "qa-bad-bowtie",
            "properties": {
                "code": "SYN-BAD-001",
                "expected_failure": "self_intersection_bowtie_polygon",
            },
            "geometry": {
                "type": "Polygon",
                "coordinates": [
                    [
                        [105.8500, 21.0280],
                        [105.8510, 21.0290],
                        [105.8510, 21.0280],
                        [105.8500, 21.0290],
                        [105.8500, 21.0280],
                    ]
                ],
            },
        },
        {
            "type": "Feature",
            "id": "qa-bad-line",
            "properties": {
                "code": "SYN-BAD-002",
                "expected_failure": "zero_length_degenerate_linestring",
            },
            "geometry": {
                "type": "LineString",
                "coordinates": [[105.8500, 21.0280], [105.8500, 21.0280]],
            },
        },
    ]


def _write_dxf_2d(path: Path) -> None:
    lines = [
        "0", "SECTION",
        "2", "HEADER",
        "9", "$ACADVER",
        "1", "AC1015",
        "0", "ENDSEC",
        "0", "SECTION",
        "2", "ENTITIES",
        # Synthetic Point
        "0", "POINT",
        "8", "SYN_POINTS",
        "10", "588350.0",
        "20", "2325410.0",
        "30", "0.0",
        # Synthetic Line
        "0", "LINE",
        "8", "SYN_LINES",
        "10", "588340.0",
        "20", "2325400.0",
        "30", "0.0",
        "11", "588390.0",
        "21", "2325430.0",
        "31", "0.0",
        # Synthetic Closed Polygon (LWPOLYLINE)
        "0", "LWPOLYLINE",
        "8", "SYN_PARCELS",
        "90", "4",
        "70", "1",
        "10", "588345.0", "20", "2325395.0",
        "10", "588385.0", "20", "2325395.0",
        "10", "588385.0", "20", "2325435.0",
        "10", "588345.0", "20", "2325435.0",
        "0", "ENDSEC",
        "0", "EOF",
    ]
    path.write_text("\n".join(lines) + "\n", encoding="ascii")


def _write_gpkg_and_shp(features_4326: list[dict[str, object]], out_dir: Path) -> None:
    from openvietgis_api.adapters.formats.geopackage import write_geopackage
    from openvietgis_api.adapters.formats.shapefile_zip import write_shapefile_zip
    from openvietgis_api.domains.spatial.domain import (
        FieldType,
        GeometryDimensions,
        NativeGeometry,
    )
    from openvietgis_api.domains.transfers.domain import ExportFieldMapping

    field_mappings = [
        ExportFieldMapping("code", "code", "code", FieldType.TEXT),
        ExportFieldMapping("name", "name", "name", FieldType.TEXT),
        ExportFieldMapping("category", "category", "category", FieldType.TEXT),
        ExportFieldMapping("elevation_m", "elevation_m", "elev_m", FieldType.DECIMAL),
    ]
    poly_feature = next(
        f for f in features_4326 if f["geometry"]["type"] == "Polygon"  # type: ignore[index]
    )
    poly_geom = NativeGeometry(
        type="Polygon",
        coordinates=poly_feature["geometry"]["coordinates"],  # type: ignore[index,arg-type]
        dimensions=GeometryDimensions.XY,
        srid=4326,
    )
    poly_rows = [(str(poly_feature["id"]), dict(poly_feature["properties"]), poly_geom)]  # type: ignore[arg-type]

    gpkg_bytes, _, _ = write_geopackage(
        layer_name="synthetic_parcels_4326",
        crs_code=4326,
        geometry_family="Polygon",
        field_mappings=field_mappings,
        features=poly_rows,
    )
    (out_dir / "synthetic_gis_4326.gpkg").write_bytes(gpkg_bytes)

    shp_bytes, _, _ = write_shapefile_zip(
        layer_name="synthetic_parcels_4326",
        crs_code=4326,
        geometry_family="Polygon",
        field_mappings=field_mappings,
        features=poly_rows,
    )
    (out_dir / "synthetic_gis_4326_shp.zip").write_bytes(shp_bytes)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def generate(out_dir: Path = OUT_DIR) -> dict[str, object]:
    out_dir.mkdir(parents=True, exist_ok=True)
    valid_4326 = _valid_features_4326()
    valid_3857 = [
        {
            "type": "Feature",
            "id": f["id"],
            "properties": f["properties"],
            "geometry": {
                "type": f["geometry"]["type"],  # type: ignore[index]
                "coordinates": _transform_coords(f["geometry"]["coordinates"]),  # type: ignore[index]
            },
        }
        for f in valid_4326
    ]
    invalid_4326 = _invalid_features_4326()

    (out_dir / "synthetic_gis_4326.geojson").write_text(
        json.dumps({"type": "FeatureCollection", "features": valid_4326}, indent=2) + "\n",
        encoding="utf-8",
    )
    (out_dir / "synthetic_gis_3857.geojson").write_text(
        json.dumps(
            {
                "type": "FeatureCollection",
                "name": "synthetic_gis_3857",
                "crs_note": "EPSG:3857 Web Mercator meters (for CRS reprojection QA)",
                "features": valid_3857,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    (out_dir / "synthetic_invalid_geometry.geojson").write_text(
        json.dumps({"type": "FeatureCollection", "features": invalid_4326}, indent=2) + "\n",
        encoding="utf-8",
    )
    _write_dxf_2d(out_dir / "synthetic_cad_2d.dxf")
    _write_gpkg_and_shp(valid_4326, out_dir)

    artifacts = [
        {
            "file": "synthetic_gis_4326.geojson",
            "format": "GeoJSON",
            "crs": "EPSG:4326",
            "feature_count": 3,
            "geometries": ["Point", "LineString", "Polygon (with interior hole)"],
            "negative_test": False,
        },
        {
            "file": "synthetic_gis_3857.geojson",
            "format": "GeoJSON",
            "crs": "EPSG:3857",
            "feature_count": 3,
            "geometries": ["Point", "LineString", "Polygon (with interior hole)"],
            "negative_test": False,
        },
        {
            "file": "synthetic_invalid_geometry.geojson",
            "format": "GeoJSON",
            "crs": "EPSG:4326",
            "feature_count": 2,
            "geometries": ["Polygon (self-intersecting bowtie)", "LineString (degenerate zero-length)"],
            "negative_test": True,
        },
        {
            "file": "synthetic_gis_4326.gpkg",
            "format": "GeoPackage",
            "crs": "EPSG:4326",
            "feature_count": 1,
            "layers": ["synthetic_parcels_4326"],
            "geometries": ["Polygon (with interior hole)"],
            "negative_test": False,
        },
        {
            "file": "synthetic_gis_4326_shp.zip",
            "format": "ESRI Shapefile ZIP",
            "crs": "EPSG:4326",
            "feature_count": 1,
            "geometries": ["Polygon (with interior hole)"],
            "negative_test": False,
        },
        {
            "file": "synthetic_cad_2d.dxf",
            "format": "DXF 2D (AC1015)",
            "crs": "Projected 2D synthetic meters (declare EPSG:32648 on import)",
            "feature_count": 3,
            "geometries": ["POINT", "LINE", "LWPOLYLINE (closed)"],
            "negative_test": False,
        },
    ]

    checksum_lines: list[str] = []
    for item in artifacts:
        digest = _sha256(out_dir / str(item["file"]))
        item["sha256"] = digest
        checksum_lines.append(f"{digest}  {item['file']}")

    manifest = {
        "schema_version": 1,
        "purpose": "Community QA isolated synthetic fixtures (Issue #7)",
        "contains_private_data": False,
        "artifacts": artifacts,
    }
    (out_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / "SHA256SUMS.txt").write_text(
        "\n".join(checksum_lines) + "\n", encoding="utf-8"
    )
    return manifest


if __name__ == "__main__":
    result = generate()
    print(f"Generated {len(result['artifacts'])} synthetic Community QA fixtures in {OUT_DIR}")  # type: ignore[arg-type]
