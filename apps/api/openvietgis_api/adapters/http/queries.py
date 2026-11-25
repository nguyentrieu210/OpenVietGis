"""R007 Attribute & Spatial Query HTTP adapter."""

from __future__ import annotations

from typing import Annotated, Any
from uuid import UUID

from fastapi import APIRouter, Depends, Query
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.query_repository import SqlQueryRepository
from openvietgis_api.domains.queries.domain import (
    MAX_QUERY_LIMIT,
    MAX_QUERY_OFFSET,
    SavedQueryRecord,
)

router = APIRouter(tags=["queries"])
_BASE = "/organizations/{organization_id}/projects/{project_id}"


def _saved_wire(q: SavedQueryRecord) -> dict[str, Any]:
    return {
        "id": str(q.id),
        "organization_id": str(q.organization_id),
        "project_id": str(q.project_id),
        "layer_id": str(q.layer_id),
        "name": q.name,
        "description": q.description,
        "attribute_filter": q.attribute_filter,
        "spatial_filter": q.spatial_filter,
        "sort_field": q.sort_field,
        "sort_direction": q.sort_direction,
        "created_by_actor_id": str(q.created_by_actor_id),
        "created_at": q.created_at.isoformat(),
        "updated_at": q.updated_at.isoformat(),
    }


class ExecuteQueryRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    attribute_filter: dict[str, Any] | None = None
    spatial_filter: dict[str, Any] | None = None
    expected_content_revision: str | None = None
    expected_schema_revision: str | None = None
    expected_query_fingerprint: str | None = None
    sort_field: str | None = None
    sort_direction: str = "asc"
    limit: int = Field(default=100, ge=1, le=MAX_QUERY_LIMIT, strict=True)
    offset: int = Field(default=0, ge=0, le=MAX_QUERY_OFFSET, strict=True)


class CreateSavedQueryRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    layer_id: UUID
    name: str
    description: str = ""
    attribute_filter: dict[str, Any] | None = None
    spatial_filter: dict[str, Any] | None = None
    sort_field: str | None = None
    sort_direction: str = "asc"


@router.post(f"{_BASE}/layers/{{layer_id}}/query")
def execute_layer_query(
    organization_id: UUID,
    project_id: UUID,
    layer_id: UUID,
    body: ExecuteQueryRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    repo = SqlQueryRepository(session)
    return repo.execute_query(
        context=ctx,
        layer_id=layer_id,
        attribute_filter=body.attribute_filter,
        spatial_filter=body.spatial_filter,
        expected_content_revision=body.expected_content_revision,
        expected_schema_revision=body.expected_schema_revision,
        expected_query_fingerprint=body.expected_query_fingerprint,
        sort_field=body.sort_field,
        sort_direction=body.sort_direction,
        limit=body.limit,
        offset=body.offset,
    )


@router.post(f"{_BASE}/saved-queries", status_code=201)
def create_saved_query(
    organization_id: UUID,
    project_id: UUID,
    body: CreateSavedQueryRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    repo = SqlQueryRepository(session)
    rec = repo.create_saved_query(
        context=ctx,
        layer_id=body.layer_id,
        name=body.name,
        description=body.description,
        attribute_filter=body.attribute_filter,
        spatial_filter=body.spatial_filter,
        sort_field=body.sort_field,
        sort_direction=body.sort_direction,
    )
    return _saved_wire(rec)


@router.get(f"{_BASE}/saved-queries")
def list_saved_queries(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    layer_id: Annotated[UUID | None, Query()] = None,
) -> dict[str, Any]:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    repo = SqlQueryRepository(session)
    items = repo.list_saved_queries(context=ctx, layer_id=layer_id)
    return {"items": [_saved_wire(q) for q in items]}


@router.delete(f"{_BASE}/saved-queries/{{query_id}}", status_code=204)
def delete_saved_query(
    organization_id: UUID,
    project_id: UUID,
    query_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> None:
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    SqlQueryRepository(session).delete_saved_query(context=ctx, query_id=query_id)
