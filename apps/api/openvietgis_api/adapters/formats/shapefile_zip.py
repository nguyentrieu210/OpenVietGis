"""ZIP-packaged ESRI Shapefile reader and writer using pure-Python pyshp."""

from __future__ import annotations

import datetime as dt
import io
import math
import re
import zipfile
from collections.abc import Sequence
from decimal import Decimal
from typing import Any

import shapefile  # type: ignore[import-untyped]

from openvietgis_api.adapters.formats.wkt_crs import EPSG_WKT1, parse_prj_to_epsg
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
    MAX_ZIP_ENTRIES,
    MAX_ZIP_EXPANDED_BYTES,
    ExportFieldMapping,
    InspectedField,
    InspectedMember,
    ParsedFeatureRecord,
    ParsedMemberData,
    ValidationFinding,
    normalize_encoding,
    sanitize_field_id,
)
from openvietgis_api.domains.transfers.ports import (
    TransferFormatError,
    TransferLimitExceededError,
)

_FEAT_ID_RE = re.compile(r"^[A-Za-z0-9_-]{1,128}$")
_ISO_TS_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$")

_SHP_TYPE_FAMILY: dict[int, GeometryFamily] = {
    shapefile.POINT: GeometryFamily.POINT,
    shapefile.POLYLINE: GeometryFamily.LINE_STRING,
    shapefile.POLYGON: GeometryFamily.POLYGON,
}


def _extract_safe_zip(data: bytes) -> dict[str, bytes]:
    """Safely inspect & extract ZIP archive entries with strict traversal/bomb limits."""
    try:
        zf = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as exc:
        raise TransferFormatError("INVALID_ZIP", "Corrupt or invalid ZIP archive.") from exc

    with zf:
        infos = zf.infolist()
        if len(infos) > MAX_ZIP_ENTRIES:
            raise TransferLimitExceededError(
                "ARCHIVE_ENTRY_LIMIT_EXCEEDED",
                f"Archive entry count exceeds maximum of {MAX_ZIP_ENTRIES}.",
            )
        total_expanded = 0
        files: dict[str, bytes] = {}
        seen_names: set[str] = set()
        for info in infos:
            if info.is_dir():
                continue
            name = info.filename.replace("\\", "/")
            if name.startswith("/") or ".." in name.split("/") or ":" in name:
                raise TransferFormatError(
                    "ARCHIVE_PATH_TRAVERSAL", "Archive contains unsafe entry path."
                )
            # Reject symlinks (UNIX mode 0o120000 in upper 16 bits)
            if ((info.external_attr >> 16) & 0o170000) == 0o120000:
                raise TransferFormatError(
                    "ARCHIVE_SYMLINK_REJECTED", "Archive symbolic links are not permitted."
                )
            if info.flag_bits & 0x1:
                raise TransferFormatError(
                    "ARCHIVE_ENCRYPTED", "Encrypted ZIP archives are not supported."
                )
            if info.file_size > MAX_ZIP_EXPANDED_BYTES:
                raise TransferLimitExceededError(
                    "ARCHIVE_SIZE_EXCEEDED", "Archive entry exceeds expanded size limit."
                )
            chunks = []
            read_total = 0
            with zf.open(info, "r") as handle:
                while True:
                    chunk = handle.read(65536)
                    if not chunk:
                        break
                    read_total += len(chunk)
                    total_expanded += len(chunk)
                    if total_expanded > MAX_ZIP_EXPANDED_BYTES:
                        raise TransferLimitExceededError(
                            "ARCHIVE_BOMB_REJECTED",
                            "Archive expansion exceeds safe byte budget.",
                        )
                    chunks.append(chunk)
            lower_key = name.lower()
            if lower_key in seen_names:
                raise TransferFormatError(
                    "ARCHIVE_DUPLICATE_ENTRY", "Archive contains duplicate member names."
                )
            seen_names.add(lower_key)
            files[name] = b"".join(chunks)
        return files


def _group_shapefile_stems(files: dict[str, bytes]) -> dict[str, dict[str, bytes]]:
    groups: dict[str, dict[str, bytes]] = {}
    for path, content in files.items():
        if path.startswith("__MACOSX/") or path.rsplit("/", 1)[-1].startswith("._"):
            continue
        if "." not in path:
            continue
        stem, ext = path.rsplit(".", 1)
        ext_low = ext.lower()
        if ext_low in ("shp", "shx", "dbf", "prj", "cpg"):
            groups.setdefault(stem, {})[ext_low] = content
    return groups


def _dbf_has_non_ascii(dbf_bytes: bytes) -> bool:
    # Header size is stored at bytes 8..10 (little-endian uint16)
    if len(dbf_bytes) < 32:
        return False
    header_len = int.from_bytes(dbf_bytes[8:10], "little")
    body = dbf_bytes[header_len:]
    return any(b >= 0x80 for b in body)


def _map_dbf_field(ftype: str, decimal_len: int) -> FieldType:
    if ftype == "L":
        return FieldType.BOOLEAN
    if ftype == "N" and decimal_len == 0:
        return FieldType.INTEGER
    if ftype in ("N", "F"):
        return FieldType.DECIMAL
    if ftype == "D":
        return FieldType.TEXT
    return FieldType.TEXT


def inspect_shapefile_zip(
    data: bytes, *, encoding: str | None = None
) -> tuple[InspectedMember, ...]:
    files = _extract_safe_zip(data)
    groups = _group_shapefile_stems(files)
    if not groups:
        raise TransferFormatError(
            "MISSING_SHAPEFILE_COMPONENTS",
            "ZIP archive does not contain Shapefile (.shp/.shx/.dbf) components.",
        )
    members: list[InspectedMember] = []
    for stem, parts in sorted(groups.items()):
        missing = [ext for ext in ("shp", "shx", "dbf") if ext not in parts]
        if missing:
            members.append(
                InspectedMember(
                    name=stem,
                    eligible=False,
                    reason=f"Incomplete Shapefile set; missing: {', '.join(missing)}.",
                    geometry_family="unknown",
                    dimensions="unknown",
                    feature_count=0,
                    detected_crs=None,
                    detected_encoding=None,
                    fields=(),
                )
            )
            continue
        cpg_enc = (
            normalize_encoding(parts["cpg"].decode("ascii", errors="ignore"))
            if "cpg" in parts
            else None
        )
        effective_enc = cpg_enc or encoding or "latin-1"
        prj_crs = (
            parse_prj_to_epsg(parts["prj"].decode("utf-8", errors="ignore"))
            if "prj" in parts
            else None
        )
        try:
            reader = shapefile.Reader(
                shp=io.BytesIO(parts["shp"]),
                shx=io.BytesIO(parts["shx"]),
                dbf=io.BytesIO(parts["dbf"]),
                encoding=effective_enc,
                encodingErrors="strict",
            )
        except Exception:
            members.append(
                InspectedMember(
                    name=stem,
                    eligible=False,
                    reason="Shapefile components could not be decoded with the selected encoding.",
                    geometry_family="unknown",
                    dimensions="unknown",
                    feature_count=0,
                    detected_crs=prj_crs,
                    detected_encoding=cpg_enc,
                    fields=(),
                )
            )
            continue
        fam = _SHP_TYPE_FAMILY.get(reader.shapeType)
        count = len(reader)
        used: set[str] = set()
        fields: list[InspectedField] = []
        for fdesc in reader.fields[1:]:
            cname = str(fdesc[0])
            if cname.upper() == "OVG_FID":
                continue
            if len(fields) >= MAX_TRANSFER_FIELDS:
                break
            ft = _map_dbf_field(str(fdesc[1]), int(fdesc[3]))
            fid = sanitize_field_id(cname, used, len(fields))
            fields.append(
                InspectedField(
                    field_id=fid,
                    name=cname,
                    type=ft,
                    nullable=True,
                    ordinal=len(fields),
                    source_name=cname,
                )
            )
        eligible = fam is not None and count <= MAX_TRANSFER_FEATURES
        reason = None
        if fam is None:
            reason = f"Unsupported Shapefile shapeType {reader.shapeType}."
        elif count > MAX_TRANSFER_FEATURES:
            reason = f"Feature count {count} exceeds limit of {MAX_TRANSFER_FEATURES}."
        members.append(
            InspectedMember(
                name=stem,
                eligible=eligible,
                reason=reason,
                geometry_family=(fam.value if fam else "unsupported"),
                dimensions="XY",
                feature_count=count,
                detected_crs=prj_crs,
                detected_encoding=cpg_enc,
                fields=tuple(fields),
            )
        )
    return tuple(members)


def parse_shapefile_member(
    data: bytes,
    *,
    member_name: str,
    declared_crs: int | None = None,
    declared_encoding: str | None = None,
) -> ParsedMemberData:
    files = _extract_safe_zip(data)
    groups = _group_shapefile_stems(files)
    parts = groups.get(member_name)
    if not parts or any(ext not in parts for ext in ("shp", "shx", "dbf")):
        raise TransferFormatError("UNKNOWN_MEMBER", "Selected Shapefile member is incomplete.")

    cpg_enc = (
        normalize_encoding(parts["cpg"].decode("ascii", errors="ignore"))
        if "cpg" in parts
        else None
    )
    has_non_ascii = _dbf_has_non_ascii(parts["dbf"])
    requires_encoding = False
    if cpg_enc is not None:
        effective_enc = cpg_enc
    elif declared_encoding is not None:
        effective_enc = declared_encoding
    elif has_non_ascii:
        requires_encoding = True
        effective_enc = "latin-1"
    else:
        effective_enc = "utf-8"

    prj_crs = (
        parse_prj_to_epsg(parts["prj"].decode("utf-8", errors="ignore")) if "prj" in parts else None
    )
    effective_crs = prj_crs or declared_crs or 4326
    findings: list[ValidationFinding] = []
    if cpg_enc and declared_encoding and cpg_enc != declared_encoding:
        findings.append(
            ValidationFinding(
                "error",
                "ENCODING_CONFLICT",
                f"Declared encoding '{declared_encoding}' conflicts with .cpg '{cpg_enc}'.",
            )
        )

    try:
        reader = shapefile.Reader(
            shp=io.BytesIO(parts["shp"]),
            shx=io.BytesIO(parts["shx"]),
            dbf=io.BytesIO(parts["dbf"]),
            encoding=effective_enc,
            encodingErrors="strict",
        )
    except UnicodeDecodeError:
        findings.append(
            ValidationFinding(
                "error",
                "ENCODING_DECODE_FAILED",
                f"DBF attributes could not be decoded cleanly as '{effective_enc}'.",
            )
        )
        return ParsedMemberData(
            member_name=member_name,
            geometry_family=GeometryFamily.POINT,
            dimensions=GeometryDimensions.XY,
            detected_crs=prj_crs,
            detected_encoding=cpg_enc or declared_encoding,
            requires_encoding_resolution=requires_encoding,
            fields=(),
            features=(),
            findings=tuple(findings),
            extent=None,
        )

    fam = _SHP_TYPE_FAMILY.get(reader.shapeType)
    if fam is None:
        findings.append(
            ValidationFinding(
                "error",
                "UNSUPPORTED_GEOMETRY",
                f"Shapefile shapeType {reader.shapeType} is outside 2D Point/LineString/Polygon.",
            )
        )
        return ParsedMemberData(
            member_name=member_name,
            geometry_family=GeometryFamily.UNSUPPORTED,
            dimensions=GeometryDimensions.XY,
            detected_crs=prj_crs,
            detected_encoding=cpg_enc
            or declared_encoding
            or ("utf-8" if not requires_encoding else None),
            requires_encoding_resolution=requires_encoding,
            fields=(),
            features=(),
            findings=tuple(findings),
            extent=None,
        )

    raw_fdescs = reader.fields[1:]
    used: set[str] = set()
    fields: list[InspectedField] = []
    ovg_fid_idx: int | None = None
    field_col_indices: list[tuple[int, InspectedField]] = []
    for col_idx, fdesc in enumerate(raw_fdescs):
        cname = str(fdesc[0])
        if cname.upper() == "OVG_FID":
            ovg_fid_idx = col_idx
            continue
        if len(fields) >= MAX_TRANSFER_FIELDS:
            findings.append(
                ValidationFinding("error", "TOO_MANY_FIELDS", "Field count exceeds 100.")
            )
            break
        ft = _map_dbf_field(str(fdesc[1]), int(fdesc[3]))
        fid = sanitize_field_id(cname, used, len(fields))
        inspected = InspectedField(
            field_id=fid,
            name=cname,
            type=ft,
            nullable=True,
            ordinal=len(fields),
            source_name=cname,
        )
        fields.append(inspected)
        field_col_indices.append((col_idx, inspected))

    records: list[ParsedFeatureRecord] = []
    seen_fids: set[str] = set()
    xs: list[float] = []
    ys: list[float] = []

    for idx, sr in enumerate(reader.iterShapeRecords()):
        if idx >= MAX_TRANSFER_FEATURES:
            findings.append(
                ValidationFinding(
                    "error",
                    "FEATURE_LIMIT_EXCEEDED",
                    f"Feature count exceeds limit of {MAX_TRANSFER_FEATURES}.",
                )
            )
            break
        shp = sr.shape
        if shp.shapeType == shapefile.NULL or not shp.points:
            findings.append(
                ValidationFinding("error", "NULL_GEOMETRY", "Feature has null/empty geometry.", idx)
            )
            continue
        if shp.shapeType != reader.shapeType:
            findings.append(ValidationFinding("error", "UNSUPPORTED_GEOMETRY", "Shape type disagrees with member.", idx))
            continue
        for pt in shp.points:
            if not (math.isfinite(pt[0]) and math.isfinite(pt[1])):
                findings.append(
                    ValidationFinding("error", "INVALID_GEOMETRY", "Non-finite coordinate.", idx)
                )
                break
        if fam == GeometryFamily.POINT:
            coords: Any = (float(shp.points[0][0]), float(shp.points[0][1]))
            xs.append(coords[0])
            ys.append(coords[1])
        elif fam == GeometryFamily.LINE_STRING:
            if len(shp.parts) > 1:
                findings.append(
                    ValidationFinding(
                        "error", "UNSUPPORTED_GEOMETRY", "Multipart LineString is unsupported.", idx
                    )
                )
                continue
            coords = tuple((float(p[0]), float(p[1])) for p in shp.points)
            for p in coords:
                xs.append(p[0])
                ys.append(p[1])
        else:
            parts_idx = list(shp.parts) + [len(shp.points)]
            rings = []
            for start, end in zip(parts_idx[:-1], parts_idx[1:], strict=True):
                ring = tuple((float(p[0]), float(p[1])) for p in shp.points[start:end])
                if len(ring) < 4:
                    findings.append(
                        ValidationFinding(
                            "error", "INVALID_GEOMETRY", "Degenerate polygon ring.", idx
                        )
                    )
                    continue
                if ring[0] != ring[-1]:
                    ring = ring + (ring[0],)
                rings.append(ring)
                for p in ring:
                    xs.append(p[0])
                    ys.append(p[1])
            coords = tuple(rings)

        rec_vals = list(sr.record)
        raw_fid = rec_vals[ovg_fid_idx] if ovg_fid_idx is not None else None
        if raw_fid is not None and _FEAT_ID_RE.match(str(raw_fid).strip()):
            fid_str = str(raw_fid).strip()
        else:
            fid_str = f"feat-{idx + 1:06d}"
        if fid_str in seen_fids:
            fid_str = f"feat-{idx + 1:06d}"
        seen_fids.add(fid_str)

        attrs: dict[str, AttributeValue] = {}
        for col_idx, fld in field_col_indices:
            raw_v = rec_vals[col_idx]
            if raw_v is None:
                attrs[fld.field_id] = None
            elif fld.type == FieldType.BOOLEAN:
                attrs[fld.field_id] = bool(raw_v)
            elif fld.type == FieldType.INTEGER:
                attrs[fld.field_id] = int(raw_v)
            elif fld.type == FieldType.DECIMAL:
                attrs[fld.field_id] = format(Decimal(str(raw_v)), "f")
            elif isinstance(raw_v, dt.date):
                attrs[fld.field_id] = raw_v.isoformat()
            else:
                text = str(raw_v)
                if len(text) > MAX_STRING_LEN:
                    findings.append(
                        ValidationFinding(
                            "error",
                            "INVALID_ATTRIBUTE_VALUE",
                            "Text exceeds 4096 chars.",
                            idx,
                            fld.source_name,
                        )
                    )
                attrs[fld.field_id] = text
        records.append(
            ParsedFeatureRecord(fid_str, attrs, NativeGeometry(fam.value, coords, effective_crs))
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
    resolved_enc = cpg_enc or declared_encoding or (None if requires_encoding else "utf-8")
    return ParsedMemberData(
        member_name=member_name,
        geometry_family=fam,
        dimensions=GeometryDimensions.XY,
        detected_crs=prj_crs,
        detected_encoding=resolved_enc,
        requires_encoding_resolution=requires_encoding,
        fields=tuple(fields),
        features=tuple(records),
        findings=tuple(findings),
        extent=extent,
    )


def write_shapefile_zip(
    *,
    layer_name: str,
    crs_code: int,
    geometry_family: str,
    field_mappings: Sequence[ExportFieldMapping],
    features: Sequence[tuple[str, dict[str, Any], NativeGeometry]],
) -> tuple[bytes, str, str]:
    stem = re.sub(r"[^A-Za-z0-9_]+", "_", layer_name.strip().lower()).strip("_") or "export"
    stem = stem[:40]
    shape_type = {
        "Point": shapefile.POINT,
        "LineString": shapefile.POLYLINE,
        "Polygon": shapefile.POLYGON,
    }[geometry_family]

    shp_buf = io.BytesIO()
    shx_buf = io.BytesIO()
    dbf_buf = io.BytesIO()
    writer = shapefile.Writer(
        shp=shp_buf, shx=shx_buf, dbf=dbf_buf, shapeType=shape_type, encoding="utf-8"
    )
    writer.field("OVG_FID", "C", size=128)
    for m in field_mappings:
        if m.type == FieldType.INTEGER:
            writer.field(m.output_name, "N", size=12, decimal=0)
        elif m.type == FieldType.DECIMAL:
            writer.field(m.output_name, "N", size=19, decimal=6)
        elif m.type == FieldType.BOOLEAN:
            writer.field(m.output_name, "L", size=1)
        elif m.type == FieldType.TIMESTAMP:
            writer.field(m.output_name, "C", size=40)
        else:
            writer.field(m.output_name, "C", size=254)

    for fid, attrs, geom in features:
        gcoords: Any = geom.coordinates
        if geom.type == "Point":
            writer.point(float(gcoords[0]), float(gcoords[1]))
        elif geom.type == "LineString":
            writer.line([[[float(p[0]), float(p[1])] for p in gcoords]])
        else:
            writer.poly([[[float(p[0]), float(p[1])] for p in ring] for ring in gcoords])
        rec: list[Any] = [fid]
        for m in field_mappings:
            v = attrs.get(m.field_id)
            if v is None:
                rec.append(None)
            elif m.type == FieldType.DECIMAL:
                rec.append(float(Decimal(str(v))))
            elif m.type == FieldType.TEXT:
                rec.append(str(v)[:254])
            else:
                rec.append(v)
        writer.record(*rec)
    writer.close()

    zip_buf = io.BytesIO()
    with zipfile.ZipFile(zip_buf, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr(f"{stem}.shp", shp_buf.getvalue())
        zf.writestr(f"{stem}.shx", shx_buf.getvalue())
        zf.writestr(f"{stem}.dbf", dbf_buf.getvalue())
        zf.writestr(f"{stem}.prj", EPSG_WKT1[crs_code].encode("utf-8"))
        zf.writestr(f"{stem}.cpg", b"UTF-8")
    return zip_buf.getvalue(), "zip", "application/zip"
