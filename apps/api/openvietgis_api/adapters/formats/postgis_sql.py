"""Read-only PostGIS SQL layer export writer for external PostGIS import.

Generates deterministic, self-contained UTF-8 SQL scripts with explicit EPSG SRID,
strictly sanitized identifiers and literals, and zero internal database credentials.
"""

from __future__ import annotations

import math
import re
from collections.abc import Sequence
from decimal import Decimal, InvalidOperation
from typing import Any

from openvietgis_api.domains.spatial.crs import get_readable_crs_profile
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping

_SUPPORTED_GEOM_TYPES: dict[str, str] = {
    "Point": "POINT",
    "LineString": "LINESTRING",
    "Polygon": "POLYGON",
}

_PG_TYPE_MAP: dict[FieldType, str] = {
    FieldType.TEXT: "TEXT",
    FieldType.INTEGER: "BIGINT",
    FieldType.DECIMAL: "NUMERIC",
    FieldType.BOOLEAN: "BOOLEAN",
    FieldType.TIMESTAMP: "TIMESTAMPTZ",
}

_TABLE_IDENT_RE = re.compile(r"^[a-z][a-z0-9_]{0,59}$")
_RESERVED_COLUMNS = frozenset({"fid", "ovg_feature_id", "geom"})


def _sanitize_table_name(layer_name: str) -> str:
    cleaned = re.sub(r"[^a-z0-9_]+", "_", layer_name.strip().lower()).strip("_")
    if not cleaned or not (cleaned[0].isascii() and cleaned[0].isalpha()):
        cleaned = f"layer_{cleaned or 'export'}"
    cleaned = cleaned[:60]
    if _TABLE_IDENT_RE.fullmatch(cleaned) is None:
        return "layer_export"
    return cleaned


def _quote_ident(name: str) -> str:
    if not name or "\x00" in name:
        raise ValueError("SQL identifier must be non-empty and free of NUL bytes.")
    return '"' + name.replace('"', '""') + '"'


def _sanitize_comment(text: str) -> str:
    return re.sub(r"[\r\n\x00]+", " ", text).strip()


def _sql_string(value: str) -> str:
    cleaned = value.replace("\x00", "")
    return "'" + cleaned.replace("'", "''") + "'"


def _sql_decimal(value: Any) -> str:
    if isinstance(value, bool):
        raise ValueError("Boolean value is not a valid decimal literal.")
    try:
        dec = value if isinstance(value, Decimal) else Decimal(str(value))
    except InvalidOperation as exc:
        raise ValueError("Invalid decimal attribute value.") from exc
    if not dec.is_finite():
        raise ValueError("Non-finite decimal attribute value.")
    return str(dec)


def _sql_literal(value: Any, field_type: FieldType) -> str:
    if value is None:
        return "NULL"
    if field_type == FieldType.BOOLEAN:
        if isinstance(value, bool):
            return "TRUE" if value else "FALSE"
        raise ValueError("Expected boolean attribute value.")
    if field_type == FieldType.INTEGER:
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError("Expected integer attribute value.")
        return str(value)
    if field_type == FieldType.DECIMAL:
        return _sql_decimal(value)
    if field_type == FieldType.TIMESTAMP:
        return _sql_string(value if isinstance(value, str) else str(value))
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, Decimal):
        return _sql_decimal(value)
    return _sql_string(value if isinstance(value, str) else str(value))


def _format_xy(pt: Any) -> str:
    if not isinstance(pt, (tuple, list)) or len(pt) != 2:
        raise ValueError("Only 2D XY coordinates are supported.")
    raw_x, raw_y = pt[0], pt[1]
    if isinstance(raw_x, bool) or isinstance(raw_y, bool):
        raise ValueError("Coordinate values must be numeric.")
    try:
        x = float(raw_x)
        y = float(raw_y)
    except (TypeError, ValueError, OverflowError) as exc:
        raise ValueError("Coordinate values must be finite floats.") from exc
    if not (math.isfinite(x) and math.isfinite(y)):
        raise ValueError("Non-finite coordinate in geometry.")
    if x == 0.0:
        x = 0.0
    if y == 0.0:
        y = 0.0
    return f"{x!r} {y!r}"


def _geometry_to_wkt(geom: NativeGeometry, expected_family: str) -> str:
    if geom.type != expected_family:
        raise ValueError(
            f"Feature geometry type '{geom.type}' does not match layer family '{expected_family}'."
        )
    coords: Any = geom.coordinates
    if geom.type == "Point":
        return f"POINT({_format_xy(coords)})"
    if geom.type == "LineString":
        if not isinstance(coords, (tuple, list)) or len(coords) < 2:
            raise ValueError("LineString geometry must contain at least 2 positions.")
        return "LINESTRING(" + ", ".join(_format_xy(pt) for pt in coords) + ")"
    if geom.type == "Polygon":
        if not isinstance(coords, (tuple, list)) or len(coords) < 1:
            raise ValueError("Polygon geometry must contain at least 1 ring.")
        rings: list[str] = []
        for ring in coords:
            if not isinstance(ring, (tuple, list)) or len(ring) < 4:
                raise ValueError("Polygon ring must contain at least 4 positions.")
            rings.append("(" + ", ".join(_format_xy(pt) for pt in ring) + ")")
        return "POLYGON(" + ", ".join(rings) + ")"
    raise ValueError(f"Unsupported geometry family: {geom.type}")


def _resolve_column_names(
    field_mappings: Sequence[ExportFieldMapping],
) -> list[tuple[ExportFieldMapping, str]]:
    used: set[str] = set(_RESERVED_COLUMNS)
    resolved: list[tuple[ExportFieldMapping, str]] = []
    for mapping in field_mappings:
        raw_name = mapping.output_name.replace("\x00", "")
        if not raw_name:
            raise ValueError("Export column output_name must be non-empty.")
        candidate = raw_name
        if candidate.lower() in used:
            candidate = f"attr_{raw_name}"
            suffix = 1
            while candidate.lower() in used:
                suffix += 1
                candidate = f"attr_{raw_name}_{suffix}"
        used.add(candidate.lower())
        resolved.append((mapping, candidate))
    return resolved


def write_postgis_sql(
    *,
    layer_name: str,
    crs_code: int,
    geometry_family: str,
    field_mappings: Sequence[ExportFieldMapping],
    features: Sequence[tuple[str, dict[str, Any], NativeGeometry]],
) -> tuple[bytes, str, str]:
    """Serialize a vector layer into a read-only PostGIS SQL script."""
    if geometry_family not in _SUPPORTED_GEOM_TYPES:
        raise ValueError(f"Unsupported geometry family: {geometry_family}")
    if get_readable_crs_profile(crs_code) is None:
        raise ValueError(f"CRS EPSG:{crs_code} is not an explicit readable EPSG profile.")

    pg_geom_type = _SUPPORTED_GEOM_TYPES[geometry_family]
    table_name = _sanitize_table_name(layer_name)
    quoted_table = _quote_ident(table_name)
    idx_name = f"idx_{table_name[:50]}_geom"
    quoted_idx = _quote_ident(idx_name)
    resolved_fields = _resolve_column_names(field_mappings)

    col_defs = [
        "fid BIGSERIAL PRIMARY KEY",
        "ovg_feature_id TEXT NOT NULL",
    ]
    for mapping, col_name in resolved_fields:
        col_defs.append(f"{_quote_ident(col_name)} {_PG_TYPE_MAP[mapping.type]}")
    col_defs.append(f"geom geometry({pg_geom_type}, {crs_code})")

    insert_cols = ", ".join(
        ["ovg_feature_id"]
        + [_quote_ident(col_name) for _, col_name in resolved_fields]
        + ["geom"]
    )

    lines: list[str] = [
        "-- OpenVietGIS Read-Only PostGIS SQL Export",
        f"-- Layer: {_sanitize_comment(layer_name) or table_name}",
        f"-- CRS: EPSG:{crs_code}",
        (
            "-- Read-only export for external PostGIS import; "
            "no internal database credentials are included."
        ),
        "BEGIN;",
        f"CREATE TABLE IF NOT EXISTS {quoted_table} ({', '.join(col_defs)});",
    ]

    for fid, attrs, geom in features:
        wkt = _geometry_to_wkt(geom, geometry_family)
        vals = [_sql_string(fid)]
        for mapping, _col_name in resolved_fields:
            vals.append(_sql_literal(attrs.get(mapping.field_id), mapping.type))
        vals.append(f"ST_GeomFromText('{wkt}', {crs_code})")
        lines.append(f"INSERT INTO {quoted_table} ({insert_cols}) VALUES ({', '.join(vals)});")

    lines.append(
        f"CREATE INDEX IF NOT EXISTS {quoted_idx} ON {quoted_table} USING GIST (geom);"
    )
    lines.append("COMMIT;")

    payload = ("\n".join(lines) + "\n").encode("utf-8")
    return payload, "sql", "application/sql; charset=utf-8"
