"""Project model vault: immutable bytes, fixed WGS84 anchors, versioned placement."""

import base64
import hashlib
import json
import re
import struct
from datetime import UTC, datetime
from typing import Annotated, Literal
from urllib.parse import quote
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, HTTPException, Response, UploadFile
from pydantic import BaseModel, ConfigDict, Field, ValidationError, model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.raster import raster_scope
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import append_project_audit
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["model-vault"])
BASE = "/organizations/{organization_id}/projects/{project_id}/3d/models"
LIMIT = 10 * 1024 * 1024
QUOTA = 100 * 1024 * 1024
HEADERS = {"Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff"}


class Placement(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=200)
    scale: float = Field(ge=0.01, le=100)
    rotation_deg: float = Field(ge=-360, le=360)
    offset_m: float = Field(ge=-1000, le=1000)
    dem_id: UUID | None = None
    vertical_reference: Literal["local", "orthometric", "ellipsoidal"] = "local"
    reference_label: str = Field(default="", max_length=120)

    @model_validator(mode="after")
    def reference(self):
        self.reference_label = self.reference_label.strip()
        if self.dem_id is not None and not self.reference_label:
            raise ValueError("DEM vertical reference required")
        if self.dem_id is None and (self.reference_label or self.vertical_reference != "local"):
            raise ValueError("Flat placement requires local zero reference")
        return self


class Revise(Placement):
    expected_version: int = Field(ge=1, le=50)


def _scope(session, auth, org, project, permission):
    ctx = raster_scope(session, auth, org, project, permission)
    if ctx.actor.actor_type != "human":
        raise HTTPException(403, "Model vault requires human access")
    return ctx


def _validate(filename, payload):
    if not filename or len(filename) > 180 or re.search(r"[/\\\x00-\x1f\x7f]", filename):
        raise HTTPException(422, "Invalid model filename")
    fmt = filename.rsplit(".", 1)[-1].lower()
    if "." not in filename or fmt not in ("glb", "gltf"):
        raise HTTPException(422, "Only GLB/GLTF supported")
    if not payload or len(payload) > LIMIT:
        raise HTTPException(413, "Model size limit")
    # Envelope validation only; rendering uses the independently bounded static mesh parser.
    try:
        if fmt == "glb":
            magic, version, size, n, kind = struct.unpack_from("<5I", payload)
            if (magic, version, size, kind) != (0x46546C67, 2, len(payload), 0x4E4F534A) or n % 4:
                raise ValueError()
            end = 20 + n
            data = json.loads(payload[20:end].decode("utf-8"))
            length, kind = struct.unpack_from("<2I", payload, end)
            if kind != 0x004E4942 or length % 4 or end + 8 + length != len(payload):
                raise ValueError()
        else:
            data = json.loads(payload.decode("utf-8"))
            uri = data["buffers"][0]["uri"]
            if not isinstance(uri, str) or not re.fullmatch(
                r"data:application/(?:octet-stream|gltf-buffer);base64,[A-Za-z0-9+/]+={0,2}", uri
            ):
                raise ValueError()
            length = len(base64.b64decode(uri.split(",", 1)[1], validate=True))
        if data["asset"]["version"] != "2.0" or len(data["buffers"]) != 1:
            raise ValueError()
        buf = data["buffers"][0]
        used = buf["byteLength"]
        if type(used) is not int or not 0 < used <= length or length - used > 3:
            raise ValueError()
        if fmt == "glb" and "uri" in buf:
            raise ValueError()
        if any(
            data.get(k)
            for k in (
                "extensionsUsed",
                "extensionsRequired",
                "images",
                "textures",
                "skins",
                "animations",
                "cameras",
            )
        ):
            raise ValueError()
        for key, maximum in (
            ("nodes", 256),
            ("meshes", 256),
            ("bufferViews", 1024),
            ("accessors", 1024),
            ("scenes", 256),
        ):
            if not isinstance(data[key], list) or not 0 < len(data[key]) <= maximum:
                raise ValueError()
    except (
        ValueError,
        TypeError,
        KeyError,
        IndexError,
        AttributeError,
        struct.error,
        RecursionError,
    ):
        raise HTTPException(422, "Invalid or unsupported self-contained static model") from None
    return fmt


def _profile(session, args, body):
    # Do not silently infer datum operations. 4326 or the standard 3857 inverse only.
    row = (
        session.execute(
            text("""
        SELECT s.content_revision, ST_XMin(g) AS xmin, ST_XMax(g) AS xmax,
            ST_YMin(g) AS ymin, ST_YMax(g) AS ymax
        FROM spatial.reference_features f
        JOIN spatial.data_sources s ON s.organization_id=f.organization_id
            AND s.project_id=f.project_id AND s.id=f.source_id
        CROSS JOIN LATERAL (SELECT CASE ST_SRID(f.geometry)
            WHEN 4326 THEN f.geometry WHEN 3857 THEN ST_Transform(f.geometry,4326)
            ELSE NULL END AS g) q
        WHERE f.organization_id=:org AND f.project_id=:proj AND f.source_id=:src
            AND f.feature_id=:fid AND s.lifecycle='active' AND s.validation_state='ready'
            AND f.content_revision=s.content_revision AND g IS NOT NULL AND NOT ST_IsEmpty(g)
            AND EXISTS (SELECT 1 FROM spatial.layers l WHERE l.organization_id=f.organization_id
                AND l.project_id=f.project_id AND l.source_id=f.source_id AND l.lifecycle='active')
        """),
            args | {"src": body.source_id, "fid": body.feature_id},
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(422, "Live reference feature with EPSG:4326 or EPSG:3857 required")
    west, east, south, north = (row[k] for k in ("xmin", "xmax", "ymin", "ymax"))
    if not (-180 <= west <= east <= 180 and east - west <= 180 and -85 <= south <= north <= 85):
        raise HTTPException(422, "Unsupported feature extent")
    if body.dem_id:
        dem = session.execute(
            text("""SELECT metadata FROM raster.layers
            WHERE organization_id=:org AND project_id=:proj AND id=:dem"""),
            args | {"dem": body.dem_id},
        ).scalar_one_or_none()
        if not dem or dem.get("method") != "band" or dem.get("output_bands", 1) != 1:
            raise HTTPException(422, "DEM not found or ineligible")
    return body.model_dump(mode="json", exclude={"expected_version"}) | {
        "longitude": (west + east) / 2,
        "latitude": (south + north) / 2,
        "crs_code": 4326,
        "source_revision": row["content_revision"],
        "altitude_mode": "relative-dem" if body.dem_id else "relative-flat",
    }


def _lock(session, proj):
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": "model-vault:" + str(proj)},
    )


def _placement(session, ctx, args, version, profile):
    session.execute(
        text("""INSERT INTO spatial.model_placements
        (organization_id,project_id,model_id,version,profile,actor_id)
        VALUES(:org,:proj,:id,:version,CAST(:profile AS jsonb),:actor)"""),
        args
        | {
            "version": version,
            "profile": json.dumps(profile, allow_nan=False),
            "actor": ctx.actor.actor_id,
        },
    )
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="3d.model.placement.save",
        resource_type="model_asset",
        resource_id=args["id"],
        metadata={"version": version, "profile": profile},
        created_at=datetime.now(UTC),
    )


def _items(session, args, model_id=None, history=False):
    sql = """SELECT a.id,a.filename,a.format,a.sha256,a.byte_size,p.version,p.profile,p.created_at
        FROM spatial.model_assets a JOIN spatial.model_placements p
        ON p.organization_id=a.organization_id AND p.project_id=a.project_id AND p.model_id=a.id
        WHERE a.organization_id=:org AND a.project_id=:proj"""
    if model_id is not None:
        sql += " AND a.id=:id"
    if not history:
        sql += """ AND p.version=(SELECT max(x.version) FROM spatial.model_placements x
            WHERE x.organization_id=a.organization_id AND x.project_id=a.project_id
            AND x.model_id=a.id)"""
    sql += " ORDER BY p.created_at DESC,p.version DESC,a.id"
    rows = session.execute(text(sql), args | {"id": model_id}).mappings()
    return [dict(r) | {"id": str(r["id"]), "created_at": r["created_at"].isoformat()} for r in rows]


@router.get(BASE)
def list_models(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    return {"items": _items(session, {"org": organization_id, "proj": project_id})}


@router.post(BASE, status_code=201)
def create_model(
    organization_id: UUID,
    project_id: UUID,
    file: Annotated[UploadFile, File()],
    placement: Annotated[str, Form(max_length=4096)],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    try:
        body = Placement.model_validate_json(placement)
    except ValidationError:
        raise HTTPException(422, "Invalid placement") from None
    data = file.file.read(LIMIT + 1)
    filename = file.filename or ""
    fmt = _validate(filename, data)
    args = {"org": organization_id, "proj": project_id, "id": uuid4()}
    _lock(session, project_id)
    count, total = session.execute(
        text("""SELECT count(*),coalesce(sum(byte_size),0)
        FROM spatial.model_assets WHERE organization_id=:org AND project_id=:proj"""),
        args,
    ).one()
    if count >= 100 or total + len(data) > QUOTA:
        raise HTTPException(413, "Project model quota exceeded")
    profile = _profile(session, args, body)
    session.execute(
        text("""INSERT INTO spatial.model_assets
      (organization_id,project_id,id,filename,format,sha256,byte_size,payload,actor_id)
      VALUES(:org,:proj,:id,:name,:fmt,:sha,:size,:payload,:actor)"""),
        args
        | {
            "name": filename,
            "fmt": fmt,
            "sha": hashlib.sha256(data).hexdigest(),
            "size": len(data),
            "payload": data,
            "actor": ctx.actor.actor_id,
        },
    )
    _placement(session, ctx, args, 1, profile)
    result = _items(session, args, args["id"])[0]
    session.commit()
    return result


@router.post(BASE + "/{model_id}/placements", status_code=201)
def revise_model(
    organization_id: UUID,
    project_id: UUID,
    model_id: UUID,
    body: Revise,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    args = {"org": organization_id, "proj": project_id, "id": model_id}
    _lock(session, project_id)
    current = _items(session, args, model_id)
    if not current:
        raise HTTPException(404, "Model not found")
    if current[0]["version"] != body.expected_version:
        raise HTTPException(409, "Placement changed; reload")
    if body.expected_version >= 50:
        raise HTTPException(413, "Placement history limit")
    profile = _profile(session, args, body)
    _placement(session, ctx, args, body.expected_version + 1, profile)
    result = _items(session, args, model_id)[0]
    session.commit()
    return result


@router.get(BASE + "/{model_id}/placements")
def history(
    organization_id: UUID,
    project_id: UUID,
    model_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    rows = _items(session, {"org": organization_id, "proj": project_id}, model_id, True)
    if not rows:
        raise HTTPException(404, "Model not found")
    return {"items": rows}


@router.get(BASE + "/{model_id}/download")
def download(
    organization_id: UUID,
    project_id: UUID,
    model_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    row = (
        session.execute(
            text("""SELECT filename,sha256,payload FROM spatial.model_assets
        WHERE organization_id=:org AND project_id=:proj AND id=:id"""),
            {"org": organization_id, "proj": project_id, "id": model_id},
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(404, "Model not found")
    data = bytes(row["payload"])
    if hashlib.sha256(data).hexdigest() != row["sha256"]:
        raise HTTPException(409, "Model integrity check failed")
    return Response(
        data,
        media_type="application/octet-stream",
        headers=HEADERS
        | {
            "Content-Disposition": "attachment; filename*=UTF-8''" + quote(row["filename"], safe="")
        },
    )
