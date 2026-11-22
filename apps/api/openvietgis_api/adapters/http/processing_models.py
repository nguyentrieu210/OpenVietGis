"""Validated reusable processing DAGs; executions reuse existing project processing APIs."""

import math
import re
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
from openvietgis_api.domains.processing.domain import AlgorithmId, validate_algorithm_parameters

router = APIRouter(tags=["processing-models"])
BASE = "/organizations/{organization_id}/projects/{project_id}/processing/models"
VECTOR = {"buffer", "clip", "dissolve"}
RASTER = {
    "band",
    "ndvi",
    "ndwi",
    "ndbi",
    "hillshade",
    "slope",
    "aspect",
    "resample",
    "mosaic",
    "change",
    "rgb",
    "calculator",
}
TWO = {"clip", "ndvi", "ndwi", "ndbi", "mosaic", "change", "rgb"}
COLS = "id,name,graph,revision,created_at,updated_at"
WHERE = "organization_id=:o AND project_id=:p"


class Ref(BaseModel):
    model_config = ConfigDict(extra="forbid")
    type: Literal["layer", "step"]
    id: UUID


class Node(BaseModel):
    model_config = ConfigDict(extra="forbid")
    id: UUID
    algorithm: Literal[
        "buffer",
        "clip",
        "dissolve",
        "band",
        "ndvi",
        "ndwi",
        "ndbi",
        "hillshade",
        "slope",
        "aspect",
        "resample",
        "mosaic",
        "change",
        "rgb",
        "calculator",
    ]
    name: str = Field(min_length=1, max_length=120)
    a: Ref
    b: Ref | None = None
    c: Ref | None = None
    parameters: dict[str, str | int | float | None] = Field(default_factory=dict)
    x: int = Field(default=0, ge=0, le=5000)
    y: int = Field(default=0, ge=0, le=5000)

    @model_validator(mode="after")
    def validate_node(self):
        self.name = self.name.strip()
        if not self.name:
            raise ValueError("Output name is required")
        alg = self.algorithm
        p = self.parameters
        uses_b = alg in TWO or (
            alg == "calculator" and bool(re.search(r"\bB\b", str(p.get("formula", ""))))
        )
        if bool(self.b) != uses_b or bool(self.c) != (alg == "rgb"):
            raise ValueError("Unexpected or missing input port")
        if alg in VECTOR:
            try:
                self.parameters = validate_algorithm_parameters(AlgorithmId(alg), p)
            except Exception as exc:
                raise ValueError(str(exc)) from exc
        else:
            allowed = {"band_a"}
            if uses_b:
                allowed.add("band_b")
            if alg == "rgb":
                allowed.add("band_c")
            if alg == "resample":
                allowed |= {"factor", "resampling"}
            if alg == "calculator":
                allowed.add("formula")
            if p.keys() - allowed:
                raise ValueError("Unknown raster parameter")
            for k in allowed & {"band_a", "band_b", "band_c"}:
                v = p.get(k, 1)
                if type(v) is not int or not 1 <= v <= 16:
                    raise ValueError("Band must be an integer 1–16")
                p[k] = v
            if alg == "resample":
                factor = p.get("factor")
                if (
                    type(factor) not in (int, float)
                    or not math.isfinite(factor)
                    or not 0.1 <= factor <= 10
                    or p.get("resampling") not in ("near", "bilinear")
                ):
                    raise ValueError("Invalid resample parameters")
            if alg == "calculator":
                f = p.get("formula")
                if not isinstance(f, str) or not 1 <= len(f) <= 200 or not re.search(r"\bA\b", f):
                    raise ValueError("Invalid calculator formula")
        return self


class Graph(BaseModel):
    model_config = ConfigDict(extra="forbid")
    version: Literal[1] = 1
    nodes: list[Node] = Field(min_length=1, max_length=12)

    @model_validator(mode="after")
    def dag(self):
        nodes = {n.id: n for n in self.nodes}
        if len(nodes) != len(self.nodes):
            raise ValueError("Duplicate step")
        done = set()
        pending = set(nodes)
        for n in self.nodes:
            for r in (n.a, n.b, n.c):
                if r is None or r.type != "step":
                    continue
                if r.id not in nodes:
                    raise ValueError("Missing referenced step")
                if (nodes[r.id].algorithm in VECTOR) != (n.algorithm in VECTOR):
                    raise ValueError("Vector/raster ports cannot connect")
        while pending:
            ready = {
                i
                for i in pending
                if all(
                    r is None or r.type != "step" or r.id in done
                    for r in (nodes[i].a, nodes[i].b, nodes[i].c)
                )
            }
            if not ready:
                raise ValueError("Processing model contains a cycle")
            done |= ready
            pending -= ready
        return self


class ModelRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=120)
    graph: Graph


class UpdateRequest(ModelRequest):
    revision: int = Field(ge=1)


def scope(session, auth, o, p, permission):
    ctx = context_for(auth, organization_id=o, project_id=p)
    prepare_project_scope(session, ctx, permission=permission)
    if scoped(session, ctx):
        deny()
    return ctx


def wire(row):
    return dict(
        id=str(row["id"]),
        name=row["name"],
        graph=row["graph"],
        revision=row["revision"],
        created_at=row["created_at"].isoformat(),
        updated_at=row["updated_at"].isoformat(),
    )


def values(body):
    name = body.name.strip()
    if not name:
        raise HTTPException(422, "Model name is required")
    return {"name": name, "graph": body.graph.model_dump_json()}


def audit(session, ctx, rid, action, name):
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action=action,
        resource_type="processing_model",
        resource_id=rid,
        metadata={"name": name},
        created_at=datetime.now(UTC),
    )


@router.get(BASE)
def list_models(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = session.execute(
        text(f"SELECT {COLS} FROM processing.models WHERE {WHERE} ORDER BY updated_at DESC,id"),
        {"o": organization_id, "p": project_id},
    ).mappings()
    return {"items": [wire(r) for r in rows]}


@router.post(BASE, status_code=201)
def create_model(
    organization_id: UUID,
    project_id: UUID,
    body: ModelRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    args = {"o": organization_id, "p": project_id}
    # Serialize the project quota check, even for concurrent writers.
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:project,0))"),
        {"project": "processing-model:" + str(project_id)},
    )
    if (
        session.execute(
            text(f"SELECT count(*) FROM processing.models WHERE {WHERE}"), args
        ).scalar_one()
        >= 50
    ):
        raise HTTPException(413, "Maximum 50 models per project")
    rid = uuid4()
    row = (
        session.execute(
            text(
                "INSERT INTO processing.models(organization_id,project_i"
                "d,id,name,graph) VALUES(:o,:p,:id,:name,CAST(:graph AS "
                "jsonb)) RETURNING "
                f"{COLS}"
            ),
            args | values(body) | {"id": rid},
        )
        .mappings()
        .one()
    )
    audit(session, ctx, rid, "processing.model.create", row["name"])
    result = wire(row)
    session.commit()
    return result


@router.put(BASE + "/{model_id}")
def update_model(
    organization_id: UUID,
    project_id: UUID,
    model_id: UUID,
    body: UpdateRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    row = (
        session.execute(
            text(
                "UPDATE processing.models SET name=:name,graph=CAST(:gra"
                "ph AS jsonb),revision=revision+1,updated_at=now() WHERE "
                f"{WHERE}"
                " AND id=:id AND revision=:rev RETURNING "
                f"{COLS}"
            ),
            {"o": organization_id, "p": project_id, "id": model_id, "rev": body.revision}
            | values(body),
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(409, "Model changed or removed; reload the catalog")
    audit(session, ctx, model_id, "processing.model.update", row["name"])
    result = wire(row)
    session.commit()
    return result


@router.delete(BASE + "/{model_id}", status_code=204)
def delete_model(
    organization_id: UUID,
    project_id: UUID,
    model_id: UUID,
    revision: int,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    name = session.execute(
        text(
            "DELETE FROM processing.models WHERE "
            f"{WHERE}"
            " AND id=:id AND revision=:rev RETURNING name"
        ),
        {"o": organization_id, "p": project_id, "id": model_id, "rev": revision},
    ).scalar_one_or_none()
    if name is None:
        raise HTTPException(409, "Model changed or removed; reload the catalog")
    audit(session, ctx, model_id, "processing.model.delete", name)
    session.commit()
