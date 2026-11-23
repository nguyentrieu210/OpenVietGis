"""Version-pinned project scene definitions; no persisted model or raster duplication."""

import json
from datetime import UTC, datetime
from typing import Annotated, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException, Response
from pydantic import BaseModel, ConfigDict, Field, model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.model_vault import HEADERS, _scope
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import append_project_audit
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["saved-3d-scenes"])
BASE = "/organizations/{organization_id}/projects/{project_id}/3d/scenes"
WHERE = "organization_id=:org AND project_id=:proj"


class Strict(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)


class ModelPin(Strict):
    id: UUID
    version: int = Field(ge=1, le=50)
    visible: bool = True


class Camera(Strict):
    crs_code: Literal[4326]
    longitude: float = Field(ge=-180, le=180)
    latitude: float = Field(ge=-85, le=85)
    zoom: float = Field(ge=0, le=22)
    bearing: float = Field(ge=-180, le=180)
    pitch: float = Field(ge=0, le=75)


class Dem(Strict):
    id: UUID
    vertical_reference: Literal["local", "orthometric", "ellipsoidal"]
    reference_label: str = Field(min_length=1, max_length=120)
    visible: bool = True


class Footprint(Strict):
    layer_id: UUID
    height_field: str = Field(min_length=1, max_length=64)
    visible: bool = True


class SceneProfile(Strict):
    name: str = Field(min_length=1, max_length=160)
    models: list[ModelPin] = Field(default_factory=list, max_length=8)
    camera: Camera
    dem: Dem | None = None
    footprint: Footprint | None = None

    @model_validator(mode="after")
    def check_scene(self):
        self.name = self.name.strip()
        if not self.name or not (self.models or self.dem or self.footprint):
            raise ValueError("Name and scene content required")
        if len({m.id for m in self.models}) != len(self.models):
            raise ValueError("Duplicate model")
        if self.dem:
            self.dem.reference_label = self.dem.reference_label.strip()
            if not self.dem.reference_label:
                raise ValueError("DEM reference required")
        return self


class Revise(SceneProfile):
    expected_version: int = Field(ge=1, le=50)


def _resolved(session, args, profile):
    # Recheck live references on opening, not only at saving. Exact model revisions are immutable.
    dem = profile.dem
    if dem:
        info = session.execute(
            text(f"SELECT metadata FROM raster.layers WHERE {WHERE} AND id=:id"),
            args | {"id": dem.id},
        ).scalar_one_or_none()
        if not info or info.get("method") != "band" or info.get("output_bands", 1) != 1:
            raise HTTPException(409, "Scene DEM no longer available")
    if profile.footprint:
        f = profile.footprint
        found = session.execute(
            text("""
            SELECT l.id FROM spatial.layers l JOIN spatial.data_sources s
            ON s.organization_id=l.organization_id AND s.project_id=l.project_id
            AND s.id=l.source_id
            JOIN spatial.source_fields sf ON sf.organization_id=s.organization_id
            AND sf.project_id=s.project_id AND sf.source_id=s.id
            AND sf.schema_revision=s.schema_revision
            WHERE l.organization_id=:org AND l.project_id=:proj AND l.id=:id
                AND l.lifecycle='active' AND s.lifecycle='active' AND s.validation_state='ready'
                AND s.geometry_family='Polygon' AND sf.field_id=:field
                AND sf.type IN ('integer','decimal')
            """),
            args | {"id": f.layer_id, "field": f.height_field},
        ).scalar_one_or_none()
        if found is None:
            raise HTTPException(409, "Scene GIS height field no longer available")
    models = []
    for pin in profile.models:
        row = (
            session.execute(
                text("""
            SELECT a.id,a.filename,a.format,a.sha256,a.byte_size,p.version,p.profile,p.created_at
            FROM spatial.model_assets a JOIN spatial.model_placements p
            ON p.organization_id=a.organization_id AND p.project_id=a.project_id AND p.model_id=a.id
            WHERE a.organization_id=:org AND a.project_id=:proj AND a.id=:id AND p.version=:version
            """),
                args | {"id": pin.id, "version": pin.version},
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            raise HTTPException(409, "Pinned model revision unavailable")
        p = row["profile"]
        if p.get("dem_id") != (str(dem.id) if dem else None) or (
            dem
            and (
                p.get("vertical_reference") != dem.vertical_reference
                or p.get("reference_label") != dem.reference_label
            )
        ):
            raise HTTPException(422, "Model and scene elevation references differ")
        models.append(
            dict(row) | {"id": str(row["id"]), "created_at": row["created_at"].isoformat()}
        )
    return models


def _rows(session, args, scene_id=None, version=None, history=False):
    sql = f"""SELECT scene_id,version,name,profile,created_at
        FROM spatial.scene_versions s WHERE {WHERE}"""
    if scene_id is not None:
        sql += " AND scene_id=:scene"
    if version is not None:
        sql += " AND version=:version"
    elif not history:
        sql += """ AND version=(SELECT max(v.version) FROM spatial.scene_versions v
          WHERE v.organization_id=s.organization_id AND v.project_id=s.project_id
          AND v.scene_id=s.scene_id)"""
    sql += " ORDER BY created_at DESC,version DESC,scene_id"
    return [
        {
            "id": str(r["scene_id"]),
            "version": r["version"],
            "name": r["name"],
            "profile": r["profile"],
            "created_at": r["created_at"].isoformat(),
        }
        for r in session.execute(
            text(sql), args | {"scene": scene_id, "version": version}
        ).mappings()
    ]


def _save(session, ctx, args, scene_id, body, expected):
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": "scene-vault:" + str(args["proj"])},
    )
    current = _rows(session, args, scene_id)
    if expected:
        if not current:
            raise HTTPException(404, "Scene not found")
        if current[0]["version"] != expected:
            raise HTTPException(409, "Scene changed; reload")
        if expected >= 50:
            raise HTTPException(413, "Scene history limit")
    else:
        count = session.execute(
            text(f"SELECT count(DISTINCT scene_id) FROM spatial.scene_versions WHERE {WHERE}"), args
        ).scalar_one()
        if count >= 100:
            raise HTTPException(413, "Project scene quota exceeded")
    _resolved(session, args, body)
    profile = body.model_dump(mode="json", exclude={"name", "expected_version"})
    session.execute(
        text("""INSERT INTO spatial.scene_versions
        (organization_id,project_id,scene_id,version,name,profile,actor_id)
        VALUES(:org,:proj,:scene,:version,:name,CAST(:profile AS jsonb),:actor)"""),
        args
        | {
            "scene": scene_id,
            "version": expected + 1,
            "name": body.name,
            "profile": json.dumps(profile, allow_nan=False),
            "actor": ctx.actor.actor_id,
        },
    )
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="3d.scene.save",
        resource_type="scene_3d",
        resource_id=scene_id,
        metadata={"version": expected + 1, "name": body.name, "profile": profile},
        created_at=datetime.now(UTC),
    )
    result = _rows(session, args, scene_id, expected + 1)[0]
    session.commit()
    return result


@router.get(BASE)
def list_scenes(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    return {"items": _rows(session, {"org": organization_id, "proj": project_id})}


@router.post(BASE, status_code=201)
def create_scene(
    organization_id: UUID,
    project_id: UUID,
    body: SceneProfile,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    return _save(session, ctx, {"org": organization_id, "proj": project_id}, uuid4(), body, 0)


@router.post(BASE + "/{scene_id}/versions", status_code=201)
def revise_scene(
    organization_id: UUID,
    project_id: UUID,
    scene_id: UUID,
    body: Revise,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    return _save(
        session,
        ctx,
        {"org": organization_id, "proj": project_id},
        scene_id,
        body,
        body.expected_version,
    )


@router.get(BASE + "/{scene_id}/versions")
def history(
    organization_id: UUID,
    project_id: UUID,
    scene_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    rows = _rows(session, {"org": organization_id, "proj": project_id}, scene_id, history=True)
    if not rows:
        raise HTTPException(404, "Scene not found")
    return {"items": rows}


@router.get(BASE + "/{scene_id}/versions/{version}")
def open_scene(
    organization_id: UUID,
    project_id: UUID,
    scene_id: UUID,
    version: int,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    args = {"org": organization_id, "proj": project_id}
    rows = _rows(session, args, scene_id, version)
    if not rows:
        raise HTTPException(404, "Scene version not found")
    row = rows[0]
    profile = SceneProfile.model_validate(row["profile"] | {"name": row["name"]})
    return {"scene": row, "models": _resolved(session, args, profile)}
