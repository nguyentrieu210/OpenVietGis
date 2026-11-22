"""Persistent partner contributions and project-scoped immutable event journal."""

from __future__ import annotations

import json
from typing import Annotated, Any
from uuid import UUID

from fastapi import APIRouter, Depends, Query, Response
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import prepare_project_scope
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.extensions.contributions import validate_contributions
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformForbiddenError,
    PlatformNotFoundError,
)

router = APIRouter(tags=["partner-platform"])
_BASE = "/organizations/{organization_id}/projects/{project_id}"


def _scope(session, auth, organization_id, project_id, permission):
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=permission)
    params = {"org": organization_id, "proj": project_id}
    if session.execute(text("SELECT gis_access.scoped(:org,:proj)"), params).scalar_one():
        raise PlatformForbiddenError("partner integrations require unrestricted project access")
    return params


_ACTIVE = """
    FROM extensions.project_activations a JOIN extensions.manifests m
      ON a.manifest_id=m.id AND a.organization_id=m.organization_id
    WHERE a.organization_id=:org AND a.project_id=:proj AND a.status='active'
      AND m.enabled AND left(m.extension_key,8)='partner_'
"""


class SaveContributions(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: int = Field(ge=0, le=2147483646, strict=True)
    document: dict[str, Any]


@router.get(f"{_BASE}/extensions/partner-contributions")
def list_contributions(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    params = _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = (
        session.execute(
            text("""
        SELECT c.manifest_id,c.document,c.revision,c.updated_at,m.display_name
        FROM extensions.partner_contributions c
        JOIN extensions.project_activations a ON a.organization_id=c.organization_id
          AND a.project_id=c.project_id AND a.manifest_id=c.manifest_id AND a.status='active'
        JOIN extensions.manifests m ON m.id=c.manifest_id AND m.organization_id=c.organization_id
        WHERE c.organization_id=:org AND c.project_id=:proj AND m.enabled
        ORDER BY c.manifest_id
    """),
            params,
        )
        .mappings()
        .all()
    )
    response.headers["Cache-Control"] = "no-store"
    return {"items": [dict(row) for row in rows]}


@router.put(f"{_BASE}/extensions/partner-contributions/{{manifest_id}}")
def save_contributions(
    organization_id: UUID,
    project_id: UUID,
    manifest_id: UUID,
    body: SaveContributions,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    if auth.actor.actor_type != "human":
        raise PlatformForbiddenError("human project manager required")
    params = _scope(session, auth, organization_id, project_id, Permission.PROJECT_MANAGE)
    params["mid"] = manifest_id
    m = (
        session.execute(
            text("SELECT m.capabilities,m.display_name " + _ACTIVE + " AND m.id=:mid"), params
        )
        .mappings()
        .first()
    )
    if m is None:
        raise PlatformNotFoundError("active partner manifest not found")
    document = validate_contributions(body.document, m["capabilities"])
    existing = session.execute(
        text("""
        SELECT revision FROM extensions.partner_contributions
        WHERE organization_id=:org AND project_id=:proj AND manifest_id=:mid FOR UPDATE
    """),
        params,
    ).scalar_one_or_none()
    if (existing or 0) != body.expected_revision:
        raise PlatformConflictError("contribution revision changed")
    params.update(doc=json.dumps(document), rev=body.expected_revision + 1)
    row = (
        session.execute(
            text("""
        INSERT INTO extensions.partner_contributions
            (organization_id,project_id,manifest_id,document,revision)
        VALUES(:org,:proj,:mid,CAST(:doc AS jsonb),:rev)
        ON CONFLICT(organization_id,project_id,manifest_id)
        DO UPDATE SET document=EXCLUDED.document,revision=EXCLUDED.revision,updated_at=now()
        RETURNING manifest_id,document,revision,updated_at
    """),
            params,
        )
        .mappings()
        .one()
    )
    session.commit()
    return {**dict(row), "display_name": m["display_name"]}


@router.get(f"{_BASE}/integrations/events")
def list_events(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    after: Annotated[int, Query(ge=0, le=9223372036854775807)] = 0,
    limit: Annotated[int, Query(ge=1, le=200)] = 100,
) -> dict[str, Any]:
    params = _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    params.update(after=after, limit=limit)
    rows = (
        session.execute(
            text("""
        SELECT sequence,id,event_kind,resource_type,resource_id,revision_ref,created_at
        FROM extensions.integration_events WHERE organization_id=:org AND project_id=:proj
          AND sequence>:after ORDER BY sequence LIMIT :limit
    """),
            params,
        )
        .mappings()
        .all()
    )
    head = session.execute(
        text("""
        SELECT COALESCE(MAX(sequence),0) FROM extensions.integration_events
        WHERE organization_id=:org AND project_id=:proj
    """),
        params,
    ).scalar_one()
    response.headers["Cache-Control"] = "no-store"
    return {
        "items": [dict(r) for r in rows],
        "head_sequence": head,
        "next_after_sequence": rows[-1]["sequence"] if rows else after,
    }
