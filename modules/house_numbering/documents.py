"""House-numbering dossier, three-map DOCX/PDF and plate issuance."""

from __future__ import annotations

import io
import zipfile
from datetime import UTC, datetime
from html import escape
from typing import Any
from uuid import UUID, uuid4

from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from openvietgis_api.module_sdk import put_module_artifact, read_module_artifact
from PIL import Image, ImageDraw, ImageFont
from sqlalchemy import text
from sqlalchemy.orm import Session

from modules.house_numbering.domain import Rule, certificate_issuance
from modules.house_numbering.persistence import HouseNumberingRepository

_DOC_TYPES = {"inspection", "certificate", "house_form", "plate"}
_OUTPUTS = {"docx", "pdf"}


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


class HouseNumberingDocumentService:
    def __init__(self, session: Session, context: AuthorizationContext) -> None:
        self.session = session
        self.context = context
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        self.org = context.organization_id
        self.project = context.project_id
        self.actor = context.actor.actor_id
        self.repo = HouseNumberingRepository(session, context)

    def _snapshot(self, building_id: UUID) -> dict[str, Any]:
        self.repo.prepare("read")
        row = (
            self.session.execute(
                text("""
                SELECT b.id AS building_id,b.parcel_ref,b.eligibility,
                       r.id AS route_id,r.name AS route_name,r.linked_source_id AS route_source_id,
                       r.linked_feature_id AS route_feature_id,
                       n.id AS number_id,n.number_text,n.kind AS number_kind,n.rule_id,
                       rule.version AS rule_version,rule.status AS rule_status,
                       rule.authority_ref,rule.left_odd,rule.suffix_style,rule.max_distance_m,
                       ST_SRID(rf.geometry) AS crs_code,
                       ST_AsGeoJSON(rf.geometry)::jsonb AS route_geometry,
                       ST_AsGeoJSON(bf.geometry)::jsonb AS building_geometry
                FROM house_numbering.buildings b
                LEFT JOIN house_numbering.routes r
                  ON r.organization_id=b.organization_id AND r.project_id=b.project_id AND r.id=b.route_id
                LEFT JOIN house_numbering.route_lines rf
                  ON rf.organization_id=r.organization_id AND rf.project_id=r.project_id AND rf.route_id=r.id
                LEFT JOIN spatial.reference_features bf
                  ON bf.organization_id=b.organization_id AND bf.project_id=b.project_id
                 AND bf.source_id=b.linked_source_id AND bf.feature_id=b.linked_feature_id
                LEFT JOIN LATERAL (
                    SELECT * FROM house_numbering.address_numbers n
                    WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                      AND n.building_id=b.id AND n.retired_at IS NULL
                    ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END,
                             n.created_at DESC LIMIT 1
                ) n ON true
                LEFT JOIN house_numbering.rules rule
                  ON rule.organization_id=b.organization_id AND rule.project_id=b.project_id
                 AND rule.id=n.rule_id
                WHERE b.organization_id=:org AND b.project_id=:project AND b.id=:building
            """),
                {"org": self.org, "project": self.project, "building": building_id},
            )
            .mappings()
            .first()
        )
        if row is None:
            raise PlatformNotFoundError("building dossier not found")
        snapshot = dict(row)
        route_buildings = (
            self.session.execute(
                text("""
                SELECT b.id,n.number_text,n.kind,
                       ST_AsGeoJSON(ST_PointOnSurface(f.geometry))::jsonb AS geometry
                FROM house_numbering.buildings b
                JOIN spatial.reference_features f
                  ON f.organization_id=b.organization_id AND f.project_id=b.project_id
                 AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
                LEFT JOIN LATERAL (
                    SELECT number_text,kind FROM house_numbering.address_numbers n
                    WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                      AND n.building_id=b.id AND n.retired_at IS NULL
                    ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END,
                             n.created_at DESC LIMIT 1
                ) n ON true
                WHERE b.organization_id=:org AND b.project_id=:project
                  AND b.route_id=:route AND b.state='active'
            """),
                {"org": self.org, "project": self.project, "route": row["route_id"]},
            )
            .mappings()
            .all()
            if row["route_id"]
            else []
        )
        snapshot["route_buildings"] = [dict(item) for item in route_buildings]
        snapshot["detail_radius"] = 35.0
        return snapshot

    def _rule(self, snapshot: dict[str, Any]) -> Rule | None:
        if not snapshot.get("rule_id"):
            return None
        return Rule(
            version=str(snapshot["rule_version"]),
            left_odd=bool(snapshot["left_odd"]),
            suffix_style=str(snapshot["suffix_style"]),
            max_distance=float(snapshot["max_distance_m"]),
            authority=str(snapshot["authority_ref"])
            if snapshot.get("authority_ref")
            else None,
        )

    def issue(
        self,
        *,
        building_id: UUID,
        doc_type: str,
        output_format: str,
        template_revision: str,
        template_verified: bool,
        replaces_document_id: UUID | None = None,
    ) -> dict[str, Any]:
        self.repo.prepare("manage")
        if doc_type not in _DOC_TYPES or output_format not in _OUTPUTS:
            raise PlatformValidationError("unsupported document type or output")
        snapshot = self._snapshot(building_id)
        number_state = snapshot.get("number_kind")
        number = str(snapshot.get("number_text") or "")
        rule = self._rule(snapshot)
        if doc_type in {"certificate", "house_form", "plate"}:
            if number_state != "approved" or not number or rule is None:
                raise PlatformConflictError(
                    "official house-number output requires approved number",
                    code="APPROVED_NUMBER_REQUIRED",
                )
            status = certificate_issuance(
                "approved", template_verified=template_verified, rule=rule
            )
        else:
            status = (
                "official"
                if template_verified
                and rule is not None
                and rule.authority
                and number_state == "approved"
                else "draft"
            )
        title = {
            "inspection": "PHIẾU KIỂM TRA THÔNG TIN NHÀ",
            "certificate": "HỒ SƠ XÁC NHẬN SỐ NHÀ",
            "house_form": "PHIẾU THÔNG TIN SỐ NHÀ",
            "plate": "BIỂN SỐ NHÀ",
        }[doc_type]
        lines = [
            f"Mã công trình: {building_id}",
            f"Tuyến: {snapshot.get('route_name') or '—'}",
            f"Số nhà: {number or 'Chưa có'}",
            f"Thửa/parcel: {snapshot.get('parcel_ref') or '—'}",
            f"Quy tắc: {snapshot.get('rule_version') or '—'}",
            f"Căn cứ: {snapshot.get('authority_ref') or '—'}",
            f"Template revision: {template_revision}",
        ]
        if doc_type == "plate":
            data = _plate_pdf(number, str(snapshot.get("route_name") or ""))
            output_format = "pdf"
        else:
            maps = [
                _map_png(snapshot, mode="overview"),
                _map_png(snapshot, mode="detail"),
                _map_png(snapshot, mode="comparison"),
            ]
            if output_format == "docx":
                data = _docx_bytes(title, lines, maps)
            else:
                data = _pdf_bytes(title, lines, maps)
        content_type = (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            if output_format == "docx"
            else "application/pdf"
        )
        artifact = put_module_artifact(
            self.context,
            namespace="house-numbering-documents",
            filename=f"{doc_type}-{building_id}.{output_format}",
            content_type=content_type,
            data=data,
        )
        now = datetime.now(UTC)
        if replaces_document_id is not None:
            previous = self.session.execute(
                text("""
                    UPDATE house_numbering.documents
                    SET status='reissue_required',revision=revision+1,
                        updated_by_actor_id=:actor,updated_at=:now
                    WHERE organization_id=:org AND project_id=:project AND id=:id
                      AND building_id=:building AND status='official'
                    RETURNING id
                """),
                {
                    "actor": self.actor,
                    "now": now,
                    "org": self.org,
                    "project": self.project,
                    "id": replaces_document_id,
                    "building": building_id,
                },
            ).scalar_one_or_none()
            if previous is None:
                raise PlatformConflictError(
                    "reissue target is not an official document"
                )
        document_id = uuid4()
        row = (
            self.session.execute(
                text("""
                INSERT INTO house_numbering.documents (
                    id,organization_id,project_id,building_id,number_id,doc_type,
                    template_revision,status,artifact_id,issued_at,replaces_document_id,
                    revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at
                ) VALUES (
                    :id,:org,:project,:building,:number,:type,:template,:status,:artifact,
                    :issued,:replaces,1,:actor,:actor,:now,:now
                ) RETURNING *
            """),
                {
                    "id": document_id,
                    "org": self.org,
                    "project": self.project,
                    "building": building_id,
                    "number": snapshot.get("number_id"),
                    "type": doc_type,
                    "template": template_revision,
                    "status": status,
                    "artifact": artifact.id,
                    "issued": now if status == "official" else None,
                    "replaces": replaces_document_id,
                    "actor": self.actor,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self.repo.audit(
            "house_numbering.document.issue",
            "house_numbering_document",
            document_id,
            {
                "doc_type": doc_type,
                "status": status,
                "artifact_id": str(artifact.id),
                "sha256": artifact.sha256,
                "template_revision": template_revision,
            },
        )
        self.session.commit()
        result = dict(row)
        result["artifact"] = {
            "id": str(artifact.id),
            "filename": artifact.filename,
            "content_type": artifact.content_type,
            "byte_size": artifact.byte_size,
            "sha256": artifact.sha256,
        }
        return result

    def issue_batch(
        self,
        *,
        building_ids: list[UUID],
        doc_type: str,
        output_format: str,
        template_revision: str,
        template_verified: bool,
    ) -> list[dict[str, Any]]:
        if (
            not building_ids
            or len(building_ids) > 100
            or len(set(building_ids)) != len(building_ids)
        ):
            raise PlatformValidationError(
                "document batch must contain 1..100 unique buildings"
            )
        return [
            self.issue(
                building_id=building_id,
                doc_type=doc_type,
                output_format=output_format,
                template_revision=template_revision,
                template_verified=template_verified,
            )
            for building_id in building_ids
        ]

    def history(self, building_id: UUID) -> list[dict[str, Any]]:
        self.repo.prepare("read")
        rows = (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.documents
                WHERE organization_id=:org AND project_id=:project AND building_id=:building
                ORDER BY created_at DESC,id DESC
            """),
                {"org": self.org, "project": self.project, "building": building_id},
            )
            .mappings()
            .all()
        )
        return [dict(row) for row in rows]

    def download(self, document_id: UUID) -> tuple[dict[str, Any], bytes]:
        self.repo.prepare("read")
        row = (
            self.session.execute(
                text("""
                SELECT * FROM house_numbering.documents
                WHERE organization_id=:org AND project_id=:project AND id=:id
            """),
                {"org": self.org, "project": self.project, "id": document_id},
            )
            .mappings()
            .first()
        )
        if row is None or row["artifact_id"] is None:
            raise PlatformNotFoundError("document artifact not found")
        try:
            artifact, data = read_module_artifact(
                self.context,
                namespace="house-numbering-documents",
                artifact_id=row["artifact_id"],
            )
        except (FileNotFoundError, RuntimeError) as exc:
            raise PlatformNotFoundError("document artifact unavailable") from exc
        self.repo.audit(
            "house_numbering.document.download",
            "house_numbering_document",
            document_id,
            {"artifact_id": str(artifact.id)},
        )
        self.session.commit()
        return {
            "filename": artifact.filename,
            "content_type": artifact.content_type,
            "sha256": artifact.sha256,
            "byte_size": artifact.byte_size,
        }, data
