"""Scoped search and redacted/public vs explicitly private exports."""

from __future__ import annotations

import csv
import io
import json
import zipfile
from datetime import UTC, datetime
from typing import Any
from uuid import UUID

import ezdxf
import shapefile
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.platform_errors import PlatformValidationError
from openvietgis_api.module_sdk import put_module_artifact, read_module_artifact
from sqlalchemy import text
from sqlalchemy.orm import Session

from modules.house_numbering.domain import public_export_record
from modules.house_numbering.persistence import HouseNumberingRepository
from modules.house_numbering.privacy import HouseNumberingPrivateData

_PUBLIC_FIELDS = {
    "building_id",
    "house_number",
    "old_number",
    "proposed_number",
    "approved_number",
    "route_name",
    "area_name",
    "parcel_number",
    "sheet_number",
    "state",
    "crs",
}
_FORMATS = {"csv", "geojson", "dxf", "shapefile_zip"}


def _safe_csv(value: Any) -> Any:
    if isinstance(value, str) and value.startswith(("=", "+", "-", "@")):
        return "'" + value
    return value


class HouseNumberingExportService:
    def __init__(self, session: Session, context: AuthorizationContext) -> None:
        self.session = session
        self.context = context
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        self.org = context.organization_id
        self.project = context.project_id
        self.actor = context.actor.actor_id
        self.repo = HouseNumberingRepository(session, context)

    def _base_rows(self) -> list[dict[str, Any]]:
        self.repo.prepare("read")
        rows = (
            self.session.execute(
                text("""
                SELECT b.id AS building_id,b.parcel_ref AS parcel_number,
                       r.name AS route_name,a.name AS area_name,
                       current_n.number_text AS house_number,current_n.kind AS state,
                       old_n.number_text AS old_number,
                       CASE WHEN current_n.kind='proposed' THEN current_n.number_text END AS proposed_number,
                       CASE WHEN current_n.kind='approved' THEN current_n.number_text END AS approved_number,
                       ST_SRID(f.geometry) AS crs,
                       ST_AsGeoJSON(ST_Transform(ST_PointOnSurface(f.geometry),4326))::jsonb AS geometry_4326,
                       ST_X(ST_PointOnSurface(f.geometry)) AS native_x,
                       ST_Y(ST_PointOnSurface(f.geometry)) AS native_y,
                       ST_SRID(f.geometry) AS native_crs
                FROM house_numbering.buildings b
                LEFT JOIN house_numbering.routes r
                  ON r.organization_id=b.organization_id AND r.project_id=b.project_id AND r.id=b.route_id
                LEFT JOIN house_numbering.areas a
                  ON a.organization_id=b.organization_id AND a.project_id=b.project_id AND a.id=b.area_id
                JOIN spatial.reference_features f
                  ON f.organization_id=b.organization_id AND f.project_id=b.project_id
                 AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
                LEFT JOIN LATERAL (
                    SELECT number_text,kind FROM house_numbering.address_numbers n
                    WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                      AND n.building_id=b.id AND n.retired_at IS NULL
                    ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2
                              WHEN 'proposed' THEN 3 ELSE 4 END,n.created_at DESC LIMIT 1
                ) current_n ON true
                LEFT JOIN LATERAL (
                    SELECT number_text FROM house_numbering.address_numbers n
                    WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                      AND n.building_id=b.id AND n.retired_at IS NULL
                      AND n.kind IN ('old','certificate','cad','survey')
                    ORDER BY n.created_at DESC LIMIT 1
                ) old_n ON true
                WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active'
                ORDER BY r.name,current_n.number_text,b.id
            """),
                {"org": self.org, "project": self.project},
            )
            .mappings()
            .all()
        )
        result: list[dict[str, Any]] = []
        for row in rows:
            item = dict(row)
            item["building_id"] = str(item["building_id"])
            item["crs"] = f"EPSG:{item['crs']}" if item.get("crs") else None
            result.append(item)
        return result

    def lookup_approved_addresses(
        self, *, query: str, limit: int = 50
    ) -> list[dict[str, Any]]:
        """Version-one integration projection: approved addresses only, no household data."""
        self.repo.prepare("read")
        q = query.strip()
        if not 2 <= len(q) <= 120 or not 1 <= limit <= 100:
            raise PlatformValidationError(
                "query length must be 2..120 and limit 1..100"
            )
        rows = (
            self.session.execute(
                text("""
                SELECT b.id AS address_id,n.number_text AS house_number,
                       r.name AS street,a.name AS area,
                       ST_X(ST_Transform(ST_PointOnSurface(f.geometry),4326)) AS longitude,
                       ST_Y(ST_Transform(ST_PointOnSurface(f.geometry),4326)) AS latitude
                FROM house_numbering.address_numbers n
                JOIN house_numbering.buildings b
                  ON b.organization_id=n.organization_id AND b.project_id=n.project_id
                 AND b.id=n.building_id AND b.state='active'
                LEFT JOIN house_numbering.routes r
                  ON r.organization_id=b.organization_id AND r.project_id=b.project_id
                 AND r.id=n.route_id
                LEFT JOIN house_numbering.areas a
                  ON a.organization_id=b.organization_id AND a.project_id=b.project_id
                 AND a.id=b.area_id
                JOIN spatial.reference_features f
                  ON f.organization_id=b.organization_id AND f.project_id=b.project_id
                 AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
                WHERE n.organization_id=:org AND n.project_id=:project
                  AND n.kind='approved' AND n.retired_at IS NULL
                  AND f.geometry IS NOT NULL AND NOT ST_IsEmpty(f.geometry)
                  AND strpos(lower(concat_ws(' ',n.number_text,r.name,a.name)),lower(:q))>0
                ORDER BY CASE WHEN n.number_text=:q THEN 0 ELSE 1 END,
                         r.name,n.number_text,b.id
                LIMIT :limit
            """),
                {"org": self.org, "project": self.project, "q": q, "limit": limit},
            )
            .mappings()
            .all()
        )
        return [dict(row) for row in rows]

    def search(
        self,
        *,
        query: str | None = None,
        x: float | None = None,
        y: float | None = None,
        crs_code: int | None = None,
        max_distance: float = 100.0,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        self.repo.prepare("read")
        if not 1 <= limit <= 500:
            raise PlatformValidationError("search limit must be 1..500")
        if query:
            q = query.strip()
            if not q:
                raise PlatformValidationError("search query must not be blank")

            def wire(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
                return [
                    {
                        "building_id": str(row["building_id"]),
                        "route_name": row["route_name"],
                        "house_number": row["house_number"],
                        "state": row["state"],
                        "old_number": row["evidence_number"],
                        "evidence_origin": row["evidence_origin"],
                        "parcel_number": row["parcel_ref"],
                    }
                    for row in rows
                ]

            exact_rows = [
                dict(row)
                for row in self.session.execute(
                    text("""
                    WITH candidates AS (
                        SELECT building_id
                        FROM house_numbering.address_numbers
                        WHERE organization_id=:org AND project_id=:project
                          AND number_text=:exact AND retired_at IS NULL
                          AND building_id IS NOT NULL
                        UNION
                        SELECT building_id
                        FROM house_numbering.evidence
                        WHERE organization_id=:org AND project_id=:project
                          AND raw_number=:exact AND building_id IS NOT NULL
                        UNION
                        SELECT id
                        FROM house_numbering.buildings
                        WHERE organization_id=:org AND project_id=:project
                          AND parcel_ref=:exact AND state='active'
                    )
                    SELECT b.id AS building_id,r.name AS route_name,b.parcel_ref,
                           n.number_text AS house_number,n.kind AS state,
                           ev.raw_number AS evidence_number,ev.origin AS evidence_origin
                    FROM candidates c
                    JOIN house_numbering.buildings b
                      ON b.organization_id=:org AND b.project_id=:project
                     AND b.id=c.building_id AND b.state='active'
                    LEFT JOIN house_numbering.routes r
                      ON r.organization_id=b.organization_id AND r.project_id=b.project_id
                     AND r.id=b.route_id
                    LEFT JOIN LATERAL (
                        SELECT number_text,kind
                        FROM house_numbering.address_numbers n
                        WHERE n.organization_id=b.organization_id
                          AND n.project_id=b.project_id AND n.building_id=b.id
                          AND n.retired_at IS NULL
                        ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2
                                 WHEN 'proposed' THEN 3 ELSE 4 END,n.created_at DESC,n.id
                        LIMIT 1
                    ) n ON true
                    LEFT JOIN LATERAL (
                        SELECT raw_number,origin
                        FROM house_numbering.evidence ev
                        WHERE ev.organization_id=b.organization_id
                          AND ev.project_id=b.project_id AND ev.building_id=b.id
                        ORDER BY ev.evidence_date DESC NULLS LAST,ev.created_at DESC,ev.id DESC
                        LIMIT 1
                    ) ev ON true
                    ORDER BY r.name,n.number_text,b.id
                    LIMIT :limit
                    """),
                    {
                        "org": self.org,
                        "project": self.project,
                        "exact": q,
                        "limit": limit,
                    },
                )
                .mappings()
                .all()
            ]
            if exact_rows:
                return wire(exact_rows)

            safe_query = q.replace("%", "").replace("_", "")
            rows = [
                dict(row)
                for row in self.session.execute(
                    text("""
                    SELECT DISTINCT b.id AS building_id,r.name AS route_name,b.parcel_ref,
                           n.number_text AS house_number,n.kind AS state,
                           ev.raw_number AS evidence_number,ev.origin AS evidence_origin
                    FROM house_numbering.buildings b
                    LEFT JOIN house_numbering.routes r
                      ON r.organization_id=b.organization_id AND r.project_id=b.project_id
                     AND r.id=b.route_id
                    LEFT JOIN house_numbering.address_numbers n
                      ON n.organization_id=b.organization_id AND n.project_id=b.project_id
                     AND n.building_id=b.id AND n.retired_at IS NULL
                    LEFT JOIN house_numbering.evidence ev
                      ON ev.organization_id=b.organization_id AND ev.project_id=b.project_id
                     AND ev.building_id=b.id
                    WHERE b.organization_id=:org AND b.project_id=:project
                      AND b.state='active' AND (
                        n.number_text ILIKE :q OR ev.raw_number ILIKE :q OR r.name ILIKE :q
                        OR COALESCE(b.parcel_ref,'') ILIKE :q
                      )
                    ORDER BY r.name,n.number_text,b.id LIMIT :limit
                    """),
                    {
                        "org": self.org,
                        "project": self.project,
                        "q": f"%{safe_query}%",
                        "limit": limit,
                    },
                )
                .mappings()
                .all()
            ]
            return wire(rows)
        if any(v is not None for v in (x, y, crs_code)):
            if x is None or y is None or crs_code is None:
                raise PlatformValidationError(
                    "coordinate search requires x, y and crs_code"
                )
            rows = (
                self.session.execute(
                    text("""
                    SELECT b.id AS building_id,r.name AS route_name,n.number_text AS house_number,
                           n.kind AS state,b.parcel_ref,
                           ST_Distance(
                             ST_Transform(ST_PointOnSurface(f.geometry),:crs),
                             ST_SetSRID(ST_MakePoint(:x,:y),:crs)
                           ) AS distance
                    FROM house_numbering.buildings b
                    JOIN spatial.reference_features f
                      ON f.organization_id=b.organization_id AND f.project_id=b.project_id
                     AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
                    LEFT JOIN house_numbering.routes r
                      ON r.organization_id=b.organization_id AND r.project_id=b.project_id AND r.id=b.route_id
                    LEFT JOIN LATERAL (
                        SELECT number_text,kind FROM house_numbering.address_numbers n
                        WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                          AND n.building_id=b.id AND n.retired_at IS NULL
                        ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END,
                                 n.created_at DESC LIMIT 1
                    ) n ON true
                    WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active'
                      AND ST_DWithin(
                        ST_Transform(ST_PointOnSurface(f.geometry),:crs),
                        ST_SetSRID(ST_MakePoint(:x,:y),:crs),:distance
                      )
                    ORDER BY distance,b.id LIMIT :limit
                """),
                    {
                        "org": self.org,
                        "project": self.project,
                        "crs": crs_code,
                        "x": x,
                        "y": y,
                        "distance": max_distance,
                        "limit": limit,
                    },
                )
                .mappings()
                .all()
            )
            return [
                {
                    "building_id": str(row["building_id"]),
                    "route_name": row["route_name"],
                    "house_number": row["house_number"],
                    "state": row["state"],
                    "parcel_number": row["parcel_ref"],
                    "distance": float(row["distance"]),
                }
                for row in rows
            ]
        raise PlatformValidationError("search requires query or coordinates")

    def _public_records(self, fields: list[str]) -> list[dict[str, Any]]:
        unknown = sorted(set(fields) - _PUBLIC_FIELDS)
        if unknown:
            raise PlatformValidationError(
                f"public export fields not allowed: {', '.join(unknown)}",
                code="EXPORT_FIELDS",
            )
        result: list[dict[str, Any]] = []
        for row in self._base_rows():
            safe = public_export_record({key: row.get(key) for key in fields})
            safe["_geometry"] = row.get("geometry_4326")
            safe["_native_x"] = row.get("native_x")
            safe["_native_y"] = row.get("native_y")
            safe["_native_crs"] = row.get("native_crs")
            result.append(safe)
        return result

    def _private_records(self, fields: list[str], purpose: str) -> list[dict[str, Any]]:
        self.repo.enable_private_access()
        if not purpose.strip():
            raise PlatformValidationError("private export requires explicit purpose")
        base = {row["building_id"]: row for row in self._base_rows()}
        private_rows = (
            self.session.execute(
                text("""
                SELECT building_id,protected_data FROM house_numbering.restricted_households
                WHERE organization_id=:org AND project_id=:project
                  AND (expires_at IS NULL OR expires_at>now())
                ORDER BY created_at DESC,id DESC
            """),
                {"org": self.org, "project": self.project},
            )
            .mappings()
            .all()
        )
        output: list[dict[str, Any]] = []
        for private in private_rows:
            building_id = str(private["building_id"])
            record = {**base.get(building_id, {}), **dict(private["protected_data"])}
            record["building_id"] = building_id
            output.append({key: record.get(key) for key in fields})
        private_service = HouseNumberingPrivateData(self.session, self.context)
        private_service._log(
            action="private.export",
            resource_type="restricted_household",
            resource_id=None,
            metadata={"purpose": purpose[:100], "fields": fields, "count": len(output)},
        )
        return output

    def _encode(
        self, records: list[dict[str, Any]], output_format: str
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
            srid = next(
                (r.get("_native_crs") for r in records if r.get("_native_crs")), None
            )
            prj = ""
            if srid:
                prj = (
                    self.session.execute(
                        text(
                            "SELECT COALESCE(srtext,'') FROM spatial_ref_sys WHERE srid=:srid"
                        ),
                        {"srid": int(srid)},
                    ).scalar_one_or_none()
                    or ""
                )
            archive_data = io.BytesIO()
            with zipfile.ZipFile(archive_data, "w", zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("house_numbers.shp", shp.getvalue())
                archive.writestr("house_numbers.shx", shx.getvalue())
                archive.writestr("house_numbers.dbf", dbf.getvalue())
                archive.writestr("house_numbers.prj", prj)
            return archive_data.getvalue(), "zip", "application/zip"
        raise PlatformValidationError("unsupported export format")

    def export(
        self,
        *,
        fields: list[str],
        output_format: str,
        include_private: bool = False,
        purpose: str | None = None,
    ) -> dict[str, Any]:
        if output_format not in _FORMATS:
            raise PlatformValidationError("unsupported export format")
        if not fields or len(fields) != len(set(fields)):
            raise PlatformValidationError("export requires unique explicit fields")
        if include_private:
            records = self._private_records(fields, purpose or "")
        else:
            records = self._public_records(fields)
        data, ext, content_type = self._encode(records, output_format)
        artifact = put_module_artifact(
            self.context,
            namespace=(
                "house-numbering-private-exports"
                if include_private
                else "house-numbering-exports"
            ),
            filename=f"house-numbering-{datetime.now(UTC).strftime('%Y%m%dT%H%M%SZ')}.{ext}",
            content_type=content_type,
            data=data,
        )
        self.repo.audit(
            "house_numbering.export",
            "house_numbering_export",
            artifact.id,
            {
                "format": output_format,
                "fields": fields,
                "include_private": include_private,
                "record_count": len(records),
                "sha256": artifact.sha256,
            },
        )
        self.session.commit()
        return {
            "artifact_id": str(artifact.id),
            "filename": artifact.filename,
            "content_type": artifact.content_type,
            "byte_size": artifact.byte_size,
            "sha256": artifact.sha256,
            "record_count": len(records),
            "include_private": include_private,
        }

    def download(self, artifact_id: UUID) -> tuple[dict[str, Any], bytes]:
        self.repo.prepare("read")
        is_private = False
        try:
            artifact, data = read_module_artifact(
                self.context,
                namespace="house-numbering-exports",
                artifact_id=artifact_id,
            )
        except FileNotFoundError:
            self.repo.enable_private_access()
            is_private = True
            artifact, data = read_module_artifact(
                self.context,
                namespace="house-numbering-private-exports",
                artifact_id=artifact_id,
            )
        self.repo.audit(
            "house_numbering.export.download",
            "house_numbering_export",
            artifact_id,
            {"sha256": artifact.sha256, "include_private": is_private},
        )
        self.session.commit()
        return {
            "filename": artifact.filename,
            "content_type": artifact.content_type,
            "byte_size": artifact.byte_size,
            "sha256": artifact.sha256,
            "include_private": is_private,
        }, data
