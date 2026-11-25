"""Native CAD file vault and explicit version-pinned 2D DXF placement."""

import hashlib
import json
import re
from collections.abc import Mapping
from datetime import UTC, datetime
from pathlib import Path
from typing import Annotated, Any, Literal
from urllib.parse import quote
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, HTTPException, Query, Response, UploadFile
from fastapi.responses import JSONResponse
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.cad.dxf_codec import write_dxf_2d
from openvietgis_api.adapters.cad.native_delta import cad_to_gis_preview, gis_to_cad_preview
from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.cad_repository import SqlCadRepository
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.storage.local_object_store import LocalObjectStore
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.cad.domain import CadUnit
from openvietgis_api.domains.cad.georeference import (
    CadPlacement,
    Control,
    fit_web_placement,
    restore_features,
)

router = APIRouter(tags=["cad-documents"])
BASE = "/organizations/{organization_id}/projects/{project_id}/cad/documents"
WHERE = "organization_id=:org AND project_id=:proj"
COLS = "document_id,version,filename,format,sha256,byte_size,created_at"
LIMIT = 25 * 1024 * 1024
QUOTA = 500 * 1024 * 1024


def _scope(
    session: Session, auth: AuthenticatedRequest, org: UUID, proj: UUID, permission: Permission
) -> AuthorizationContext:
    ctx = context_for(auth, organization_id=org, project_id=proj)
    if ctx.actor.actor_type != "human":
        raise HTTPException(403, "CAD requires human project access")
    prepare_project_scope(session, ctx, permission=permission)
    if session.execute(
        text("SELECT gis_access.scoped(:org,:proj)"), {"org": org, "proj": proj}
    ).scalar():
        raise HTTPException(403, "CAD requires unrestricted project access")
    return ctx


def _validate(filename: str, payload: bytes) -> str:
    if not filename or len(filename) > 180 or re.search(r"[/\\\x00-\x1f\x7f]", filename):
        raise HTTPException(422, "Invalid file name")
    ext = filename.rsplit(".", 1)[-1].lower()
    if "." not in filename or ext not in ("dxf", "dwg"):
        raise HTTPException(422, "Only DWG/DXF supported")
    if not payload or len(payload) > LIMIT:
        raise HTTPException(413, "CAD file size limit")
    if ext == "dwg" and not re.match(rb"AC10[0-9]{2}", payload[:6]):
        raise HTTPException(422, "Invalid DWG header")
    if ext == "dxf" and not (
        payload.startswith(b"AutoCAD Binary DXF")
        or re.search(rb"(?:^|\r?\n)\s*0\s*\r?\n\s*SECTION\s*\r?\n", payload[:4096])
    ):
        raise HTTPException(422, "Invalid DXF header")
    return ext


def _wire(r: Mapping[str, Any]) -> dict[str, Any]:
    return {
        **{k: r[k] for k in ("version", "filename", "format", "sha256", "byte_size")},
        "id": str(r["document_id"]),
        "created_at": r["created_at"].isoformat(),
    }


def _store() -> LocalObjectStore:
    return LocalObjectStore(Path(get_settings().artifact_store_dir))


def _save(
    session: Session, ctx: AuthorizationContext, doc: UUID, expected: int, file: UploadFile
) -> dict[str, Any]:
    args = {"org": ctx.organization_id, "proj": ctx.project_id, "doc": doc}
    # Serialize all writes, including project quota, before touching storage.
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": "cad-doc:" + str(ctx.project_id)},
    )
    current = session.execute(
        text(f"SELECT max(version) FROM cad.document_versions WHERE {WHERE} AND document_id=:doc"),
        args,
    ).scalar_one()
    if expected == 0:
        if current is not None:
            raise HTTPException(409, "Document already exists")
    elif current is None:
        raise HTTPException(404, "CAD document not found")
    elif current != expected:
        raise HTTPException(409, "CAD document changed; reload")
    if expected >= 20:
        raise HTTPException(413, "CAD revision limit")
    payload = file.file.read(LIMIT + 1)
    filename = file.filename or ""
    fmt = _validate(filename, payload)
    total = session.execute(
        text(f"SELECT coalesce(sum(byte_size),0) FROM cad.document_versions WHERE {WHERE}"), args
    ).scalar_one()
    if total + len(payload) > QUOTA:
        raise HTTPException(413, "Project CAD quota exceeded")
    if (
        expected == 0
        and session.execute(
            text(f"SELECT count(DISTINCT document_id) FROM cad.document_versions WHERE {WHERE}"),
            args,
        ).scalar_one()
        >= 100
    ):
        raise HTTPException(413, "Project CAD document limit")
    if expected:
        prior = session.execute(
            text(
                f"SELECT format FROM cad.document_versions WHERE {WHERE} "
                "AND document_id=:doc AND version=:prev"
            ),
            args | {"prev": expected},
        ).scalar_one()
        if prior != fmt:
            raise HTTPException(422, "CAD revision format cannot change")
    key = f"cad-doc/{ctx.organization_id}/{ctx.project_id}/{doc}/{uuid4()}.{fmt}"
    store = _store()
    digest, size = store.put_bytes(key, payload)
    try:
        row = (
            session.execute(
                text(f"""INSERT INTO cad.document_versions(
                    organization_id,project_id,document_id,version,filename,format,
                    storage_key,sha256,byte_size,actor_id)
                    VALUES(:org,:proj,:doc,:version,:filename,:format,:key,:sha,:size,:actor)
                    RETURNING {COLS}"""),
                args
                | {
                    "version": expected + 1,
                    "filename": filename,
                    "format": fmt,
                    "key": key,
                    "sha": digest,
                    "size": size,
                    "actor": ctx.actor.actor_id,
                },
            )
            .mappings()
            .one()
        )
        append_project_audit(
            session,
            ctx,
            audit_id=uuid4(),
            action="cad.document.save",
            resource_type="cad_document",
            resource_id=doc,
            metadata={"version": expected + 1, "sha256": digest, "byte_size": size},
            created_at=datetime.now(UTC),
        )
        result = _wire(dict(row))
    except Exception:
        session.rollback()
        store.delete_object(key)
        raise
    # Do not remove on an ambiguous commit failure: an orphan is safer than lost committed bytes.
    session.commit()
    return result


@router.get(BASE)
def list_documents(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, list[dict[str, Any]]]:
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = session.execute(
        text(
            f"SELECT DISTINCT ON(document_id) {COLS} FROM cad.document_versions "
            f"WHERE {WHERE} ORDER BY document_id,version DESC"
        ),
        {"org": organization_id, "proj": project_id},
    ).mappings()
    return {"items": [_wire(dict(r)) for r in rows]}


@router.post(BASE, status_code=201)
def create_document(
    organization_id: UUID,
    project_id: UUID,
    file: Annotated[UploadFile, File()],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    return _save(session, ctx, uuid4(), 0, file)


@router.post(BASE + "/{document_id}/versions", status_code=201)
def append_version(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    file: Annotated[UploadFile, File()],
    expected_version: Annotated[int, Form(ge=1)],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    return _save(session, ctx, document_id, expected_version, file)


@router.get(BASE + "/{document_id}/versions")
def list_versions(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, list[dict[str, Any]]]:
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = (
        session.execute(
            text(
                f"SELECT {COLS} FROM cad.document_versions WHERE {WHERE} "
                "AND document_id=:doc ORDER BY version DESC"
            ),
            {"org": organization_id, "proj": project_id, "doc": document_id},
        )
        .mappings()
        .all()
    )
    if not rows:
        raise HTTPException(404, "CAD document not found")
    return {"items": [_wire(dict(r)) for r in rows]}


@router.get(BASE + "/{document_id}/versions/{version}/download")
def download_version(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    version: int,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    row = (
        session.execute(
            text(
                f"SELECT filename,storage_key,sha256 FROM cad.document_versions WHERE {WHERE} "
                "AND document_id=:doc AND version=:version"
            ),
            {"org": organization_id, "proj": project_id, "doc": document_id, "version": version},
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(404, "CAD version not found")
    payload = _store().get_bytes(row["storage_key"], expected_sha256=row["sha256"])
    return Response(
        payload,
        media_type="application/octet-stream",
        headers={
            "Content-Disposition": "attachment; filename*=UTF-8''"
            + quote(row["filename"], safe=""),
            "Cache-Control": "no-store",
            "X-Content-Type-Options": "nosniff",
            "X-Content-SHA256": row["sha256"],
        },
    )


def _linked_features(
    session: Session,
    ctx: AuthorizationContext,
    *,
    source_id: UUID | None = None,
    feature_id: str | None = None,
    document_id: UUID | None = None,
    version: int | None = None,
    handle: str | None = None,
) -> list[dict[str, Any]]:
    # Only immutable conversion provenance is authoritative, never editable feature attributes.
    # The document, layer, source and live feature must still exist in this exact project.
    filter_sql = (
        "AND c.source_id=:src AND f.feature_id=:fid"
        if source_id is not None
        else "AND d.document_id=:doc AND d.version=:version AND upper(m.item->>'handle')=:handle"
    )
    rows = (
        session.execute(
            text(
                """
            SELECT d.document_id, d.version, d.filename, c.source_id, c.layer_id,
                   f.feature_id, m.item->>'handle' AS handle, l.name AS layer_name,
                   (c.content_revision=s.content_revision) AS original_revision,
                   c.id AS conversion_id, s.content_revision, s.schema_revision
            FROM cad.conversions c
            JOIN cad.document_versions d ON d.organization_id=c.organization_id
                AND d.project_id=c.project_id
                AND d.document_id::text=c.entity_summary->>'cad_document_id'
                AND d.version::text=c.entity_summary->>'cad_document_version'
                AND d.sha256=c.sha256
            JOIN spatial.layers l ON l.organization_id=c.organization_id
                AND l.project_id=c.project_id AND l.id=c.layer_id
                AND l.source_id=c.source_id AND l.lifecycle='active'
            JOIN spatial.data_sources s ON s.organization_id=c.organization_id
                AND s.project_id=c.project_id AND s.id=c.source_id AND s.lifecycle='active'
                AND s.validation_state='ready'
            CROSS JOIN LATERAL jsonb_array_elements(c.entity_summary->'entity_mapping') m(item)
            JOIN spatial.reference_features f ON f.organization_id=c.organization_id
                AND f.project_id=c.project_id AND f.source_id=c.source_id
                AND f.feature_id=m.item->>'feature_id'
            WHERE c.organization_id=:org AND c.project_id=:proj
                AND c.direction='import' AND c.status='succeeded'
                AND m.item->>'disposition' IN ('mapped','approximated')
            """
                + filter_sql
                + " ORDER BY c.created_at,c.id LIMIT 101"
            ),
            {
                "org": ctx.organization_id,
                "proj": ctx.project_id,
                "src": source_id,
                "fid": feature_id,
                "doc": document_id,
                "version": version,
                "handle": handle,
            },
        )
        .mappings()
        .all()
    )
    if len(rows) > 100:
        raise HTTPException(409, "Too many linked conversions; narrow selection")
    return [
        {
            **dict(row),
            "document_id": str(row["document_id"]),
            "source_id": str(row["source_id"]),
            "layer_id": str(row["layer_id"]),
            "conversion_id": str(row["conversion_id"]),
        }
        for row in rows
    ]


@router.get(BASE + "/links")
def feature_cad_links(
    organization_id: UUID,
    project_id: UUID,
    source_id: UUID,
    feature_id: Annotated[str, Query(min_length=1, max_length=200)],
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    return {"items": _linked_features(session, ctx, source_id=source_id, feature_id=feature_id)}


@router.get(BASE + "/{document_id}/versions/{version}/links")
def cad_feature_links(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    version: int,
    handle: Annotated[str, Query(pattern=r"^[0-9a-fA-F]{1,32}$")],
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    return {
        "items": _linked_features(
            session, ctx, document_id=document_id, version=version, handle=handle.upper()
        )
    }


class GeorefRequest(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)
    crs_code: Literal[3857, 5899]
    web_controls: tuple[Control, ...] | None = Field(default=None, min_length=3, max_length=100)
    units: CadUnit
    controls: tuple[Control, ...] = Field(min_length=3, max_length=100)
    tolerance_m: float = Field(ge=1e-6, le=1000)
    max_scale_deviation: float = Field(ge=0, le=0.2)


class GeorefImportRequest(GeorefRequest):
    layer_name: str = Field(min_length=1, max_length=200)
    expected_sha256: str = Field(pattern=r"^[a-f0-9]{64}$")
    expected_inspection_fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    acknowledge_loss: Literal[True]


def _placement(
    session: Session,
    ctx: AuthorizationContext,
    document_id: UUID,
    version: int,
    body: GeorefRequest,
) -> tuple[str, bytes, CadPlacement]:
    row = (
        session.execute(
            text(
                "SELECT filename,format,storage_key,sha256 FROM cad.document_versions "
                f"WHERE {WHERE} AND document_id=:doc AND version=:version"
            ),
            {
                "org": ctx.organization_id,
                "proj": ctx.project_id,
                "doc": document_id,
                "version": version,
            },
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(404, "CAD version not found")
    if row["format"] != "dxf":
        raise HTTPException(422, "Control-point import supports 2D DXF only")
    placement = fit_web_placement(
        document_id=document_id,
        version=version,
        crs_code=body.crs_code,
        units=body.units,
        controls=body.controls,
        web_controls=body.web_controls,
        tolerance_m=body.tolerance_m,
        max_scale_deviation=body.max_scale_deviation,
    )
    payload = _store().get_bytes(row["storage_key"], expected_sha256=row["sha256"])
    return str(row["filename"]), payload, placement


@router.post(BASE + "/{document_id}/versions/{version}/inspect-georef")
def inspect_georef(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    version: int,
    body: GeorefRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    filename, payload, placement = _placement(session, ctx, document_id, version, body)
    return SqlCadRepository(session, Path(get_settings().artifact_store_dir)).inspect_dxf(
        context=ctx,
        filename=filename,
        payload=payload,
        units=body.units,
        crs_code=placement.crs_code,
        curve_tolerance_m=None,
        placement=placement,
    )


@router.post(BASE + "/{document_id}/versions/{version}/import-georef", status_code=201)
def import_georef(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    version: int,
    body: GeorefImportRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    filename, payload, placement = _placement(session, ctx, document_id, version, body)
    record = SqlCadRepository(session, Path(get_settings().artifact_store_dir)).import_cad(
        context=ctx,
        filename=filename,
        payload=payload,
        layer_name=body.layer_name,
        units=body.units,
        crs_code=placement.crs_code,
        acknowledge_loss=body.acknowledge_loss,
        expected_sha256=body.expected_sha256,
        expected_inspection_fingerprint=body.expected_inspection_fingerprint,
        placement=placement,
    )
    return {
        "id": str(record.id),
        "layer_id": str(record.layer_id),
        "source_id": str(record.source_id),
        "sha256": record.sha256,
        "content_revision": record.content_revision,
        "entity_summary": record.entity_summary,
        "loss_report": record.loss_report,
    }


def _reviewed_placement(
    summary: Mapping[str, Any], document_id: UUID, version: int
) -> CadPlacement:
    # Use immutable conversion provenance, not request-supplied controls.
    meta = summary.get("placement")
    if not isinstance(meta, dict) or meta.get("algorithm") != "similarity-grid-2d-v1":
        raise HTTPException(422, "Missing reviewed CAD placement")
    native = meta.get("native_reference")
    control = GeorefRequest.model_validate(
        {
            "crs_code": native["crs_code"] if native else meta["crs_code"],
            "units": meta["units"],
            "controls": native["controls"] if native else meta["controls"],
            "web_controls": meta["controls"] if native else None,
            "tolerance_m": meta["tolerance_grid_m"],
            "max_scale_deviation": meta["max_scale_deviation"],
        }
    )
    return fit_web_placement(
        document_id=document_id,
        version=version,
        crs_code=control.crs_code,
        units=control.units,
        controls=control.controls,
        web_controls=control.web_controls,
        tolerance_m=control.tolerance_m,
        max_scale_deviation=control.max_scale_deviation,
    )


class GeorefDraftRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    conversion_id: UUID
    expected_document_sha256: str = Field(pattern=r"^[a-f0-9]{64}$")
    expected_content_revision: str = Field(pattern=r"^[a-f0-9]{64}$")
    expected_schema_revision: str = Field(pattern=r"^[a-f0-9]{64}$")
    expected_preview_fingerprint: str | None = Field(default=None, pattern=r"^[a-f0-9]{64}$")
    acknowledge_loss: bool = False
    profile: Literal["derived-local-dxf-v1", "native-xy-dxf-v1"] = "derived-local-dxf-v1"


@router.post(BASE + "/{document_id}/versions/{version}/export-georef-draft")
def export_georef_draft(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    version: int,
    body: GeorefDraftRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    """Review and download a derived DXF, never mutate the original document."""
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    args = {
        "org": ctx.organization_id,
        "proj": ctx.project_id,
        "doc": document_id,
        "version": version,
        "conversion": body.conversion_id,
    }
    # Share the native vault's lock order so latest version and source revision
    # cannot change between validation and materialization.
    session.execute(
        text("SELECT pg_advisory_xact_lock_shared(hashtextextended(:key,0))"),
        {"key": "cad-doc:" + str(ctx.project_id)},
    )
    row = (
        session.execute(
            text("""
        SELECT d.sha256, d.storage_key, d.format, c.entity_summary,
               c.source_id, c.layer_id, s.content_revision, s.schema_revision,
               s.stored_crs, s.dimensions, s.feature_count, l.name AS layer_name
        FROM cad.conversions c
        JOIN cad.document_versions d ON d.organization_id=c.organization_id
          AND d.project_id=c.project_id AND d.document_id=:doc
          AND d.version=:version AND d.sha256=c.sha256
          AND d.document_id::text=c.entity_summary->>'cad_document_id'
          AND d.version::text=c.entity_summary->>'cad_document_version'
        JOIN spatial.data_sources s ON s.organization_id=c.organization_id
          AND s.project_id=c.project_id AND s.id=c.source_id
        JOIN spatial.layers l ON l.organization_id=c.organization_id
          AND l.project_id=c.project_id AND l.id=c.layer_id AND l.source_id=s.id
        WHERE c.organization_id=:org AND c.project_id=:proj AND c.id=:conversion
          AND c.direction='import' AND c.status='succeeded'
          AND s.lifecycle='active' AND s.validation_state='ready' AND l.lifecycle='active'
        FOR SHARE OF s,l
    """),
            args,
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(404, "Version-pinned CAD conversion not found")
    latest = session.execute(
        text(f"SELECT max(version) FROM cad.document_versions WHERE {WHERE} AND document_id=:doc"),
        {"org": ctx.organization_id, "proj": ctx.project_id, "doc": document_id},
    ).scalar_one()
    if latest != version or (row["sha256"], row["content_revision"], row["schema_revision"]) != (
        body.expected_document_sha256,
        body.expected_content_revision,
        body.expected_schema_revision,
    ):
        raise HTTPException(409, "CAD or GIS source revision changed")
    if row["format"] != "dxf" or row["stored_crs"] != 3857 or row["dimensions"] != "XY":
        raise HTTPException(422, "Only calibrated local XY DXF supported")
    if not 1 <= row["feature_count"] <= 10_000:
        raise HTTPException(422, "Draft requires 1–10000 features")
    placement = _reviewed_placement(row["entity_summary"], document_id, version)
    original = _store().get_bytes(row["storage_key"], expected_sha256=row["sha256"])
    # Bound database materialization before serializing any geometry to JSON.
    counts = (
        session.execute(
            text("""
            SELECT count(*) AS n,coalesce(sum(ST_NPoints(geometry)),0) AS positions,
                   coalesce(max(ST_NPoints(geometry)),0) AS largest
            FROM spatial.reference_features WHERE organization_id=:org
              AND project_id=:proj AND source_id=:src
        """),
            {"org": ctx.organization_id, "proj": ctx.project_id, "src": row["source_id"]},
        )
        .mappings()
        .one()
    )
    if (
        counts["n"] != row["feature_count"]
        or counts["positions"] > 200_000
        or counts["largest"] > 4096
    ):
        raise HTTPException(422, "Derived DXF position limit exceeded")
    records = (
        session.execute(
            text("""
        SELECT feature_id,ST_AsGeoJSON(geometry,15,0)::jsonb AS geom,
               ST_IsValid(geometry) AND NOT ST_IsEmpty(geometry) AS valid
        FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj
          AND source_id=:src ORDER BY feature_id LIMIT 10001
    """),
            {"org": ctx.organization_id, "proj": ctx.project_id, "src": row["source_id"]},
        )
        .mappings()
        .all()
    )
    if len(records) != row["feature_count"] or any(
        not r["valid"] or r["geom"] is None for r in records
    ):
        raise HTTPException(422, "Source contains invalid, missing or excessive geometry")
    features = [{"feature_id": r["feature_id"], "geometry": dict(r["geom"])} for r in records]
    changed_handles: tuple[str, ...] = ()
    if body.profile == "native-xy-dxf-v1":
        payload, changed_handles = gis_to_cad_preview(
            original,
            base_sha256=row["sha256"],
            placement=placement,
            mapping=row["entity_summary"]["entity_mapping"],
            features=features,
        )
        loss = {
            "derived_document": True,
            "original_document_unchanged": True,
            "attributes_omitted": True,
            "native_handles_not_preserved": False,
            "qualified_entity_properties_preserved": True,
            "other_native_metadata_not_certified": True,
            "geometry_only": True,
            "all_current_source_features_included": True,
            "survey_grade": False,
            "requires_acknowledgement": True,
        }
    else:
        local = restore_features(
            features, placement, document_id=document_id, version=version, source_crs_code=3857
        )
        payload = write_dxf_2d(local, layer_name=str(row["layer_name"]), units=placement.units)
        loss = {
            "derived_document": True,
            "original_document_unchanged": True,
            "attributes_omitted": True,
            "native_handles_not_preserved": True,
            "native_styles_layers_curves_blocks_text_and_unlinked_entities_not_preserved": True,
            "all_current_source_features_included": True,
            "survey_grade": False,
            "requires_acknowledgement": True,
        }
    if len(payload) > LIMIT:
        raise HTTPException(422, "Derived DXF exceeds 25 MiB")
    preview = {
        "document_id": str(document_id),
        "version": version,
        "document_sha256": row["sha256"],
        "conversion_id": str(body.conversion_id),
        "source_id": str(row["source_id"]),
        "layer_id": str(row["layer_id"]),
        "content_revision": row["content_revision"],
        "schema_revision": row["schema_revision"],
        "format": "dxf",
        "units": placement.units.value,
        "feature_count": len(features),
        "loss_report": loss,
        "published": False,
        "profile": body.profile,
        "changed_handles": list(changed_handles),
    }
    fingerprint = hashlib.sha256(
        json.dumps(
            {
                "profile": body.profile,
                "preview": preview,
                "placement": placement.metadata(),
                "features": features,
                "layer_name": row["layer_name"],
            },
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        ).encode()
    ).hexdigest()
    if body.expected_preview_fingerprint is None:
        if body.acknowledge_loss:
            raise HTTPException(422, "Review preview before acknowledging loss")
        return JSONResponse(
            {**preview, "preview_fingerprint": fingerprint}, headers={"Cache-Control": "no-store"}
        )
    if body.expected_preview_fingerprint != fingerprint:
        raise HTTPException(409, "Draft preview changed")
    if not body.acknowledge_loss:
        raise HTTPException(422, "Explicit derived DXF loss acknowledgement required")
    return Response(
        payload,
        media_type="application/dxf",
        headers={
            "Content-Disposition": f'attachment; filename="derived-{document_id}-v{version}.dxf"',
            "Cache-Control": "no-store",
            "X-Content-Type-Options": "nosniff",
            "X-Content-SHA256": hashlib.sha256(payload).hexdigest(),
            "X-CAD-Preview-Fingerprint": fingerprint,
        },
    )


class NativeImportDraftRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    conversion_id: UUID
    expected_candidate_sha256: str = Field(pattern=r"^[a-f0-9]{64}$")


@router.post(BASE + "/{document_id}/versions/{version}/preview-native-import")
def preview_native_import(
    organization_id: UUID,
    project_id: UUID,
    document_id: UUID,
    version: int,
    body: NativeImportDraftRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    """Review geometry edits from the next saved DXF revision using the R006 request shape."""
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    session.execute(
        text("SELECT pg_advisory_xact_lock_shared(hashtextextended(:key,0))"),
        {"key": "cad-doc:" + str(ctx.project_id)},
    )
    row = (
        session.execute(
            text("""
        SELECT c.entity_summary, c.source_id, c.content_revision AS imported_content_revision,
               s.content_revision, s.schema_revision,
               original.sha256 AS original_sha, original.storage_key AS original_key,
               candidate.sha256 AS candidate_sha, candidate.storage_key AS candidate_key
        FROM cad.conversions c
        JOIN cad.document_versions original ON original.organization_id=c.organization_id
          AND original.project_id=c.project_id AND original.document_id=:doc
          AND original.version=:version AND original.sha256=c.sha256
          AND original.document_id::text=c.entity_summary->>'cad_document_id'
          AND original.version::text=c.entity_summary->>'cad_document_version'
        JOIN cad.document_versions candidate ON candidate.organization_id=c.organization_id
          AND candidate.project_id=c.project_id AND candidate.document_id=:doc
          AND candidate.version=original.version+1
        JOIN spatial.data_sources s ON s.organization_id=c.organization_id
          AND s.project_id=c.project_id AND s.id=c.source_id
        JOIN spatial.layers l ON l.organization_id=c.organization_id
          AND l.project_id=c.project_id AND l.id=c.layer_id AND l.source_id=s.id
        WHERE c.organization_id=:org AND c.project_id=:proj AND c.id=:conversion
          AND c.direction='import' AND c.status='succeeded'
          AND original.format='dxf' AND candidate.format='dxf'
          AND s.lifecycle='active' AND s.validation_state='ready'
          AND s.stored_crs=3857 AND s.dimensions='XY' AND l.lifecycle='active'
          AND NOT EXISTS (
            SELECT 1 FROM cad.document_versions newer
            WHERE newer.organization_id=c.organization_id AND newer.project_id=c.project_id
              AND newer.document_id=:doc AND newer.version>candidate.version
          )
        FOR SHARE OF s,l
        """),
            {
                "org": ctx.organization_id,
                "proj": ctx.project_id,
                "doc": document_id,
                "version": version,
                "conversion": body.conversion_id,
            },
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(404, "Current next DXF revision and pinned CAD conversion required")
    if row["candidate_sha"] != body.expected_candidate_sha256:
        raise HTTPException(409, "Saved CAD candidate changed")
    summary = row["entity_summary"]
    imported_schema = summary.get("imported_schema_revision")
    if not isinstance(imported_schema, str):
        raise HTTPException(422, "CAD conversion lacks an immutable schema revision")
    store = _store()
    ops = cad_to_gis_preview(
        store.get_bytes(row["original_key"], expected_sha256=row["original_sha"]),
        store.get_bytes(row["candidate_key"], expected_sha256=row["candidate_sha"]),
        base_sha256=row["original_sha"],
        candidate_sha256=row["candidate_sha"],
        placement=_reviewed_placement(summary, document_id, version),
        mapping=summary["entity_mapping"],
        imported_content_revision=row["imported_content_revision"],
        current_content_revision=row["content_revision"],
        imported_schema_revision=imported_schema,
        current_schema_revision=row["schema_revision"],
    )
    if ops:
        invalid = session.execute(
            text("""
                SELECT item->>'feature_id'
                FROM jsonb_array_elements(CAST(:operations AS jsonb)) AS item
                CROSS JOIN LATERAL (
                    SELECT ST_GeomFromGeoJSON(item->'geometry') AS geom
                ) AS shape
                WHERE ST_IsEmpty(shape.geom) OR NOT ST_IsValid(shape.geom)
                   OR ST_NPoints(shape.geom) > 4096
                LIMIT 1
            """),
            {
                "operations": json.dumps(
                    [{"feature_id": op.feature_id, "geometry": op.geometry} for op in ops],
                    allow_nan=False,
                )
            },
        ).scalar_one_or_none()
        if invalid is not None:
            raise HTTPException(422, f"Invalid or oversized CAD geometry for {invalid}")
    return JSONResponse(
        {
            "conversion_id": str(body.conversion_id),
            "source_id": str(row["source_id"]),
            "candidate_sha256": row["candidate_sha"],
            "candidate_version": version + 1,
            "base_content_revision": row["content_revision"],
            "base_schema_revision": row["schema_revision"],
            "operations": [
                {"op_type": op.op_type.value, "feature_id": op.feature_id, "geometry": op.geometry}
                for op in ops
            ],
            "published": False,
        },
        headers={"Cache-Control": "no-store"},
    )
