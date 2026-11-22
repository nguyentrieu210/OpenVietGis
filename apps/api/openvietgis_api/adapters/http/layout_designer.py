"""Saved Layout Designer documents; no embedded remote URLs or untrusted images."""

from datetime import UTC, datetime
from typing import Annotated, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, ConfigDict, Field, model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.gis_access import deny, scoped
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["layout-designer"])
BASE = "/organizations/{organization_id}/projects/{project_id}/cartography/designer"
PAPERS = {"A0": (841, 1189), "A1": (594, 841), "A2": (420, 594), "A3": (297, 420), "A4": (210, 297)}
COLS = "id,name,document,revision,created_at,updated_at"
WHERE = "organization_id=:o AND project_id=:p"


class Camera(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)
    lng: float = Field(ge=-180, le=180)
    lat: float = Field(ge=-85, le=85)
    zoom: float = Field(ge=0, le=24)
    bearing: float = Field(ge=-180, le=180)
    reference_width: float = Field(ge=1, le=100000)


class Element(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)
    id: UUID
    kind: Literal["map", "text", "legend", "north", "scale", "rect"]
    x: float = Field(ge=0, le=1200)
    y: float = Field(ge=0, le=1200)
    w: float = Field(ge=5, le=1200)
    h: float = Field(ge=5, le=1200)
    text: str = Field(default="", max_length=500)
    font_size: float = Field(default=4, ge=2, le=40)
    color: str = Field(default="#15383b", pattern=r"^#[0-9a-fA-F]{6}$")
    border: bool = True
    grid: bool = False
    camera: Camera | None = None
    map_id: UUID | None = None

    @model_validator(mode="after")
    def valid_kind(self):
        if (self.camera is not None) != (self.kind == "map"):
            raise ValueError("Camera required for map frames only")
        if self.map_id is not None and self.kind not in ("north", "scale"):
            raise ValueError("Only north/scale elements link to maps")
        return self


class Document(BaseModel):
    model_config = ConfigDict(extra="forbid")
    version: Literal[1] = 1
    paper: Literal["A0", "A1", "A2", "A3", "A4"] = "A4"
    landscape: bool = True
    dpi: Literal[150, 300] = 150
    items: list[Element] = Field(min_length=1, max_length=40)

    @model_validator(mode="after")
    def valid_items(self):
        w, h = PAPERS[self.paper]
        if self.landscape:
            w, h = h, w
        if self.paper not in ("A3", "A4") and self.dpi != 150:
            raise ValueError("A0–A2 limited to 150 DPI")
        ids = {i.id: i for i in self.items}
        if len(ids) != len(self.items):
            raise ValueError("Duplicate item ID")
        maps = [i for i in self.items if i.kind == "map"]
        if not 1 <= len(maps) <= 4:
            raise ValueError("Layout requires 1–4 map frames")
        for i in self.items:
            if i.x + i.w > w + 0.0001 or i.y + i.h > h + 0.0001:
                raise ValueError("Element outside paper")
            if i.kind in ("scale", "north") and (
                i.map_id not in ids or ids[i.map_id].kind != "map"
            ):
                raise ValueError("Scale/north must reference a map")
        return self


class Create(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=120)
    document: Document


class Update(Create):
    revision: int = Field(ge=1)


def scope(session, auth, o, p, permission):
    ctx = context_for(auth, organization_id=o, project_id=p)
    prepare_project_scope(session, ctx, permission=permission)
    if scoped(session, ctx):
        deny()
    return ctx


def values(body):
    name = body.name.strip()
    if not name:
        raise HTTPException(422, "Layout name is required")
    return {"name": name, "document": body.document.model_dump_json()}


def wire(row):
    return dict(
        id=str(row["id"]),
        name=row["name"],
        document=row["document"],
        revision=row["revision"],
        created_at=row["created_at"].isoformat(),
        updated_at=row["updated_at"].isoformat(),
    )


def audit(session, ctx, rid, action, name):
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action=action,
        resource_type="designer_layout",
        resource_id=rid,
        metadata={"name": name},
        created_at=datetime.now(UTC),
    )


@router.get(BASE)
def list_designer(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = session.execute(
        text(
            "SELECT "
            f"{COLS}"
            " FROM cartography.designer_layouts WHERE "
            f"{WHERE}"
            " ORDER BY updated_at DESC,id"
        ),
        {"o": organization_id, "p": project_id},
    ).mappings()
    return {"items": [wire(r) for r in rows]}


@router.post(BASE, status_code=201)
def create_designer(
    organization_id: UUID,
    project_id: UUID,
    body: Create,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    args = {"o": organization_id, "p": project_id}
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": "layout-designer:" + str(project_id)},
    )
    if (
        session.execute(
            text(f"SELECT count(*) FROM cartography.designer_layouts WHERE {WHERE}"), args
        ).scalar_one()
        >= 50
    ):
        raise HTTPException(413, "Maximum 50 layouts per project")
    rid = uuid4()
    row = (
        session.execute(
            text(
                "INSERT INTO cartography.designer_layouts(organization_i"
                "d,project_id,id,name,document) VALUES(:o,:p,:id,:name,C"
                "AST(:document AS jsonb)) RETURNING "
                f"{COLS}"
            ),
            args | values(body) | {"id": rid},
        )
        .mappings()
        .one()
    )
    audit(session, ctx, rid, "cartography.designer.create", row["name"])
    result = wire(row)
    session.commit()
    return result


@router.put(BASE + "/{layout_id}")
def update_designer(
    organization_id: UUID,
    project_id: UUID,
    layout_id: UUID,
    body: Update,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    row = (
        session.execute(
            text(
                "UPDATE cartography.designer_layouts SET "
                "name=:name,document=CAST(:document AS "
                "jsonb),revision=revision+1,updated_at=now() WHERE "
                f"{WHERE}"
                " AND id=:id AND revision=:rev RETURNING "
                f"{COLS}"
            ),
            {"o": organization_id, "p": project_id, "id": layout_id, "rev": body.revision}
            | values(body),
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(409, "Layout changed or removed; reload")
    audit(session, ctx, layout_id, "cartography.designer.update", row["name"])
    result = wire(row)
    session.commit()
    return result


@router.delete(BASE + "/{layout_id}", status_code=204)
def delete_designer(
    organization_id: UUID,
    project_id: UUID,
    layout_id: UUID,
    revision: int,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    name = session.execute(
        text(
            "DELETE FROM cartography.designer_layouts WHERE "
            f"{WHERE}"
            " AND id=:id AND revision=:rev RETURNING name"
        ),
        {"o": organization_id, "p": project_id, "id": layout_id, "rev": revision},
    ).scalar_one_or_none()
    if name is None:
        raise HTTPException(409, "Layout changed or removed; reload")
    audit(session, ctx, layout_id, "cartography.designer.delete", name)
    session.commit()
