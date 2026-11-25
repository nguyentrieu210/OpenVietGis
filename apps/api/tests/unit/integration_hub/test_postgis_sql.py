"""Unit tests for the read-only PostGIS SQL export writer."""

from __future__ import annotations

from decimal import Decimal
from types import SimpleNamespace
from typing import Any

import pytest

from openvietgis_api.adapters.formats.postgis_sql import write_postgis_sql
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping


@pytest.mark.parametrize("crs_code", [4326, 3857, 4756, 5899])
def test_write_postgis_sql_point_with_explicit_readable_srid(crs_code: int) -> None:
    fields = [
        ExportFieldMapping("fld_name", "Name", "fld_name", FieldType.TEXT),
        ExportFieldMapping("fld_rank", "Rank", "fld_rank", FieldType.INTEGER),
        ExportFieldMapping("fld_area", "Area", "fld_area", FieldType.DECIMAL),
        ExportFieldMapping("fld_active", "Active", "fld_active", FieldType.BOOLEAN),
        ExportFieldMapping("fld_ts", "Observed", "fld_ts", FieldType.TIMESTAMP),
    ]
    features: list[tuple[str, dict[str, Any], NativeGeometry]] = [
        (
            "feat-001",
            {
                "fld_name": "Control Point A",
                "fld_rank": 42,
                "fld_area": Decimal("123.45"),
                "fld_active": True,
                "fld_ts": "2026-01-02T03:04:05Z",
            },
            NativeGeometry("Point", (106.7, 10.8), crs_code),
        ),
        (
            "feat-002",
            {
                "fld_name": None,
                "fld_rank": None,
                "fld_area": None,
                "fld_active": False,
                "fld_ts": None,
            },
            NativeGeometry("Point", (106.8, 10.9), crs_code),
        ),
    ]

    payload, ext, media_type = write_postgis_sql(
        layer_name="Control Points",
        crs_code=crs_code,
        geometry_family="Point",
        field_mappings=fields,
        features=features,
    )

    assert ext == "sql"
    assert media_type == "application/sql; charset=utf-8"
    sql = payload.decode("utf-8")
    assert f"-- CRS: EPSG:{crs_code}" in sql
    assert "BEGIN;" in sql
    assert f"geom geometry(POINT, {crs_code})" in sql
    assert f"ST_GeomFromText('POINT(106.7 10.8)', {crs_code})" in sql
    assert f"ST_GeomFromText('POINT(106.8 10.9)', {crs_code})" in sql
    assert "'Control Point A', 42, 123.45, TRUE, '2026-01-02T03:04:05Z'" in sql
    assert "NULL, NULL, NULL, FALSE, NULL" in sql
    assert 'CREATE INDEX IF NOT EXISTS "idx_control_points_geom"' in sql
    assert sql.rstrip().endswith("COMMIT;")


def test_write_postgis_sql_linestring_and_polygon_wkt_and_srid() -> None:
    line_payload, _, _ = write_postgis_sql(
        layer_name="Road Centerline",
        crs_code=5899,
        geometry_family="LineString",
        field_mappings=[ExportFieldMapping("fld_id", "ID", "fld_id", FieldType.TEXT)],
        features=[
            (
                "road-1",
                {"fld_id": "R1"},
                NativeGeometry(
                    "LineString",
                    ((438442.69, 2291039.83), (438500.0, 2291100.5)),
                    5899,
                ),
            )
        ],
    )
    line_sql = line_payload.decode("utf-8")
    assert "geom geometry(LINESTRING, 5899)" in line_sql
    assert (
        "ST_GeomFromText('LINESTRING(438442.69 2291039.83, 438500.0 2291100.5)', 5899)"
        in line_sql
    )

    poly_payload, _, _ = write_postgis_sql(
        layer_name="Parcel Boundary",
        crs_code=4756,
        geometry_family="Polygon",
        field_mappings=[ExportFieldMapping("fld_code", "Code", "fld_code", FieldType.TEXT)],
        features=[
            (
                "parcel-1",
                {"fld_code": "P1"},
                NativeGeometry(
                    "Polygon",
                    (((107.0, 20.8), (107.1, 20.8), (107.1, 20.9), (107.0, 20.8)),),
                    4756,
                ),
            )
        ],
    )
    poly_sql = poly_payload.decode("utf-8")
    assert "geom geometry(POLYGON, 4756)" in poly_sql
    assert (
        "ST_GeomFromText('POLYGON((107.0 20.8, 107.1 20.8, 107.1 20.9, 107.0 20.8))', 4756)"
        in poly_sql
    )


def test_write_postgis_sql_defends_against_sql_injection_and_nul_bytes() -> None:
    malicious_table = 'parcels"; DROP TABLE users; --\nCREATE ROLE evil;'
    malicious_col = 'note"; DROP TABLE users; --\x00'
    malicious_value = "val'; DROP TABLE users; --\x00end"
    malicious_fid = "fid'; DROP TABLE audit; --\x00"

    payload, _, _ = write_postgis_sql(
        layer_name=malicious_table,
        crs_code=4326,
        geometry_family="Point",
        field_mappings=[
            ExportFieldMapping("f1", "Malicious Col", malicious_col, FieldType.TEXT),
        ],
        features=[
            (
                malicious_fid,
                {"f1": malicious_value},
                NativeGeometry("Point", (106.6, 10.7), 4326),
            )
        ],
    )
    assert b"\x00" not in payload
    sql = payload.decode("utf-8")

    # Layer comment must stay on a single comment line (newlines stripped).
    comment_lines = [line for line in sql.splitlines() if line.startswith("-- Layer:")]
    assert len(comment_lines) == 1
    assert "\n" not in comment_lines[0]
    assert not any(line.startswith("CREATE ROLE evil") for line in sql.splitlines())

    # Table identifier is strictly sanitized to [a-z][a-z0-9_]* and double-quoted.
    assert 'CREATE TABLE IF NOT EXISTS "parcels_drop_table_users_create_role_evil" (' in sql

    # Column identifier containing double quotes is escaped by doubling '"' and NUL is stripped.
    assert '"note""; DROP TABLE users; --" TEXT' in sql

    # String literals escape single quotes by doubling "'" and strip NUL bytes.
    assert "'val''; DROP TABLE users; --end'" in sql
    assert "'fid''; DROP TABLE audit; --'" in sql

    # Script structure remains strictly the expected 5 SQL statements.
    non_comment_lines = [line for line in sql.splitlines() if not line.startswith("--")]
    assert len(non_comment_lines) == 5
    assert non_comment_lines[0] == "BEGIN;"
    assert non_comment_lines[1].startswith("CREATE TABLE IF NOT EXISTS ")
    assert non_comment_lines[2].startswith("INSERT INTO ")
    assert non_comment_lines[3].startswith("CREATE INDEX IF NOT EXISTS ")
    assert non_comment_lines[4] == "COMMIT;"


@pytest.mark.parametrize("bad_crs", [3405, 3406, 99999])
def test_write_postgis_sql_rejects_definition_only_and_unknown_crs(bad_crs: int) -> None:
    with pytest.raises(ValueError, match=f"EPSG:{bad_crs}"):
        write_postgis_sql(
            layer_name="Bad CRS",
            crs_code=bad_crs,
            geometry_family="Point",
            field_mappings=[],
            features=[],
        )


@pytest.mark.parametrize("bad_coord", [float("nan"), float("inf"), float("-inf")])
def test_write_postgis_sql_rejects_non_finite_coordinates(bad_coord: float) -> None:
    # Bypass NativeGeometry.__post_init__ to exercise write_postgis_sql's own guard.
    raw_geom: Any = SimpleNamespace(type="Point", coordinates=(bad_coord, 10.7), srid=4326)
    with pytest.raises(ValueError, match="Non-finite coordinate"):
        write_postgis_sql(
            layer_name="Non-finite",
            crs_code=4326,
            geometry_family="Point",
            field_mappings=[],
            features=[("f-1", {}, raw_geom)],
        )


@pytest.mark.parametrize("bad_family", ["MultiPoint", "MultiPolygon", "GeometryCollection"])
def test_write_postgis_sql_rejects_unsupported_geometry_family(bad_family: str) -> None:
    with pytest.raises(ValueError, match="Unsupported geometry family"):
        write_postgis_sql(
            layer_name="Unsupported",
            crs_code=4326,
            geometry_family=bad_family,
            field_mappings=[],
            features=[],
        )


def test_write_postgis_sql_rejects_mismatched_feature_geometry() -> None:
    with pytest.raises(ValueError, match="does not match layer family"):
        write_postgis_sql(
            layer_name="Mismatch",
            crs_code=4326,
            geometry_family="Point",
            field_mappings=[],
            features=[
                (
                    "f-1",
                    {},
                    NativeGeometry("LineString", ((106.6, 10.7), (106.7, 10.8)), 4326),
                )
            ],
        )


def test_vietnamese_layer_names_fold_to_readable_ascii() -> None:
    from openvietgis_api.adapters.formats.postgis_sql import _sanitize_table_name, ascii_fold
    from openvietgis_api.adapters.http.integration_hub import _safe_filename_slug

    name = "Phước Lộc · Tuyến cần đối soát quan hệ địa chỉ"
    assert ascii_fold("Đường Ấp ơ ư") == "Duong Ap o u"
    assert _safe_filename_slug(name) == "phuoc-loc-tuyen-can-doi-soat-quan-he-dia-chi"
    assert _sanitize_table_name(name) == "phuoc_loc_tuyen_can_doi_soat_quan_he_dia_chi"
