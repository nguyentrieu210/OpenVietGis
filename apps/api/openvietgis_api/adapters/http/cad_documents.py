"""Native CAD file vault. Does not parse, execute, publish, or georeference drawings."""

import re
from collections.abc import Mapping
from datetime import UTC, datetime
from pathlib import Path
from typing import Annotated, Any
from urllib.parse import quote
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, HTTPException, Response, UploadFile
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.storage.local_object_store import LocalObjectStore
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission

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
        result = _wire(row)
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
    return {"items": [_wire(r) for r in rows]}


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
    return {"items": [_wire(r) for r in rows]}


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
