"""R012 RLS-enforced PostGIS repository for 2D DXF import/export & loss-disclosed conversion."""

from __future__ import annotations

import hashlib
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.adapters.cad.dxf_codec import parse_dxf_2d, write_dxf_2d
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.storage.local_object_store import LocalObjectStore
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.cad.domain import CadConversionRecord, CadUnit
from openvietgis_api.domains.cad.georeference import CadPlacement, place_features
from openvietgis_api.domains.editing.domain import geometry_to_wkt, validate_geometry_payload
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from openvietgis_api.domains.spatial.domain import (
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    SourceField,
    schema_revision_digest,
)


def _conv_from_row(row: RowMapping) -> CadConversionRecord:
    return CadConversionRecord(
        id=row["id"],
        organization_id=row["organization_id"],
        project_id=row["project_id"],
        direction=str(row["direction"]),
        cad_format=str(row["cad_format"]),
        filename=str(row["filename"]),
        units=CadUnit(str(row["units"])),
        crs_code=int(row["crs_code"]),
        status=str(row["status"]),
        entity_summary=dict(row["entity_summary"]),
        loss_report=dict(row["loss_report"]),
        storage_key=str(row["storage_key"]),
        sha256=str(row["sha256"]),
        byte_size=int(row["byte_size"]),
        source_id=row["source_id"],
        layer_id=row["layer_id"],
        content_revision=str(row["content_revision"]) if row["content_revision"] else None,
        actor_id=row["actor_id"],
        created_at=row["created_at"],
    )


def _inspection_fingerprint(
    digest: str,
    units: CadUnit,
    crs_code: int,
    tolerance: float | None,
    placement: CadPlacement | None = None,
) -> str:
    profile: list[Any] = ["dxf-2d-bounded-v1", digest, units.value, crs_code, tolerance]
    if placement is not None:
        profile.append(placement.metadata())
    return hashlib.sha256(json.dumps(profile, allow_nan=False).encode()).hexdigest()


def _apply_placement(
    features: list[dict[str, Any]],
    summary: dict[str, Any],
    placement: CadPlacement | None,
    units: CadUnit,
    crs_code: int,
    curve_tolerance_m: float | None,
) -> None:
    if placement is None:
        summary["spatial_mapping"] = "caller_asserted_projected_grid_xy_scaled_to_meters"
        return
    if placement.units != units or crs_code != 3857 or curve_tolerance_m is not None:
        raise PlatformValidationError(
            "Unsupported control-point placement profile", code="CAD_GEOREFERENCE_INVALID"
        )
    summary["spatial_mapping"] = "control_point_similarity_grid_metres"
    summary["cad_document_id"] = str(placement.document_id)
    summary["cad_document_version"] = placement.version
    summary["placement"] = placement.metadata()
    if placement.native_reference is not None:
        # Verify native bounds for every vertex before publishing the independently fitted web view.
        from copy import deepcopy

        summary["native_placed_extent"] = place_features(
            deepcopy(features), placement.native_reference
        )
    summary["placed_extent"] = place_features(features, placement)


class SqlCadRepository:
    def __init__(self, session: Session, storage_root: Path) -> None:
        self._session = session
        self._store = LocalObjectStore(storage_root)

    def inspect_dxf(
        self,
        *,
        context: AuthorizationContext,
        filename: str,
        payload: bytes,
        units: CadUnit,
        crs_code: int,
        curve_tolerance_m: float | None,
        placement: CadPlacement | None = None,
    ) -> dict[str, Any]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_EDIT)
        if not filename.lower().endswith(".dxf"):
            raise PlatformValidationError(
                "only 2D DXF can be inspected; DWG adapter unavailable",
                code="CAD_FORMAT_UNSUPPORTED",
            )
        if crs_code not in (3857, 5899):
            raise PlatformValidationError(
                "explicit projected CRS required", code="UNGEOREFERENCED_CAD_REJECTED"
            )
        if not payload or len(payload) > 25 * 1024 * 1024:
            raise PlatformValidationError("CAD upload size unsupported", code="CAD_FILE_LIMIT")
        family, features, summary, report = parse_dxf_2d(
            payload, units=units, curve_tolerance_m=curve_tolerance_m
        )
        _apply_placement(features, summary, placement, units, crs_code, curve_tolerance_m)
        for feat in features:
            validate_geometry_payload(feat["geometry"], GeometryFamily(family))
        # The same PostGIS geometry predicate as the eventual publication, without
        # inserting a source, layer, geometry, conversion or artifact.
        invalid = self._session.execute(
            text("""
                SELECT count(*) FROM jsonb_array_elements_text(CAST(:wkts AS jsonb)) AS a(wkt)
                WHERE NOT ST_IsValid(ST_GeomFromText(a.wkt)) OR ST_IsEmpty(ST_GeomFromText(a.wkt))
            """),
            {"wkts": json.dumps([geometry_to_wkt(feat["geometry"]) for feat in features])},
        ).scalar_one()
        if invalid:
            raise PlatformValidationError(
                "invalid DXF geometry; publication rejected", code="INVALID_GEOMETRY"
            )
        digest = hashlib.sha256(payload).hexdigest()
        summary["drawing_sha256"] = digest
        summary["crs_code"] = crs_code
        return {
            "sha256": digest,
            "inspection_fingerprint": _inspection_fingerprint(
                digest, units, crs_code, curve_tolerance_m, placement
            ),
            "published": False,
            "geometry_valid": True,
            "entity_summary": summary,
            "loss_report": report,
        }

    def import_cad(
        self,
        *,
        context: AuthorizationContext,
        filename: str,
        payload: bytes,
        layer_name: str,
        units: CadUnit,
        crs_code: int,
        curve_tolerance_m: float | None = None,
        acknowledge_loss: bool = False,
        expected_sha256: str | None = None,
        expected_inspection_fingerprint: str | None = None,
        placement: CadPlacement | None = None,
    ) -> CadConversionRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_EDIT)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        org_id, project_id = context.organization_id, context.project_id

        if crs_code not in (3857, 5899):
            raise PlatformValidationError(
                "DXF length units require an explicit supported projected CRS (3857 or 5899); "
                "geographic degrees are not metres",
                code="UNGEOREFERENCED_CAD_REJECTED",
            )
        if not payload or len(payload) > 25 * 1024 * 1024:
            raise PlatformValidationError("CAD upload size unsupported", code="CAD_FILE_LIMIT")

        conv_id = uuid4()
        lower = filename.lower()
        if not lower.endswith((".dwg", ".dxf")):
            raise PlatformValidationError("only .dxf is supported", code="CAD_FORMAT_UNSUPPORTED")
        fmt = "dwg" if lower.endswith(".dwg") else "dxf"
        skey = f"cad/{org_id}/{project_id}/{conv_id}.{fmt}"
        sha256_hex, byte_size = hashlib.sha256(payload).hexdigest(), len(payload)
        now = datetime.now(UTC)
        if expected_sha256 is not None and expected_sha256 != sha256_hex:
            raise PlatformConflictError("DXF changed after inspection", code="CAD_INSPECTION_STALE")

        if fmt == "dwg":
            # Preserve original evidence and reject DWG without licensed adapter.
            self._store.put_bytes(skey, payload)
            row = (
                self._session.execute(
                    text("""
                    INSERT INTO cad.conversions (
                        id, organization_id, project_id, direction, cad_format, filename,
                        units, crs_code, status, entity_summary, loss_report, storage_key,
                        sha256, byte_size, actor_id, created_at
                    ) VALUES (
                        :id, :org, :proj, 'import', 'dwg', :fname,
                        :units, :crs, 'rejected', CAST(:esum AS jsonb), CAST(:loss AS jsonb),
                        :skey, :sha, :sz, :actor, :now
                    )
                    RETURNING *
                """),
                    {
                        "id": conv_id,
                        "org": org_id,
                        "proj": project_id,
                        "fname": filename[:255],
                        "units": units.value,
                        "crs": crs_code,
                        "esum": json.dumps({"detected_format": "dwg"}),
                        "loss": json.dumps(
                            {
                                "reason": "DWG_ADAPTER_REQUIRED",
                                "message": "Native DWG requires a separately licensed adapter; "
                                "convert to 2D DXF R2010+ for direct import.",
                            }
                        ),
                        "skey": skey,
                        "sha": sha256_hex,
                        "sz": byte_size,
                        "actor": context.actor.actor_id,
                        "now": now,
                    },
                )
                .mappings()
                .one()
            )
            self._session.commit()
            raise PlatformValidationError(
                "DWG requires a separately licensed adapter; original evidence preserved.",
                code="DWG_ADAPTER_REQUIRED",
                details={"conversion_id": str(row["id"]), "sha256": sha256_hex},
            )

        family, features, entity_summary, loss_report = parse_dxf_2d(
            payload, units=units, curve_tolerance_m=curve_tolerance_m
        )
        _apply_placement(features, entity_summary, placement, units, crs_code, curve_tolerance_m)
        for feature in features:
            validate_geometry_payload(feature["geometry"], GeometryFamily(family))
        inspection_fingerprint = _inspection_fingerprint(
            sha256_hex, units, crs_code, curve_tolerance_m, placement
        )
        if (
            expected_inspection_fingerprint is not None
            and expected_inspection_fingerprint != inspection_fingerprint
        ):
            raise PlatformConflictError(
                "DXF inspection profile or drawing changed", code="CAD_INSPECTION_STALE"
            )
        if loss_report["requires_acknowledgement"] and (
            not acknowledge_loss or expected_inspection_fingerprint is None
        ):
            raise PlatformValidationError(
                "Inspect the drawing and explicitly acknowledge CAD mapping loss before import",
                code="CAD_LOSS_ACK_REQUIRED",
                details={
                    "sha256": sha256_hex,
                    "inspection_fingerprint": inspection_fingerprint,
                    "entity_summary": entity_summary,
                    "loss_report": loss_report,
                },
            )
        entity_summary["drawing_sha256"] = sha256_hex
        entity_summary["inspection_fingerprint"] = inspection_fingerprint
        entity_summary["crs_code"] = crs_code
        loss_report["acknowledged"] = acknowledge_loss
        if placement is not None:
            if expected_inspection_fingerprint != inspection_fingerprint or not acknowledge_loss:
                raise PlatformValidationError(
                    "Inspect and confirm control-point placement",
                    code="CAD_GEOREFERENCE_CONFIRM_REQUIRED",
                )
            # Serialize retries of the same immutable version/placement profile.
            self._session.execute(
                text("SELECT pg_advisory_xact_lock(hashtextextended(:key, 0))"),
                {"key": f"cad-georef:{org_id}:{project_id}:{inspection_fingerprint}"},
            )
            prior = (
                self._session.execute(
                    text("""SELECT * FROM cad.conversions
                WHERE organization_id=:org AND project_id=:proj AND direction='import'
                AND status='succeeded' AND entity_summary->>'inspection_fingerprint'=:fingerprint
                ORDER BY created_at LIMIT 1"""),
                    {"org": org_id, "proj": project_id, "fingerprint": inspection_fingerprint},
                )
                .mappings()
                .one_or_none()
            )
            if prior is not None:
                return _conv_from_row(prior)

        # Publish as a new project-owned spatial source & layer (kind='derived_processing')
        out_source_id = uuid4()
        out_layer_id = uuid4()
        clean_name = layer_name.strip()[:200] or "CAD DXF Layer"
        cad_fields = [
            SourceField(
                organization_id=org_id,
                project_id=project_id,
                source_id=out_source_id,
                field_id=fid,
                name=fname,
                ordinal=ordinal,
                type=FieldType.TEXT,
                nullable=False,
                schema_revision="0" * 64,
            )
            for ordinal, (fid, fname) in enumerate(
                [("cad_layer", "CAD Layer"), ("cad_entity", "CAD Entity")]
            )
        ]
        schema_rev = schema_revision_digest(
            cad_fields, GeometryFamily(family), GeometryDimensions.XY
        )
        content_rev = hashlib.sha256(f"{out_source_id}:{sha256_hex}".encode()).hexdigest()
        if placement is not None:
            entity_summary["imported_schema_revision"] = schema_rev
        ref_key = f"cad_dxf_{str(out_source_id).replace('-', '')[:12]}"

        self._session.execute(
            text("""
                INSERT INTO spatial.data_sources (
                    id, organization_id, project_id, name, kind, reference_key,
                    fixture_version, manifest_digest, source_asset_id, lifecycle,
                    validation_state, validation_reason, source_crs, stored_crs,
                    schema_state, geometry_family, dimensions, feature_count_state,
                    feature_count, extent_state, extent, catalog_revision,
                    content_revision, schema_revision, created_at, updated_at, retired_at
                ) VALUES (
                    :id, :org, :proj, :name, 'derived_processing', :rkey,
                    'cad_dxf_2d', :mdig, NULL, 'active',
                    'ready', NULL, :crs, :crs,
                    'known', :gfam, 'XY', 'known',
                    0, 'empty', NULL, 1,
                    :crev, :srev, :now, :now, NULL
                )
            """),
            {
                "id": out_source_id,
                "org": org_id,
                "proj": project_id,
                "name": clean_name,
                "rkey": ref_key,
                "mdig": sha256_hex,
                "crs": crs_code,
                "gfam": family,
                "crev": content_rev,
                "srev": schema_rev,
                "now": now,
            },
        )

        for ordinal, (fid, fname) in enumerate(
            [("cad_layer", "CAD Layer"), ("cad_entity", "CAD Entity")]
        ):
            self._session.execute(
                text("""
                    INSERT INTO spatial.source_fields (
                        organization_id, project_id, source_id, field_id,
                        name, ordinal, type, nullable, schema_revision
                    ) VALUES (:org, :proj, :src, :fid, :fname, :ord, 'text', false, :srev)
                """),
                {
                    "org": org_id,
                    "proj": project_id,
                    "src": out_source_id,
                    "fid": fid,
                    "fname": fname,
                    "ord": ordinal,
                    "srev": schema_rev,
                },
            )

        for feat in features:
            wkt = geometry_to_wkt(feat["geometry"])
            self._session.execute(
                text("""
                    INSERT INTO spatial.reference_features (
                        organization_id, project_id, source_id, feature_id,
                        attributes, geometry, geometry_state, content_revision, schema_revision
                    ) VALUES (
                        :org, :proj, :src, :fid,
                        CAST(:attrs AS jsonb),
                        ST_SetSRID(ST_GeomFromText(:wkt), :srid),
                        'present', :crev, :srev
                    )
                """),
                {
                    "org": org_id,
                    "proj": project_id,
                    "src": out_source_id,
                    "fid": feat["feature_id"],
                    "attrs": json.dumps(feat["attributes"]),
                    "wkt": wkt,
                    "srid": crs_code,
                    "crev": content_rev,
                    "srev": schema_rev,
                },
            )

        invalid = self._session.execute(
            text("""
                SELECT count(*) FROM spatial.reference_features
                WHERE organization_id = :org AND project_id = :proj AND source_id = :src
                  AND (ST_IsEmpty(geometry) OR NOT ST_IsValid(geometry))
            """),
            {"org": org_id, "proj": project_id, "src": out_source_id},
        ).scalar_one()
        if invalid:
            raise PlatformValidationError(
                "invalid DXF geometry; publication rejected", code="INVALID_GEOMETRY"
            )

        stats = (
            self._session.execute(
                text("""
                SELECT COUNT(*)::int AS cnt,
                       ST_XMin(ST_Extent(geometry)) AS min_x,
                       ST_YMin(ST_Extent(geometry)) AS min_y,
                       ST_XMax(ST_Extent(geometry)) AS max_x,
                       ST_YMax(ST_Extent(geometry)) AS max_y
                FROM spatial.reference_features
                WHERE organization_id = :org AND project_id = :proj AND source_id = :src
            """),
                {"org": org_id, "proj": project_id, "src": out_source_id},
            )
            .mappings()
            .one()
        )
        self._session.execute(
            text("""
                UPDATE spatial.data_sources
                SET feature_count = :cnt,
                    extent_state = 'available',
                    extent = CAST(:ext AS jsonb)
                WHERE organization_id = :org AND project_id = :proj AND id = :src
            """),
            {
                "org": org_id,
                "proj": project_id,
                "src": out_source_id,
                "cnt": int(stats["cnt"]),
                "ext": json.dumps(
                    {
                        "min_x": float(stats["min_x"]),
                        "min_y": float(stats["min_y"]),
                        "max_x": float(stats["max_x"]),
                        "max_y": float(stats["max_y"]),
                        "crs_code": int(crs_code),
                    }
                ),
            },
        )
        self._session.execute(
            text("""
                INSERT INTO spatial.layers (
                    id, organization_id, project_id, source_id, name,
                    lifecycle, layer_revision, created_at, updated_at, retired_at
                ) VALUES (:lid, :org, :proj, :src, :name, 'active', 1, :now, :now, NULL)
            """),
            {
                "lid": out_layer_id,
                "org": org_id,
                "proj": project_id,
                "src": out_source_id,
                "name": clean_name,
                "now": now,
            },
        )

        audit_id = uuid4()
        append_project_audit(
            self._session,
            context,
            audit_id=audit_id,
            action="cad.dxf.import",
            resource_type="data_source",
            resource_id=out_source_id,
            metadata={"filename": filename[:255], "crs_code": crs_code, "units": units.value}
            | (
                {
                    "cad_document_id": str(placement.document_id),
                    "cad_document_version": placement.version,
                    "inspection_fingerprint": inspection_fingerprint,
                }
                if placement
                else {}
            ),
            created_at=now,
        )

        row = (
            self._session.execute(
                text("""
                INSERT INTO cad.conversions (
                    id, organization_id, project_id, direction, cad_format, filename,
                    units, crs_code, status, entity_summary, loss_report, storage_key,
                    sha256, byte_size, source_id, layer_id, content_revision,
                    actor_id, created_at
                ) VALUES (
                    :id, :org, :proj, 'import', 'dxf', :fname,
                    :units, :crs, 'succeeded', CAST(:esum AS jsonb), CAST(:loss AS jsonb),
                    :skey, :sha, :sz, :src, :lid, :crev, :actor, :now
                )
                RETURNING *
            """),
                {
                    "id": conv_id,
                    "org": org_id,
                    "proj": project_id,
                    "fname": filename[:255],
                    "units": units.value,
                    "crs": crs_code,
                    "esum": json.dumps(entity_summary),
                    "loss": json.dumps(loss_report),
                    "skey": skey,
                    "sha": sha256_hex,
                    "sz": byte_size,
                    "src": out_source_id,
                    "lid": out_layer_id,
                    "crev": content_rev,
                    "actor": context.actor.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        # Publish original bytes only after parsing, geometry validation and
        # transaction writes succeeded; rejected DXF must not leave orphan files.
        self._store.put_bytes(skey, payload)
        self._session.commit()
        return _conv_from_row(row)

    def export_dxf(
        self,
        *,
        context: AuthorizationContext,
        layer_id: UUID,
        units: CadUnit,
        expected_content_revision: str,
        acknowledge_attribute_loss: bool,
    ) -> tuple[CadConversionRecord, bytes]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        org_id, project_id = context.organization_id, context.project_id

        meta = (
            self._session.execute(
                text("""
                SELECT l.id AS layer_id, l.name AS layer_name,
                       s.id AS source_id, s.stored_crs, s.content_revision, s.dimensions
                FROM spatial.layers l
                JOIN spatial.data_sources s
                  ON s.organization_id = l.organization_id
                 AND s.project_id = l.project_id
                 AND s.id = l.source_id
                WHERE l.organization_id = :org AND l.project_id = :proj AND l.id = :lid
                  AND l.lifecycle = 'active' AND s.lifecycle = 'active'
                  AND s.validation_state = 'ready'
                FOR SHARE OF s, l
            """),
                {"org": org_id, "proj": project_id, "lid": layer_id},
            )
            .mappings()
            .first()
        )
        if meta is None:
            raise PlatformNotFoundError("layer not found")
        if int(meta["stored_crs"]) not in (3857, 5899):
            raise PlatformValidationError(
                "DXF export requires a metric projected source; "
                "no implicit degree-to-meter conversion",
                code="CAD_PROJECTED_CRS_REQUIRED",
            )
        if expected_content_revision != meta["content_revision"]:
            raise PlatformConflictError(
                "requested CAD source revision changed", code="STALE_CAD_REVISION"
            )

        if str(meta["dimensions"]) != GeometryDimensions.XY.value:
            raise PlatformValidationError(
                "DXF export requires XY geometry", code="CAD_3D_UNSUPPORTED"
            )
        if not acknowledge_attribute_loss:
            raise PlatformValidationError(
                "DXF export omits source attributes and original curve semantics; "
                "explicit acknowledgement required",
                code="CAD_LOSS_ACK_REQUIRED",
            )

        frows = (
            self._session.execute(
                text("""
                SELECT feature_id, ST_AsGeoJSON(geometry, 15, 0)::jsonb AS geom
                FROM spatial.reference_features
                WHERE organization_id = :org AND project_id = :proj AND source_id = :src
                ORDER BY feature_id ASC LIMIT 10001
            """),
                {"org": org_id, "proj": project_id, "src": meta["source_id"]},
            )
            .mappings()
            .all()
        )
        if len(frows) > 10000:
            raise PlatformValidationError(
                "DXF source exceeds 10000 features", code="CAD_FEATURE_LIMIT"
            )
        if any(r["geom"] is None for r in frows):
            raise PlatformValidationError(
                "DXF cannot preserve null geometry", code="CAD_GEOMETRY_UNSUPPORTED"
            )
        features = [
            {"feature_id": str(r["feature_id"]), "geometry": dict(r["geom"])} for r in frows
        ]
        dxf_bytes = write_dxf_2d(features, layer_name=str(meta["layer_name"]), units=units)
        conv_id = uuid4()
        fname = f"export-{conv_id}.dxf"
        skey = f"cad/{org_id}/{project_id}/{fname}"
        sha256_hex, byte_size = self._store.put_bytes(skey, dxf_bytes)
        now = datetime.now(UTC)

        row = (
            self._session.execute(
                text("""
                INSERT INTO cad.conversions (
                    id, organization_id, project_id, direction, cad_format, filename,
                    units, crs_code, status, entity_summary, loss_report, storage_key,
                    sha256, byte_size, source_id, layer_id, content_revision,
                    actor_id, created_at
                ) VALUES (
                    :id, :org, :proj, 'export', 'dxf', :fname,
                    :units, :crs, 'succeeded', CAST(:esum AS jsonb), CAST(:loss AS jsonb),
                    :skey, :sha, :sz, :src, :lid, :crev, :actor, :now
                )
                RETURNING *
            """),
                {
                    "id": conv_id,
                    "org": org_id,
                    "proj": project_id,
                    "fname": fname,
                    "units": units.value,
                    "crs": int(meta["stored_crs"]),
                    "esum": json.dumps(
                        {"exported_features": len(features), "dxf_version": "R2010"}
                    ),
                    "loss": json.dumps(
                        {
                            "attributes_omitted_in_dxf_geometry": True,
                            "original_curve_semantics_not_preserved": True,
                            "acknowledged": True,
                            "silent_loss": False,
                        }
                    ),
                    "skey": skey,
                    "sha": sha256_hex,
                    "sz": byte_size,
                    "src": meta["source_id"],
                    "lid": layer_id,
                    "crev": str(meta["content_revision"]),
                    "actor": context.actor.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self._session.commit()
        return _conv_from_row(row), dxf_bytes

    def list_conversions(self, *, context: AuthorizationContext) -> list[CadConversionRecord]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        rows = (
            self._session.execute(
                text("""
                SELECT * FROM cad.conversions
                WHERE organization_id = :org AND project_id = :proj
                ORDER BY created_at DESC LIMIT 50
            """),
                {"org": context.organization_id, "proj": context.project_id},
            )
            .mappings()
            .all()
        )
        return [_conv_from_row(r) for r in rows]
