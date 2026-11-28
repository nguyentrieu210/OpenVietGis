"""Isolated E15 byte-format adapter. No database, HTTP, credentials or publication."""
from __future__ import annotations
import csv
import io
import json
import resource
import sys
import zipfile
from dataclasses import dataclass
from datetime import date, datetime
from hashlib import sha256
from html import escape
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.parse import urlencode
from uuid import UUID
from xml.etree import ElementTree as ET

resource.setrlimit(resource.RLIMIT_AS, (768 * 1024 * 1024,) * 2)
resource.setrlimit(resource.RLIMIT_CPU, (25, 30))
resource.setrlimit(resource.RLIMIT_FSIZE, (64 * 1024 * 1024,) * 2)
import ezdxf
import shapefile
import qrcode
from PIL import Image, ImageDraw, ImageFont

class PlatformValidationError(Exception):
    def __init__(self, message, code="VALIDATION_ERROR"):
        super().__init__(message)
        self.code = code

class PlatformConflictError(PlatformValidationError):
    pass

_ALLOWED_FORMATS = {"csv", "xlsx", "geojson", "dxf", "shapefile_zip"}
_ALLOWED_ORIGINS = {"certificate", "cad", "survey", "legacy", "manual"}
_ALLOWED_CONFIDENCE = {"authoritative", "observed", "estimated"}


@dataclass(frozen=True, slots=True)
class ParsedImport:
    rows: list[dict[str, Any]]
    encoding: str | None


def _decode_text(data: bytes) -> tuple[str, str]:
    for encoding in ("utf-8-sig", "utf-8", "cp1258"):
        try:
            return data.decode(encoding), encoding
        except UnicodeDecodeError:
            continue
    raise PlatformValidationError("unsupported text encoding", code="IMPORT_ENCODING")


def _parse_csv(data: bytes) -> ParsedImport:
    decoded, encoding = _decode_text(data)
    sample = decoded[:8192]
    try:
        dialect = csv.Sniffer().sniff(sample, delimiters=",;\t|")
    except csv.Error:
        dialect = csv.excel
    reader = csv.DictReader(io.StringIO(decoded), dialect=dialect)
    if not reader.fieldnames:
        raise PlatformValidationError("CSV has no header", code="IMPORT_HEADER")
    rows = [{str(k): v for k, v in row.items()} for row in reader]
    return ParsedImport(rows, encoding)


def _xlsx_shared_strings(archive: zipfile.ZipFile) -> list[str]:
    try:
        xml = archive.read("xl/sharedStrings.xml")
    except KeyError:
        return []
    root = ET.fromstring(xml)
    ns = {"x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
    result: list[str] = []
    for si in root.findall("x:si", ns):
        result.append("".join(t.text or "" for t in si.findall(".//x:t", ns)))
    return result


def _xlsx_cell_value(cell: ET.Element, shared: list[str], ns: dict[str, str]) -> str:
    cell_type = cell.attrib.get("t")
    if cell_type == "inlineStr":
        return "".join(t.text or "" for t in cell.findall(".//x:t", ns))
    value = cell.find("x:v", ns)
    raw = "" if value is None or value.text is None else value.text
    if cell_type == "s" and raw:
        try:
            return shared[int(raw)]
        except (ValueError, IndexError):
            return ""
    return raw


def _parse_xlsx(data: bytes) -> ParsedImport:
    try:
        archive = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as exc:
        raise PlatformValidationError(
            "invalid XLSX archive", code="IMPORT_XLSX"
        ) from exc
    with archive:
        try:
            sheet_xml = archive.read("xl/worksheets/sheet1.xml")
        except KeyError as exc:
            raise PlatformValidationError(
                "XLSX first sheet missing", code="IMPORT_XLSX"
            ) from exc
        shared = _xlsx_shared_strings(archive)
        root = ET.fromstring(sheet_xml)
        ns = {"x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
        matrix: list[list[str]] = []
        for row in root.findall(".//x:sheetData/x:row", ns):
            values = [
                _xlsx_cell_value(cell, shared, ns) for cell in row.findall("x:c", ns)
            ]
            matrix.append(values)
    if not matrix or not any(str(v).strip() for v in matrix[0]):
        raise PlatformValidationError("XLSX has no header", code="IMPORT_HEADER")
    header = [str(v).strip() for v in matrix[0]]
    rows: list[dict[str, Any]] = []
    for values in matrix[1:]:
        padded = values + [""] * (len(header) - len(values))
        rows.append({header[i]: padded[i] for i in range(len(header)) if header[i]})
    return ParsedImport(rows, None)


def _parse_geojson(data: bytes) -> ParsedImport:
    decoded, encoding = _decode_text(data)
    try:
        payload = json.loads(decoded)
    except json.JSONDecodeError as exc:
        raise PlatformValidationError("invalid GeoJSON", code="IMPORT_GEOJSON") from exc
    if payload.get("type") != "FeatureCollection" or not isinstance(
        payload.get("features"), list
    ):
        raise PlatformValidationError(
            "GeoJSON FeatureCollection required", code="IMPORT_GEOJSON"
        )
    rows: list[dict[str, Any]] = []
    for feature in payload["features"]:
        properties = dict(feature.get("properties") or {})
        geometry = feature.get("geometry")
        properties["_geometry"] = geometry
        if isinstance(geometry, dict) and geometry.get("type") == "Point":
            coords = geometry.get("coordinates")
            if isinstance(coords, list) and len(coords) >= 2:
                properties["_x"] = coords[0]
                properties["_y"] = coords[1]
        rows.append(properties)
    return ParsedImport(rows, encoding)


def _parse_dxf(data: bytes) -> ParsedImport:
    decoded, encoding = _decode_text(data)
    try:
        doc = ezdxf.read(io.StringIO(decoded))
    except Exception as exc:
        raise PlatformValidationError("invalid DXF", code="IMPORT_DXF") from exc
    rows: list[dict[str, Any]] = []
    for entity in doc.modelspace():
        kind = entity.dxftype()
        if kind == "TEXT":
            point = entity.dxf.insert
            rows.append(
                {
                    "raw_number": entity.dxf.text,
                    "_x": float(point.x),
                    "_y": float(point.y),
                    "_entity": kind,
                }
            )
        elif kind == "MTEXT":
            point = entity.dxf.insert
            rows.append(
                {
                    "raw_number": entity.plain_text(),
                    "_x": float(point.x),
                    "_y": float(point.y),
                    "_entity": kind,
                }
            )
        elif kind == "INSERT":
            attrs = {attr.dxf.tag: attr.dxf.text for attr in entity.attribs}
            point = entity.dxf.insert
            attrs.update({"_x": float(point.x), "_y": float(point.y), "_entity": kind})
            rows.append(attrs)
    return ParsedImport(rows, encoding)


def _parse_shapefile_zip(data: bytes) -> ParsedImport:
    try:
        archive = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as exc:
        raise PlatformValidationError(
            "invalid shapefile zip", code="IMPORT_SHP"
        ) from exc
    with archive:
        names = archive.namelist()
        shp_names = [n for n in names if PurePosixPath(n).suffix.lower() == ".shp"]
        if len(shp_names) != 1:
            raise PlatformValidationError(
                "zip must contain exactly one .shp", code="IMPORT_SHP"
            )
        base = str(PurePosixPath(shp_names[0]).with_suffix(""))
        members = {
            PurePosixPath(n).suffix.lower(): n
            for n in names
            if str(PurePosixPath(n).with_suffix("")) == base
        }
        if ".dbf" not in members:
            raise PlatformValidationError("shapefile DBF missing", code="IMPORT_SHP")
        reader = shapefile.Reader(
            shp=io.BytesIO(archive.read(members[".shp"])),
            shx=io.BytesIO(archive.read(members[".shx"]))
            if ".shx" in members
            else None,
            dbf=io.BytesIO(archive.read(members[".dbf"])),
        )
        fields = [field[0] for field in reader.fields[1:]]
        rows: list[dict[str, Any]] = []
        for sr in reader.iterShapeRecords():
            row = dict(zip(fields, list(sr.record), strict=False))
            if sr.shape.shapeType in {1, 11, 21} and sr.shape.points:
                row["_x"], row["_y"] = sr.shape.points[0][:2]
            row["_shape_type"] = int(sr.shape.shapeType)
            rows.append(row)
        reader.close()
    return ParsedImport(rows, None)


def parse_import(source_format: str, data: bytes) -> ParsedImport:
    if source_format not in _ALLOWED_FORMATS:
        raise PlatformValidationError("unsupported import format", code="IMPORT_FORMAT")
    parser = {
        "csv": _parse_csv,
        "xlsx": _parse_xlsx,
        "geojson": _parse_geojson,
        "dxf": _parse_dxf,
        "shapefile_zip": _parse_shapefile_zip,
    }[source_format]
    return parser(data)


def _font(size: int, *, bold: bool = False) -> ImageFont.ImageFont:
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
        if bold
        else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf"
        if bold
        else "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    ]
    for path in candidates:
        try:
            return ImageFont.truetype(path, size=size)
        except OSError:
            continue
    return ImageFont.load_default()


def _points(geometry: dict[str, Any] | None) -> list[tuple[float, float]]:
    if not geometry:
        return []
    kind = geometry.get("type")
    coords = geometry.get("coordinates")
    if kind == "Point" and isinstance(coords, list) and len(coords) >= 2:
        return [(float(coords[0]), float(coords[1]))]
    if kind == "LineString" and isinstance(coords, list):
        return [
            (float(p[0]), float(p[1]))
            for p in coords
            if isinstance(p, list) and len(p) >= 2
        ]
    if kind == "Polygon" and isinstance(coords, list) and coords:
        ring = coords[0]
        if isinstance(ring, list):
            return [
                (float(p[0]), float(p[1]))
                for p in ring
                if isinstance(p, list) and len(p) >= 2
            ]
    return []


def _fit_transform(
    points: list[tuple[float, float]], width: int, height: int, pad: int = 70
):
    if not points:
        points = [(0.0, 0.0), (1.0, 1.0)]
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    minx, maxx, miny, maxy = min(xs), max(xs), min(ys), max(ys)
    dx = max(maxx - minx, 1.0)
    dy = max(maxy - miny, 1.0)
    scale = min((width - 2 * pad) / dx, (height - 2 * pad) / dy)

    def transform(point: tuple[float, float]) -> tuple[int, int]:
        x, y = point
        px = pad + (x - minx) * scale
        py = height - pad - (y - miny) * scale
        return int(px), int(py)

    return transform


def _map_png(snapshot: dict[str, Any], *, mode: str) -> bytes:
    width, height = 1200, 700
    img = Image.new("RGB", (width, height), "white")
    draw = ImageDraw.Draw(img)
    title_font = _font(32, bold=True)
    label_font = _font(22)
    route_points = _points(snapshot.get("route_geometry"))
    building_points: list[tuple[float, float]] = []
    for building in snapshot.get("route_buildings", []):
        building_points.extend(_points(building.get("geometry")))
    subject_points = _points(snapshot.get("building_geometry"))
    points_for_fit = route_points + building_points + subject_points
    if mode == "detail" and subject_points:
        sx, sy = subject_points[0]
        radius = max(float(snapshot.get("detail_radius", 40.0)), 10.0)
        points_for_fit = [
            (sx - radius, sy - radius),
            (sx + radius, sy + radius),
        ] + subject_points
    transform = _fit_transform(points_for_fit, width, height)

    title = {
        "overview": "Sơ đồ 1 - Vị trí tổng thể",
        "detail": "Sơ đồ 2 - Tuyến và vị trí nhà",
        "comparison": "Sơ đồ 3 - Đối chiếu số nhà lân cận",
    }.get(mode, "Sơ đồ")
    draw.text((30, 20), title, fill="black", font=title_font)
    draw.text(
        (30, 60),
        f"CRS: EPSG:{snapshot.get('crs_code', '—')} | Tuyến: {snapshot.get('route_name', '—')}",
        fill="black",
        font=label_font,
    )
    if len(route_points) >= 2:
        draw.line([transform(p) for p in route_points], fill="black", width=5)
    for building in snapshot.get("route_buildings", []):
        pts = _points(building.get("geometry"))
        if not pts:
            continue
        x, y = transform(pts[0])
        radius = 7
        draw.ellipse(
            (x - radius, y - radius, x + radius, y + radius), outline="black", width=2
        )
        number = building.get("number_text")
        if number and mode != "overview":
            draw.text((x + 10, y - 12), str(number), fill="black", font=label_font)
    if subject_points:
        x, y = transform(subject_points[0])
        radius = 13
        draw.ellipse(
            (x - radius, y - radius, x + radius, y + radius), outline="black", width=5
        )
        draw.text((x + 18, y + 8), "Vị trí hồ sơ", fill="black", font=label_font)
    out = io.BytesIO()
    img.save(out, format="PNG", optimize=True)
    return out.getvalue()


def _drawing_xml(
    rid: str, image_id: int, cx: int = 7_800_000, cy: int = 4_550_000
) -> str:
    return f"""
<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">
<wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{image_id}" name="Map {image_id}"/>
<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
<pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="map{image_id}.png"/><pic:cNvPicPr/></pic:nvPicPr>
<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>
</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
"""


def _paragraph(text_value: str, *, bold: bool = False) -> str:
    props = "<w:rPr><w:b/></w:rPr>" if bold else ""
    return (
        "<w:p><w:r>"
        + props
        + '<w:t xml:space="preserve">'
        + escape(text_value)
        + "</w:t></w:r></w:p>"
    )


def _docx_bytes(title: str, lines: list[str], images: list[bytes]) -> bytes:
    body = [_paragraph(title, bold=True)] + [_paragraph(line) for line in lines]
    rels: list[str] = []
    for index in range(1, len(images) + 1):
        rid = f"rId{index}"
        body.append(_drawing_xml(rid, index))
        rels.append(
            f'<Relationship Id="{rid}" '
            'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
            f'Target="media/map{index}.png"/>'
        )
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
        'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
        'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
        'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        "<w:body>" + "".join(body) + '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
        '<w:pgMar w:top="900" w:right="900" w:bottom="900" w:left="900"/></w:sectPr>'
        "</w:body></w:document>"
    )
    document_rels = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + "".join(rels)
        + "</Relationships>"
    )
    content_types = """<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""
    root_rels = """<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("_rels/.rels", root_rels)
        archive.writestr("word/document.xml", document)
        archive.writestr("word/_rels/document.xml.rels", document_rels)
        for index, image in enumerate(images, start=1):
            archive.writestr(f"word/media/map{index}.png", image)
    return out.getvalue()


def _pdf_bytes(title: str, lines: list[str], images: list[bytes]) -> bytes:
    pages: list[Image.Image] = []
    page = Image.new("RGB", (1240, 1754), "white")
    draw = ImageDraw.Draw(page)
    y = 70
    draw.text((70, y), title, fill="black", font=_font(36, bold=True))
    y += 60
    for line in lines:
        draw.text((70, y), line, fill="black", font=_font(24))
        y += 38
    pages.append(page)
    for image_data in images:
        with Image.open(io.BytesIO(image_data)) as image:
            map_page = Image.new("RGB", (1240, 1754), "white")
            resized = image.convert("RGB")
            resized.thumbnail((1100, 1400))
            x = (1240 - resized.width) // 2
            y = (1754 - resized.height) // 2
            map_page.paste(resized, (x, y))
            pages.append(map_page)
    out = io.BytesIO()
    pages[0].save(
        out, format="PDF", save_all=True, append_images=pages[1:], resolution=150
    )
    return out.getvalue()


def _plate_pdf(number: str, route_name: str) -> bytes:
    img = Image.new("RGB", (1400, 900), "white")
    draw = ImageDraw.Draw(img)
    draw.rectangle((35, 35, 1365, 865), outline="black", width=10)
    number_font = _font(260, bold=True)
    route_font = _font(58, bold=True)
    bbox = draw.textbbox((0, 0), number, font=number_font)
    draw.text(
        ((1400 - (bbox[2] - bbox[0])) / 2, 200), number, fill="black", font=number_font
    )
    route_bbox = draw.textbbox((0, 0), route_name, font=route_font)
    draw.text(
        ((1400 - (route_bbox[2] - route_bbox[0])) / 2, 570),
        route_name,
        fill="black",
        font=route_font,
    )
    out = io.BytesIO()
    img.save(out, format="PDF", resolution=150)
    return out.getvalue()


def _safe_text(value: Any) -> str:
    text = "" if value is None else str(value)
    if text.startswith(("=", "+", "-", "@")):
        text = "'" + text
    return text[:32767]


def _cell(ref: str, value: Any) -> str:
    if isinstance(value, bool):
        return f'<c r="{ref}" t="b"><v>{1 if value else 0}</v></c>'
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return f'<c r="{ref}"><v>{value}</v></c>'
    return (
        f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">'
        f"{escape(_safe_text(value))}</t></is></c>"
    )


def _col_name(index: int) -> str:
    result = ""
    n = index
    while n:
        n, rem = divmod(n - 1, 26)
        result = chr(65 + rem) + result
    return result


WIDTH, HEIGHT = 1240, 1754
def put(
    draw: ImageDraw.ImageDraw,
    value: Any,
    x: int,
    y: int,
    max_width: int,
    size: int = 24,
    bold: bool = False,
):
    font = _font(size, bold=bold)
    s = str(value)
    while s and draw.textbbox((0, 0), s, font=font)[2] > max_width:
        s = s[:-1]
    if s != str(value):
        s = (s[:-1] + "…") if len(s) > 1 else "…"
    draw.text((x, y), s, fill="black", font=font)


def page(title: str, note: str) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    im = Image.new("RGB", (WIDTH, HEIGHT), "white")
    draw = ImageDraw.Draw(im)
    draw.rectangle((55, 55, 1185, 1700), outline="#94A3B8", width=2)
    put(draw, note, 80, 82, 1070, 22, True)
    put(draw, title, 80, 132, 1070, 32, True)
    draw.line((80, 186, 1160, 186), fill="black", width=2)
    return im, draw


def pdf(pages: list[Image.Image]) -> bytes:
    out = io.BytesIO()
    pages[0].save(out, "PDF", save_all=True, append_images=pages[1:], resolution=150)
    return out.getvalue()


def appendix_pages(p: dict[str, Any], rows: list[dict[str, Any]]) -> list[Image.Image]:
    pages = []
    for start in range(0, len(rows), 27):
        im, d = page(
            "DANH SÁCH SỐ NHÀ KÈM PHƯƠNG ÁN", "PHỤ LỤC – TRÍCH XUẤT DỮ LIỆU NỘI BỘ"
        )
        put(d, "Phương án: " + str(p["id"]), 90, 215, 1060, 19)
        for x, v in (
            (90, "STT"),
            (155, "Số nhà"),
            (365, "Đường/hẻm"),
            (800, "Thửa/Mã công trình"),
        ):
            put(d, v, x, 273, 340, 21, True)
        d.line((85, 315, 1155, 315), fill="#334155", width=2)
        for i, row in enumerate(rows[start : start + 27]):
            y = 334 + i * 47
            put(d, start + i + 1, 91, y, 55, 20)
            put(
                d,
                row.get("number_text") or row["proposed_number"],
                155,
                y,
                190,
                20,
                True,
            )
            put(d, row.get("route_name") or "Chưa có", 365, y, 423, 20)
            put(d, row.get("parcel_ref") or row["building_id"], 800, y, 342, 19)
        put(
            d,
            f"Trang phụ lục {len(pages) + 1} · Tổng {len(rows)} hồ sơ",
            90,
            1650,
            1040,
            18,
        )
        pages.append(im)
    return pages


def decision_draft(p: dict[str, Any], rows: list[dict[str, Any]]) -> bytes:
    im, d = page(
        "DỰ THẢO QUYẾT ĐỊNH CẤP SỐ NHÀ", "DỰ THẢO – CHƯA KÝ – KHÔNG CÓ GIÁ TRỊ PHÁP LÝ"
    )
    lines = [
        "Cơ quan có thẩm quyền: [GHI TÊN CƠ QUAN BAN HÀNH]",
        "Số quyết định: [CHƯA CẤP SỐ VĂN BẢN]",
        "Ngày soạn mẫu: " + date.today().strftime("%d/%m/%Y"),
        "Căn cứ quy tắc: " + str(p.get("authority_ref") or "[CHƯA BỔ SUNG]"),
        "Căn cứ hồ sơ khảo sát, xác minh và đối chiếu số nhà.",
        "Điều 1. Dự thảo phê duyệt danh sách số nhà theo bảng kê kèm theo.",
        f"Số lượng công trình: {len(rows)}.",
        f"Trạng thái quy trình nội bộ: {p['status'].upper()}.",
        "Điều 2. Rà soát điều kiện pháp lý, hồ sơ và các bên liên quan.",
        "Điều 3. Hiệu lực theo quyết định được ký và ban hành hợp lệ.",
        "Nơi nhận: [BỔ SUNG]",
        "Người có thẩm quyền ký: [BỔ SUNG]",
        "Chữ ký, dấu: [CHƯA CÓ]",
        "",
        "Bảng kê chi tiết xuất Excel/PDF riêng từ phương án này.",
        "Đây là mẫu trình ký, KHÔNG phải quyết định hành chính đã ban hành.",
    ]
    for i, v in enumerate(lines):
        put(d, v, 92, 235 + i * 78, 1020, 23)
    return pdf([im])


def xlsx(rows: list[list[Any]]) -> bytes:
    sheet = '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
    for i, values in enumerate(rows, 1):
        sheet += (
            f'<row r="{i}">'
            + "".join(_cell(f"{_col_name(j)}{i}", v) for j, v in enumerate(values, 1))
            + "</row>"
        )
    sheet += "</sheetData></worksheet>"
    content_types = """<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"""
    rels = """<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"""
    book = """<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Danh sach so nha" sheetId="1" r:id="rId1"/></sheets></workbook>"""
    bookrels = """<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"""
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in (
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("xl/workbook.xml", book),
            ("xl/_rels/workbook.xml.rels", bookrels),
            ("xl/worksheets/sheet1.xml", sheet),
        ):
            z.writestr(name, '<?xml version="1.0" encoding="UTF-8"?>' + data)
    return out.getvalue()


def register_xlsx(p: dict[str, Any], items: list[dict[str, Any]]) -> bytes:
    title = (
        "DANH SÁCH SỐ NHÀ ĐÃ DUYỆT NỘI BỘ"
        if p["status"] == "approved"
        else "DANH SÁCH SỐ NHÀ DỰ THẢO"
    )
    matrix = [
        [title],
        ["Mã phương án", str(p["id"])],
        ["Trạng thái", p["status"]],
        ["Căn cứ", p.get("authority_ref") or "Chưa xác định"],
        [],
        [
            "STT",
            "Building ID",
            "Số nhà",
            "Tên tuyến/hẻm",
            "Tiền tố",
            "Thửa",
            "Khu vực",
            "Trạng thái số",
            "Đã nghỉ hiệu lực?",
        ],
    ]
    for i, r in enumerate(items, 1):
        matrix.append(
            [
                i,
                str(r["building_id"]),
                r.get("number_text") or r["proposed_number"],
                r.get("route_name") or "",
                r.get("prefix") or "",
                r.get("parcel_ref") or "",
                r.get("area_name") or "",
                r.get("number_kind") or "",
                bool(r.get("retired_at")),
            ]
        )
    return xlsx(matrix)


def qr_house_link(*, org: UUID, project: UUID, source: UUID, feature: str) -> str:
    """Canonical /app URL; never use the unsupported /app/projects/... path."""
    if not feature or len(feature) > 128:
        raise PlatformValidationError(
            "Invalid GIS feature id", code="INVALID_FEATURE_ID"
        )
    return "https://openvietgis.cloud/app?" + urlencode(
        {
            "hn_org": str(org),
            "hn_project": str(project),
            "hn_source": str(source),
            "hn_feature": feature,
        }
    )


def plates_pdf(
    p: dict[str, Any],
    rows: list[dict[str, Any]],
    org: UUID,
    project: UUID,
    offset: int = 0,
    limit: int = 24,
) -> bytes:
    if (
        p["status"] != "approved"
        or p["rule_status"] != "authoritative"
        or not p.get("authority_ref")
    ):
        raise PlatformConflictError(
            "Approved proposal and authoritative rule are required",
            code="APPROVAL_REQUIRED",
        )
    if offset < 0 or not 1 <= limit <= 100:
        raise PlatformValidationError(
            "Print at most 100 QR plates", code="INVALID_PLATE_BATCH"
        )
    chosen = rows[offset : offset + limit]
    if not chosen:
        raise PlatformValidationError(
            "No houses in this page", code="EMPTY_PLATE_BATCH"
        )
    if any(
        x.get("number_kind") != "approved"
        or x.get("retired_at") is not None
        or x.get("building_state") != "active"
        or x.get("number_text") != x.get("proposed_number")
        for x in chosen
    ):
        raise PlatformConflictError(
            "An approved number was changed or retired", code="STALE_APPROVAL"
        )
    pages = []
    for begin in range(0, len(chosen), 6):
        im, d = page(
            "MẪU BIỂN QR SỐ NHÀ", "TRA CỨU NỘI BỘ – KHÔNG THAY THẾ BIỂN CHÍNH THỨC"
        )
        for i, row in enumerate(chosen[begin : begin + 6]):
            x = 80 + (i % 2) * 550
            y = 234 + (i // 2) * 472
            d.rounded_rectangle(
                (x, y, x + 530, y + 445), radius=12, outline="#334155", width=3
            )
            put(d, row["number_text"], x + 28, y + 20, 450, 70, True)
            put(d, row.get("route_name") or "", x + 28, y + 117, 460, 23, True)
            href = qr_house_link(
                org=org,
                project=project,
                source=row["linked_source_id"],
                feature=str(row["linked_feature_id"]),
            )
            qr = qrcode.QRCode(
                error_correction=qrcode.constants.ERROR_CORRECT_M, box_size=6, border=2
            )
            qr.add_data(href)
            qr.make(fit=True)
            qr_img = qr.make_image(fill_color="black", back_color="white").convert(
                "RGB"
            )
            qr_img.thumbnail((245, 245), Image.Resampling.NEAREST)
            im.paste(qr_img, (x + 150, y + 155))
            put(d, "Quét QR → đăng nhập để xem trên GIS", x + 25, y + 407, 490, 18)
        pages.append(im)
    return pdf(pages)


def _safe_csv(value: Any) -> Any:
    if isinstance(value, str) and value.startswith(("=", "+", "-", "@")):
        return "'" + value
    return value


def _encode(
    records: list[dict[str, Any]], output_format: str, prj: str
) -> tuple[bytes, str, str]:
    if output_format == "csv":
        visible = (
            [key for key in records[0] if not key.startswith("_")]
            if records
            else []
        )
        stream = io.StringIO(newline="")
        writer = csv.DictWriter(stream, fieldnames=visible, extrasaction="ignore")
        writer.writeheader()
        for record in records:
            writer.writerow({key: _safe_csv(record.get(key)) for key in visible})
        return (
            stream.getvalue().encode("utf-8-sig"),
            "csv",
            "text/csv; charset=utf-8",
        )
    if output_format == "geojson":
        features = []
        for record in records:
            props = {
                key: value
                for key, value in record.items()
                if not key.startswith("_")
            }
            features.append(
                {
                    "type": "Feature",
                    "geometry": record.get("_geometry"),
                    "properties": props,
                }
            )
        data = json.dumps(
            {"type": "FeatureCollection", "features": features},
            ensure_ascii=False,
            separators=(",", ":"),
        ).encode("utf-8")
        return data, "geojson", "application/geo+json"
    if output_format == "dxf":
        doc = ezdxf.new("R2010")
        msp = doc.modelspace()
        for record in records:
            x, y = record.get("_native_x"), record.get("_native_y")
            if x is None or y is None:
                continue
            msp.add_point((float(x), float(y)))
            label = str(
                record.get("house_number") or record.get("building_id") or ""
            )
            if label:
                msp.add_text(label, height=2.5).set_placement((float(x), float(y)))
        stream = io.StringIO()
        doc.write(stream)
        return stream.getvalue().encode("utf-8"), "dxf", "application/dxf"
    if output_format == "shapefile_zip":
        shp, shx, dbf = io.BytesIO(), io.BytesIO(), io.BytesIO()
        writer = shapefile.Writer(
            shp=shp, shx=shx, dbf=dbf, shapeType=shapefile.POINT
        )
        visible = (
            [key for key in records[0] if not key.startswith("_")]
            if records
            else []
        )
        for key in visible:
            writer.field(key[:10].upper(), "C", size=254)
        for record in records:
            x, y = record.get("_native_x"), record.get("_native_y")
            if x is None or y is None:
                continue
            writer.point(float(x), float(y))
            writer.record(*[str(record.get(key) or "")[:254] for key in visible])
        writer.close()
        archive_data = io.BytesIO()
        with zipfile.ZipFile(archive_data, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("house_numbers.shp", shp.getvalue())
            archive.writestr("house_numbers.shx", shx.getvalue())
            archive.writestr("house_numbers.dbf", dbf.getvalue())
            archive.writestr("house_numbers.prj", prj)
        return archive_data.getvalue(), "zip", "application/zip"
    raise PlatformValidationError("unsupported export format")


def bounded_archive(data):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        names = [i.filename.replace("\\", "/") for i in entries]
        if (len(entries) > 1024 or sum(i.file_size for i in entries) > 64 * 1024 * 1024
                or len(set(names)) != len(names)
                or any(n.startswith("/") or ".." in n.split("/") or ":" in n
                       for n in names)
                or any(i.flag_bits & 1 or (i.external_attr >> 16) & 0o170000 == 0o120000
                       for i in entries)):
            raise PlatformValidationError("unsafe archive", code="IMPORT_ARCHIVE")

def run(req):
    action = req["action"]
    if action == "parse":
        data = Path("input.bin").read_bytes()
        fmt = req["format"]
        if fmt in ("xlsx", "shapefile_zip"):
            bounded_archive(data)
        parsed = parse_import(fmt, data)
        if len(parsed.rows) > 50000:
            raise PlatformValidationError("too many import rows", code="IMPORT_ROW_LIMIT")
        # Use the original canonical bytes for legacy fingerprint replay. No auth or
        # decisions are made by the format adapter.
        canonical = json.dumps(req["fingerprint_options"], sort_keys=True,
                               separators=(",", ":")).encode()
        result = {"rows": parsed.rows, "encoding": parsed.encoding,
                  "fingerprint": sha256(data + b"\0" + canonical).hexdigest()}
        Path("result.json").write_text(json.dumps(result, default=str, allow_nan=False), "utf-8")
        return
    if action == "document":
        snapshot = req["snapshot"]
        if req["doc_type"] == "plate":
            data = _plate_pdf(str(snapshot.get("number_text") or ""),
                              str(snapshot.get("route_name") or ""))
        else:
            maps = [_map_png(snapshot, mode=mode) for mode in ("overview", "detail", "comparison")]
            render = _docx_bytes if req["format"] == "docx" else _pdf_bytes
            data = render(req["title"], req["lines"], maps)
    elif action == "packet":
        p, rows = req["proposal"], req["rows"]
        kind = req["kind"]
        if kind == "decision":
            data = decision_draft(p, rows)
        elif kind == "register":
            data = register_xlsx(p, rows)
        elif kind == "appendix":
            data = pdf(appendix_pages(p, rows))
        elif kind == "plates":
            data = plates_pdf(p, rows, UUID(req["org"]), UUID(req["project"]),
                              0, len(rows))
        else:
            raise PlatformValidationError("Unsupported output", code="INVALID_PACKET_KIND")
    elif action == "export":
        # JSON object keys are sorted by the Rust serializer, so restore explicit
        # client field ordering before the format writer constructs CSV or DBF.
        records = [{**{k: row.get(k) for k in req["fields"]},
                    **{k: v for k, v in row.items() if k.startswith("_")}}
                   for row in req["rows"]]
        data, _, _ = _encode(records, req["format"], req["prj"])
    else:
        raise PlatformValidationError("unsupported action")
    if not data or len(data) > 64 * 1024 * 1024:
        raise PlatformValidationError("artifact size limit exceeded", code="REPORT_SIZE_EXCEEDED")
    Path("output.bin").write_bytes(data)

if __name__ == "__main__":
    try:
        run(json.loads(Path("request.json").read_text("utf-8")))
        print(json.dumps({"ok": True}))
    except PlatformValidationError as error:
        print(json.dumps({"ok": False, "code": error.code, "message": str(error),
                          "conflict": isinstance(error, PlatformConflictError)}))
    except Exception:
        print(json.dumps({"ok": False, "code": "FORMAT_ENGINE_FAILED",
                          "message": "Unable to process format payload"}))
