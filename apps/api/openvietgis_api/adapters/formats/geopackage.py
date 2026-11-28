"""OGC GeoPackage 1.2/1.3 vector reader and writer (built on Python sqlite3)."""

from __future__ import annotations

import math
import os
import re
import sqlite3
import struct
import tempfile
from collections.abc import Sequence
from datetime import datetime, timezone
from decimal import Context, Decimal
from pathlib import Path
from typing import Any

from openvietgis_api.adapters.formats.wkt_crs import EPSG_NAMES, EPSG_WKT1
from openvietgis_api.domains.spatial.domain import (
    AttributeValue,
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    NativeGeometry,
)
from openvietgis_api.domains.transfers.domain import (
    MAX_STRING_LEN,
    MAX_TRANSFER_FEATURES,
    MAX_TRANSFER_FIELDS,
    SUPPORTED_CRS_CODES,
    ExportFieldMapping,
    InspectedField,
    InspectedMember,
    ParsedFeatureRecord,
    ParsedMemberData,
    ValidationFinding,
    sanitize_field_id,
)
from openvietgis_api.domains.transfers.ports import TransferFormatError

_SQLITE_MAGIC = b"SQLite format 3\x00"
_SAFE_IDENT_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{0,63}$")
_FEAT_ID_RE = re.compile(r"^[A-Za-z0-9_-]{1,128}$")
_ISO_TS_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$")

_GPKG_FAMILY_MAP: dict[str, GeometryFamily] = {
    "POINT": GeometryFamily.POINT,
    "LINESTRING": GeometryFamily.LINE_STRING,
    "POLYGON": GeometryFamily.POLYGON,
}


def _quote_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def _decode_wkb_2d(wkb: bytes) -> tuple[GeometryFamily, Any, bool]:
    """Decode standard ISO WKB 2D Point/LineString/Polygon; return (family, coords, is_non_2d)."""
    if len(wkb) < 5:
        raise ValueError("Truncated WKB geometry")
    endian = "<" if wkb[0] == 1 else ">"
    raw_type = struct.unpack(endian + "I", wkb[1:5])[0]
    has_z_or_m = (raw_type >= 1000) or bool(raw_type & 0xE0000000)
    if has_z_or_m:
        return GeometryFamily.UNSUPPORTED, (), True
    pos = 5

    def read_point() -> tuple[float, float]:
        nonlocal pos
        if pos + 16 > len(wkb):
            raise ValueError("Truncated WKB point")
        x, y = struct.unpack(endian + "dd", wkb[pos : pos + 16])
        pos += 16
        if not (math.isfinite(x) and math.isfinite(y)):
            raise ValueError("Non-finite coordinate")
        return (float(x), float(y))

    if raw_type == 1:
        return GeometryFamily.POINT, read_point(), False
    if raw_type == 2:
        if pos + 4 > len(wkb):
            raise ValueError("Truncated WKB linestring")
        count = struct.unpack(endian + "I", wkb[pos : pos + 4])[0]
        pos += 4
        if count < 2:
            raise ValueError("Empty or degenerate linestring")
        return GeometryFamily.LINE_STRING, tuple(read_point() for _ in range(count)), False
    if raw_type == 3:
        if pos + 4 > len(wkb):
            raise ValueError("Truncated WKB polygon")
        ring_count = struct.unpack(endian + "I", wkb[pos : pos + 4])[0]
        pos += 4
        if ring_count < 1:
            raise ValueError("Empty polygon")
        rings = []
        for _ in range(ring_count):
            if pos + 4 > len(wkb):
                raise ValueError("Truncated WKB ring")
            pt_count = struct.unpack(endian + "I", wkb[pos : pos + 4])[0]
            pos += 4
            if pt_count < 4:
                raise ValueError("Invalid polygon ring")
            rings.append(tuple(read_point() for _ in range(pt_count)))
        return GeometryFamily.POLYGON, tuple(rings), False
    raise ValueError(f"Unsupported WKB geometry type {raw_type}")


def _decode_gpkg_geom(blob: bytes) -> tuple[GeometryFamily, Any, int, bool]:
    """Decode OGC GeoPackage StandardBinary geometry blob."""
    if len(blob) < 8 or blob[0:2] != b"GP":
        raise ValueError("Invalid GeoPackage geometry header")
    flags = blob[3]
    is_empty = bool((flags >> 4) & 1)
    if is_empty:
        raise ValueError("Empty geometry is not supported")
    envelope_indicator = (flags >> 1) & 0x07
    envelope_sizes = {0: 0, 1: 32, 2: 48, 3: 48, 4: 64}
    if envelope_indicator not in envelope_sizes:
        raise ValueError("Invalid GeoPackage envelope indicator")
    hdr_endian = "<" if (flags & 1) == 1 else ">"
    srs_id = struct.unpack(hdr_endian + "i", blob[4:8])[0]
    wkb_offset = 8 + envelope_sizes[envelope_indicator]
    family, coords, non_2d = _decode_wkb_2d(blob[wkb_offset:])
    return family, coords, srs_id, non_2d


def _encode_wkb_2d(geom: NativeGeometry) -> bytes:
    out = bytearray()
    out.append(1)  # little-endian
    coords: Any = geom.coordinates
    if geom.type == "Point":
        out.extend(struct.pack("<I", 1))
        out.extend(struct.pack("<dd", float(coords[0]), float(coords[1])))
    elif geom.type == "LineString":
        out.extend(struct.pack("<II", 2, len(coords)))
        for pt in coords:
            out.extend(struct.pack("<dd", float(pt[0]), float(pt[1])))
    elif geom.type == "Polygon":
        out.extend(struct.pack("<II", 3, len(coords)))
        for ring in coords:
            out.extend(struct.pack("<I", len(ring)))
            for pt in ring:
                out.extend(struct.pack("<dd", float(pt[0]), float(pt[1])))
    else:
        raise ValueError(f"Unsupported geometry type: {geom.type}")
    return bytes(out)


def _encode_gpkg_geom(geom: NativeGeometry, crs_code: int) -> bytes:
    # Magic 'GP', version 0, flags 0x01 (little-endian, no envelope, standard, non-empty)
    header = b"GP" + bytes([0, 1]) + struct.pack("<i", crs_code)
    return header + _encode_wkb_2d(geom)


def _map_sqlite_col_type(decl: str) -> FieldType:
    upper = (decl or "TEXT").upper()
    if "BOOL" in upper:
        return FieldType.BOOLEAN
    if "INT" in upper:
        return FieldType.INTEGER
    if any(tok in upper for tok in ("REAL", "DOUBLE", "FLOAT", "NUMERIC", "DEC")):
        return FieldType.DECIMAL
    if "DATETIME" in upper or "TIMESTAMP" in upper:
        return FieldType.TIMESTAMP
    return FieldType.TEXT


def _open_readonly_gpkg(data: bytes) -> tuple[sqlite3.Connection, Path]:
    if not data.startswith(_SQLITE_MAGIC):
        raise TransferFormatError("INVALID_GEOPACKAGE", "File is not a valid SQLite GeoPackage.")
    fd, tmp_name = tempfile.mkstemp(suffix=".gpkg")
    tmp_path = Path(tmp_name)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
        conn = sqlite3.connect(f"file:{tmp_path}?mode=ro", uri=True)
        conn.execute("PRAGMA trusted_schema=OFF")
        conn.execute("PRAGMA query_only=ON")
        conn.row_factory = sqlite3.Row
        tables = {
            row[0]
            for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table' AND upper(ltrim(sql)) NOT LIKE 'CREATE VIRTUAL TABLE%'").fetchall()
        }
        if "gpkg_contents" not in tables or "gpkg_geometry_columns" not in tables:
            conn.close()
            tmp_path.unlink(missing_ok=True)
            raise TransferFormatError(
                "INVALID_GEOPACKAGE",
                "SQLite database is missing required GeoPackage metadata tables.",
            )
        return conn, tmp_path
    except TransferFormatError:
        raise
    except Exception as exc:
        tmp_path.unlink(missing_ok=True)
        raise TransferFormatError("INVALID_GEOPACKAGE", "Unreadable GeoPackage database.") from exc


def _resolve_gpkg_epsg(conn: sqlite3.Connection, srs_id: int | None) -> int | None:
    if srs_id is None:
        return None
    try:
        row = conn.execute(
            "SELECT organization, organization_coordsys_id "
            "FROM gpkg_spatial_ref_sys WHERE srs_id = ?",
            (srs_id,),
        ).fetchone()
    except sqlite3.Error:
        row = None
    if row is not None:
        org = str(row["organization"] or "").upper()
        code = int(row["organization_coordsys_id"])
        if org == "EPSG" and code in SUPPORTED_CRS_CODES:
            return code
        return None
    return srs_id if srs_id in SUPPORTED_CRS_CODES else None


def inspect_geopackage(data: bytes) -> tuple[InspectedMember, ...]:
    conn, tmp_path = _open_readonly_gpkg(data)
    try:
        rows = conn.execute(
            """
            SELECT c.table_name, c.data_type, c.srs_id,
                   g.column_name AS geom_col, g.geometry_type_name, g.z, g.m
            FROM gpkg_contents c
            LEFT JOIN gpkg_geometry_columns g ON g.table_name = c.table_name
            ORDER BY c.table_name
            """
        ).fetchall()
        members: list[InspectedMember] = []
        for row in rows:
            tname = str(row["table_name"])
            dtype = str(row["data_type"] or "")
            if dtype != "features" or not row["geom_col"]:
                members.append(
                    InspectedMember(
                        name=tname,
                        eligible=False,
                        reason=f"Member data_type '{dtype}' is not a vector feature layer.",
                        geometry_family="unknown",
                        dimensions="unknown",
                        feature_count=0,
                        detected_crs=None,
                        detected_encoding="utf-8",
                        fields=(),
                    )
                )
                continue
            gtype_raw = str(row["geometry_type_name"] or "").upper()
            has_z = int(row["z"] or 0) > 0
            has_m = int(row["m"] or 0) > 0
            epsg = _resolve_gpkg_epsg(conn, row["srs_id"])
            if not conn.execute("SELECT 1 FROM sqlite_master WHERE name=? AND type='table' AND upper(ltrim(sql)) NOT LIKE 'CREATE VIRTUAL TABLE%'",(tname,)).fetchone():
                raise TransferFormatError("INVALID_GEOPACKAGE","Vector member must be an ordinary table.")
            count = int(conn.execute(f"SELECT count(*) FROM {_quote_ident(tname)}").fetchone()[0])
            cols = conn.execute(f"PRAGMA table_info({_quote_ident(tname)})").fetchall()
            geom_col = str(row["geom_col"])
            used: set[str] = set()
            fields: list[InspectedField] = []
            for col in cols:
                cname = str(col["name"])
                if cname == geom_col or int(col["pk"]) == 1 or cname == "ovg_feature_id":
                    continue
                if len(fields) >= MAX_TRANSFER_FIELDS:
                    break
                ftype = _map_sqlite_col_type(str(col["type"] or "TEXT"))
                fid = sanitize_field_id(cname, used, len(fields))
                fields.append(
                    InspectedField(
                        field_id=fid,
                        name=cname[:120] or fid,
                        type=ftype,
                        nullable=int(col["notnull"]) == 0,
                        ordinal=len(fields),
                        source_name=cname,
                    )
                )
            eligible = True
            reason: str | None = None
            family = _GPKG_FAMILY_MAP.get(gtype_raw)
            if has_z or has_m:
                eligible = False
                reason = "Only 2D XY vector layers are supported."
            elif family is None and gtype_raw != "GEOMETRY":
                eligible = False
                reason = f"Geometry type '{gtype_raw}' is outside supported single-part XY profile."
            elif count > MAX_TRANSFER_FEATURES:
                eligible = False
                reason = f"Feature count {count} exceeds limit of {MAX_TRANSFER_FEATURES}."
            members.append(
                InspectedMember(
                    name=tname,
                    eligible=eligible,
                    reason=reason,
                    geometry_family=(family.value if family else gtype_raw.lower()),
                    dimensions=("XYZ" if has_z else ("XYM" if has_m else "XY")),
                    feature_count=count,
                    detected_crs=epsg,
                    detected_encoding="utf-8",
                    fields=tuple(fields),
                )
            )
        return tuple(members)
    finally:
        conn.close()
        tmp_path.unlink(missing_ok=True)


def parse_geopackage_member(
    data: bytes, *, member_name: str, declared_crs: int | None = None
) -> ParsedMemberData:
    conn, tmp_path = _open_readonly_gpkg(data)
    try:
        meta = conn.execute(
            """
            SELECT c.table_name, c.srs_id, g.column_name AS geom_col,
                   g.geometry_type_name, g.z, g.m
            FROM gpkg_contents c
            JOIN gpkg_geometry_columns g ON g.table_name = c.table_name
            WHERE c.table_name = ? AND c.data_type = 'features'
            """,
            (member_name,),
        ).fetchone()
        if meta is None:
            raise TransferFormatError("UNKNOWN_MEMBER", "GeoPackage vector member not found.")
        detected_crs = _resolve_gpkg_epsg(conn, meta["srs_id"])
        effective_crs = detected_crs or declared_crs or 4326
        geom_col = str(meta["geom_col"])
        cols = conn.execute(f"PRAGMA table_info({_quote_ident(member_name)})").fetchall()
        pk_col = next((str(c["name"]) for c in cols if int(c["pk"]) == 1), None)
        has_ovg_fid = any(str(c["name"]) == "ovg_feature_id" for c in cols)
        used: set[str] = set()
        fields: list[InspectedField] = []
        for col in cols:
            cname = str(col["name"])
            if cname == geom_col or cname == pk_col or cname == "ovg_feature_id":
                continue
            ftype = _map_sqlite_col_type(str(col["type"] or "TEXT"))
            fid = sanitize_field_id(cname, used, len(fields))
            fields.append(
                InspectedField(
                    field_id=fid,
                    name=cname[:120] or fid,
                    type=ftype,
                    nullable=int(col["notnull"]) == 0,
                    ordinal=len(fields),
                    source_name=cname,
                )
            )

        findings: list[ValidationFinding] = []
        if len(fields) > MAX_TRANSFER_FIELDS:
            findings.append(
                ValidationFinding("error", "TOO_MANY_FIELDS", "Field count exceeds 100.")
            )
            fields = fields[:MAX_TRANSFER_FIELDS]

        select_cols = [_quote_ident(geom_col)]
        if pk_col:
            select_cols.append(_quote_ident(pk_col))
        if has_ovg_fid:
            select_cols.append(_quote_ident("ovg_feature_id"))
        select_cols.extend(_quote_ident(f.source_name) for f in fields)
        cursor = conn.execute(f"SELECT {', '.join(select_cols)} FROM {_quote_ident(member_name)}")

        records: list[ParsedFeatureRecord] = []
        families: set[GeometryFamily] = set()
        seen_fids: set[str] = set()
        xs: list[float] = []
        ys: list[float] = []
        has_non_2d = int(meta["z"] or 0) > 0 or int(meta["m"] or 0) > 0

        for idx, row in enumerate(cursor):
            if idx >= MAX_TRANSFER_FEATURES:
                findings.append(
                    ValidationFinding(
                        "error",
                        "FEATURE_LIMIT_EXCEEDED",
                        f"Feature count exceeds limit of {MAX_TRANSFER_FEATURES}.",
                    )
                )
                break
            blob = row[geom_col]
            if not isinstance(blob, (bytes, bytearray)):
                findings.append(
                    ValidationFinding("error", "NULL_GEOMETRY", "Feature has null geometry.", idx)
                )
                continue
            try:
                fam, coords, blob_srid, non_2d = _decode_gpkg_geom(bytes(blob))
            except ValueError as exc:
                findings.append(ValidationFinding("error", "INVALID_GEOMETRY", str(exc), idx))
                continue
            if non_2d:
                has_non_2d = True
                findings.append(
                    ValidationFinding(
                        "error",
                        "UNSUPPORTED_DIMENSIONS",
                        "Only 2D XY geometries are supported.",
                        idx,
                    )
                )
                continue
            if detected_crs is not None and blob_srid not in (0, meta["srs_id"], detected_crs):
                findings.append(
                    ValidationFinding(
                        "error", "CRS_MISMATCH", "Feature SRID disagrees with layer CRS.", idx
                    )
                )
                continue
            families.add(fam)

            raw_fid = row["ovg_feature_id"] if has_ovg_fid else (row[pk_col] if pk_col else None)
            if raw_fid is not None and _FEAT_ID_RE.match(str(raw_fid)):
                fid_str = str(raw_fid)
            else:
                fid_str = f"feat-{idx + 1:06d}"
            if fid_str in seen_fids:
                fid_str = f"feat-{idx + 1:06d}"
            seen_fids.add(fid_str)

            attrs: dict[str, AttributeValue] = {}
            for fld in fields:
                raw_val = row[fld.source_name]
                try:
                    attrs[fld.field_id] = _coerce_gpkg_value(fld.type, raw_val)
                except ValueError as exc:
                    findings.append(
                        ValidationFinding(
                            "error", "INVALID_ATTRIBUTE_VALUE", str(exc), idx, fld.source_name
                        )
                    )
            geom = NativeGeometry(fam.value, coords, effective_crs)
            records.append(ParsedFeatureRecord(fid_str, attrs, geom))
            _collect_xy(coords, xs, ys)

        if len(families) > 1:
            findings.append(
                ValidationFinding(
                    "error", "MIXED_GEOMETRY_FAMILY", "Mixed geometry families are not supported."
                )
            )
        declared_fam = _GPKG_FAMILY_MAP.get(str(meta["geometry_type_name"] or "").upper())
        family = (
            next(iter(families))
            if len(families) == 1
            else (
                GeometryFamily.MIXED
                if len(families) > 1
                else (declared_fam or GeometryFamily.POINT)
            )
        )
        extent = (
            {
                "min_x": min(xs),
                "min_y": min(ys),
                "max_x": max(xs),
                "max_y": max(ys),
                "crs_code": effective_crs,
            }
            if xs and ys
            else None
        )
        return ParsedMemberData(
            member_name=member_name,
            geometry_family=family,
            dimensions=GeometryDimensions.XYZ if has_non_2d else GeometryDimensions.XY,
            detected_crs=detected_crs,
            detected_encoding="utf-8",
            requires_encoding_resolution=False,
            fields=tuple(fields),
            features=tuple(records),
            findings=tuple(findings),
            extent=extent,
        )
    finally:
        conn.close()
        tmp_path.unlink(missing_ok=True)


# The canonical decimal check allows 38 significant digits; the default context (28) would round.
_EXACT_DECIMAL = Context(prec=38)


def _coerce_gpkg_value(ftype: FieldType, val: Any) -> AttributeValue:
    if val is None:
        return None
    if ftype == FieldType.BOOLEAN:
        if isinstance(val, bool):
            return val
        if isinstance(val, int) and val in (0, 1):
            return bool(val)
        raise ValueError("expected boolean")
    if ftype == FieldType.INTEGER:
        if isinstance(val, bool) or not isinstance(val, int):
            raise ValueError("expected integer")
        return val
    if ftype == FieldType.DECIMAL:
        if isinstance(val, bool):
            raise ValueError("expected decimal")
        # format(..., "f"): normalize() alone gives "2E+1" for 20.0, which the canonical
        # base-10 decimal check rejects, so any GeoPackage with round decimals failed to import.
        if isinstance(val, (int, float)):
            if not math.isfinite(float(val)):
                raise ValueError("non-finite decimal")
            return format(Decimal(str(val)).normalize(_EXACT_DECIMAL), "f")
        if isinstance(val, str):
            return format(Decimal(val).normalize(_EXACT_DECIMAL), "f")
        raise ValueError("expected decimal")
    if ftype == FieldType.TIMESTAMP:
        if not isinstance(val, str) or not _ISO_TS_RE.match(val):
            raise ValueError("expected ISO-8601 timestamp")
        parsed = datetime.fromisoformat(val.replace("Z", "+00:00"))
        return parsed.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    if isinstance(val, (bytes, bytearray)):
        raise ValueError("binary attribute is not supported")
    text = str(val)
    if len(text) > MAX_STRING_LEN:
        raise ValueError("text exceeds 4096 characters")
    return text


def _collect_xy(coords: Any, xs: list[float], ys: list[float]) -> None:
    if isinstance(coords, tuple) and coords:
        if isinstance(coords[0], (int, float)):
            xs.append(float(coords[0]))
            ys.append(float(coords[1]))
        else:
            for child in coords:
                _collect_xy(child, xs, ys)


def write_geopackage(
    *,
    layer_name: str,
    crs_code: int,
    geometry_family: str,
    field_mappings: Sequence[ExportFieldMapping],
    features: Sequence[tuple[str, dict[str, Any], NativeGeometry]],
) -> tuple[bytes, str, str]:
    table_name = re.sub(r"[^A-Za-z0-9_]+", "_", layer_name.strip().lower()).strip("_")
    if not table_name or not table_name[0].isalpha():
        table_name = f"layer_{table_name or 'export'}"
    table_name = table_name[:60]

    gpkg_geom_type = {
        "Point": "POINT",
        "LineString": "LINESTRING",
        "Polygon": "POLYGON",
    }[geometry_family]

    fd, tmp_name = tempfile.mkstemp(suffix=".gpkg")
    os.close(fd)
    tmp_path = Path(tmp_name)
    try:
        conn = sqlite3.connect(str(tmp_path))
        try:
            conn.execute("PRAGMA application_id = 1196444487")  # 0x47504B47 'GPKG'
            conn.execute("PRAGMA user_version = 10300")  # GeoPackage 1.3
            conn.executescript(
                """
                CREATE TABLE gpkg_spatial_ref_sys (
                    srs_name TEXT NOT NULL,
                    srs_id INTEGER NOT NULL PRIMARY KEY,
                    organization TEXT NOT NULL,
                    organization_coordsys_id INTEGER NOT NULL,
                    definition TEXT NOT NULL,
                    description TEXT
                );
                CREATE TABLE gpkg_contents (
                    table_name TEXT NOT NULL PRIMARY KEY,
                    data_type TEXT NOT NULL,
                    identifier TEXT UNIQUE,
                    description TEXT DEFAULT '',
                    last_change DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                    min_x DOUBLE,
                    min_y DOUBLE,
                    max_x DOUBLE,
                    max_y DOUBLE,
                    srs_id INTEGER,
                    CONSTRAINT fk_gc_r_srs_id FOREIGN KEY (srs_id)
                        REFERENCES gpkg_spatial_ref_sys(srs_id)
                );
                CREATE TABLE gpkg_geometry_columns (
                    table_name TEXT NOT NULL,
                    column_name TEXT NOT NULL,
                    geometry_type_name TEXT NOT NULL,
                    srs_id INTEGER NOT NULL,
                    z TINYINT NOT NULL,
                    m TINYINT NOT NULL,
                    CONSTRAINT pk_geom_cols PRIMARY KEY (table_name, column_name),
                    CONSTRAINT uk_gc_table_name UNIQUE (table_name),
                    CONSTRAINT fk_gc_tn FOREIGN KEY (table_name)
                        REFERENCES gpkg_contents(table_name),
                    CONSTRAINT fk_gc_srs FOREIGN KEY (srs_id)
                        REFERENCES gpkg_spatial_ref_sys(srs_id)
                );
                """
            )
            srs_rows = [
                ("Undefined cartesian SRS", -1, "NONE", -1, "undefined", "undefined"),
                ("Undefined geographic SRS", 0, "NONE", 0, "undefined", "undefined"),
            ]
            for code, wkt in EPSG_WKT1.items():
                srs_rows.append((EPSG_NAMES[code], code, "EPSG", code, wkt, EPSG_NAMES[code]))
            conn.executemany("INSERT INTO gpkg_spatial_ref_sys VALUES (?,?,?,?,?,?)", srs_rows)

            col_defs = [
                "fid INTEGER PRIMARY KEY AUTOINCREMENT",
                "geom BLOB",
                "ovg_feature_id TEXT NOT NULL",
            ]
            sql_type_map = {
                FieldType.TEXT: "TEXT",
                FieldType.INTEGER: "INTEGER",
                FieldType.DECIMAL: "REAL",
                FieldType.BOOLEAN: "BOOLEAN",
                FieldType.TIMESTAMP: "DATETIME",
            }
            for m in field_mappings:
                col_defs.append(f"{_quote_ident(m.output_name)} {sql_type_map[m.type]}")
            conn.execute(f"CREATE TABLE {_quote_ident(table_name)} ({', '.join(col_defs)})")

            xs: list[float] = []
            ys: list[float] = []
            placeholders = ", ".join(["?"] * (2 + len(field_mappings)))
            col_list = ", ".join(
                ["geom", "ovg_feature_id"] + [_quote_ident(m.output_name) for m in field_mappings]
            )
            insert_sql = (
                f"INSERT INTO {_quote_ident(table_name)} ({col_list}) VALUES ({placeholders})"
            )
            rows_to_insert = []
            for fid, attrs, geom in features:
                _collect_xy(geom.coordinates, xs, ys)
                vals: list[Any] = [_encode_gpkg_geom(geom, crs_code), fid]
                for m in field_mappings:
                    v = attrs.get(m.field_id)
                    if v is None:
                        vals.append(None)
                    elif m.type == FieldType.BOOLEAN:
                        vals.append(1 if v else 0)
                    elif m.type == FieldType.DECIMAL:
                        vals.append(float(Decimal(str(v))))
                    else:
                        vals.append(v)
                rows_to_insert.append(tuple(vals))
            if rows_to_insert:
                conn.executemany(insert_sql, rows_to_insert)

            conn.execute(
                """
                INSERT INTO gpkg_contents
                    (table_name, data_type, identifier, description,
                     min_x, min_y, max_x, max_y, srs_id)
                VALUES (?, 'features', ?, '', ?, ?, ?, ?, ?)
                """,
                (
                    table_name,
                    layer_name,
                    min(xs) if xs else None,
                    min(ys) if ys else None,
                    max(xs) if xs else None,
                    max(ys) if ys else None,
                    crs_code,
                ),
            )
            conn.execute(
                """
                INSERT INTO gpkg_geometry_columns
                    (table_name, column_name, geometry_type_name, srs_id, z, m)
                VALUES (?, 'geom', ?, ?, 0, 0)
                """,
                (table_name, gpkg_geom_type, crs_code),
            )
            conn.commit()
        finally:
            conn.close()
        return tmp_path.read_bytes(), "gpkg", "application/geopackage+sqlite3"
    finally:
        tmp_path.unlink(missing_ok=True)
