"""Project-scoped human chat and task routes."""
from datetime import date, datetime, UTC
from typing import Annotated, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException, Query, Response
from pydantic import BaseModel, ConfigDict, Field, StringConstraints, model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import AuthenticatedRequest, authenticate_request, get_db_session, require_csrf
from openvietgis_api.adapters.persistence.scope_helper import prepare_project_scope, append_project_audit
from openvietgis_api.adapters.persistence.gis_access import scoped
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.context import AuthorizationContext

router = APIRouter(tags=["team-chat"])
BASE = "/organizations/{organization_id}/projects/{project_id}/collaboration"
ReadAuth = Annotated[AuthenticatedRequest, Depends(authenticate_request)]
WriteAuth = Annotated[AuthenticatedRequest, Depends(require_csrf)]
DB = Annotated[Session, Depends(get_db_session, scope="function")]
Status = Literal["todo", "in_progress", "done"]
Priority = Literal["low", "normal", "high"]
WHERE = "organization_id=:o AND project_id=:p"
MESSAGE_COLS = "id,author_id,author_name,body,created_at"
TASK_COLS = "id,creator_id,creator_name,assignee_id,assignee_name,title,description,status,priority,due_date,longitude,latitude,revision,created_at,updated_at"

class MessageRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    body: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=4000)]

class TaskRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    title: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=200)]
    description: str = Field(default="", max_length=2000)
    assignee_id: UUID
    priority: Priority = "normal"
    due_date: date | None = None
    longitude: float | None = Field(default=None, ge=-180, le=180, allow_inf_nan=False)
    latitude: float | None = Field(default=None, ge=-90, le=90, allow_inf_nan=False)

    @model_validator(mode="after")
    def validate_location(self):
        if (self.longitude is None) != (self.latitude is None):
            raise ValueError("longitude and latitude must both be supplied")
        return self

class StatusRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    status: Status
    revision: int = Field(ge=1)

def scope(session: Session, auth: AuthenticatedRequest, o: UUID, p: UUID, *, write: bool = False) -> AuthorizationContext:
    if auth.actor.actor_type != "human":
        raise HTTPException(403, "Human project member required")
    ctx = context_for(auth, organization_id=o, project_id=p)
    prepare_project_scope(session, ctx, permission=Permission.PROJECT_EDIT if write else Permission.PROJECT_READ)
    if scoped(session, ctx):
        raise HTTPException(403, "Full project access required")
    return ctx

def eligible(session: Session, o: UUID, p: UUID):
    # Only active, unrestricted project editors (or owners/admins).
    return session.execute(text("""
      SELECT DISTINCT u.id,u.display_name FROM org.organization_memberships m
      JOIN identity.users u ON u.id=m.user_id AND u.status='active'
      JOIN workspace.projects pr ON pr.organization_id=m.organization_id AND pr.id=:p
      WHERE m.organization_id=:o AND m.status='active' AND (
        m.role IN ('owner','admin') OR (
          NOT EXISTS(SELECT 1 FROM gis_access.scoped_members sm WHERE sm.organization_id=:o AND sm.project_id=:p AND sm.user_id=u.id)
          AND EXISTS(SELECT 1 FROM org.resource_grants g WHERE g.organization_id=:o AND g.membership_id=m.id
            AND g.subject_kind='membership' AND g.revoked_at IS NULL
            AND g.role IN ('manager','editor','contributor')
            AND (g.scope_kind='organization' OR (g.scope_kind='workspace' AND g.workspace_id=pr.workspace_id)
              OR (g.scope_kind='project' AND g.project_id=pr.id)))
        )
      ) ORDER BY u.display_name,u.id
    """), {"o": o, "p": p}).mappings().all()

def audit(session, ctx, rid, action, metadata):
    append_project_audit(session, ctx, audit_id=uuid4(), action=action, resource_type="collaboration_task", resource_id=rid, metadata=metadata, created_at=datetime.now(UTC))

@router.get(BASE + "/members")
def members(organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB, response: Response):
    scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    return {"items": [dict(r) for r in eligible(session, organization_id, project_id)]}

@router.get(BASE + "/messages")
def messages(organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB, response: Response,
             limit: Annotated[int, Query(ge=1, le=100)] = 100):
    scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    rows = session.execute(text(f"SELECT {MESSAGE_COLS} FROM collaboration.messages WHERE {WHERE} ORDER BY created_at DESC,id DESC LIMIT :limit"),
                           {"o": organization_id, "p": project_id, "limit": limit}).mappings().all()
    return {"items": [dict(r) for r in reversed(rows)]}

@router.post(BASE + "/messages", status_code=201)
def post_message(organization_id: UUID, project_id: UUID, body: MessageRequest, auth: WriteAuth, session: DB):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    row = session.execute(text(f"""INSERT INTO collaboration.messages(organization_id,project_id,id,author_id,author_name,body)
      VALUES(:o,:p,:id,:actor,:name,:body) RETURNING {MESSAGE_COLS}"""),
      {"o": organization_id, "p": project_id, "id": uuid4(), "actor": ctx.actor.actor_id, "name": ctx.actor.display_name[:200], "body": body.body}).mappings().one()
    return dict(row)

@router.get(BASE + "/tasks")
def tasks(organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB, response: Response,
          mine: bool = False, limit: Annotated[int, Query(ge=1, le=200)] = 200):
    ctx = scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    condition = " AND assignee_id=:actor" if mine else ""
    rows = session.execute(text(f"SELECT {TASK_COLS} FROM collaboration.tasks WHERE {WHERE}{condition} ORDER BY (status='done'),due_date NULLS LAST,updated_at DESC,id LIMIT :limit"),
                           {"o": organization_id, "p": project_id, "actor": ctx.actor.actor_id, "limit": limit}).mappings().all()
    return {"items": [dict(r) for r in rows]}

@router.post(BASE + "/tasks", status_code=201)
def create_task(organization_id: UUID, project_id: UUID, body: TaskRequest, auth: WriteAuth, session: DB):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    member = next((r for r in eligible(session, organization_id, project_id) if r["id"] == body.assignee_id), None)
    if member is None:
        raise HTTPException(422, "Assignee must be an active unrestricted project editor")
    rid = uuid4()
    row = session.execute(text(f"""INSERT INTO collaboration.tasks(organization_id,project_id,id,creator_id,creator_name,assignee_id,assignee_name,title,description,priority,due_date,longitude,latitude)
      VALUES(:o,:p,:id,:actor,:name,:assignee_id,:assignee_name,:title,:description,:priority,:due_date,:longitude,:latitude)
      RETURNING {TASK_COLS}"""),
      {"o": organization_id, "p": project_id, "id": rid, "actor": ctx.actor.actor_id, "name": ctx.actor.display_name[:200], "assignee_name": member["display_name"], **body.model_dump()}).mappings().one()
    audit(session, ctx, rid, "collaboration.task.create", {"assignee_id": str(body.assignee_id), "revision": 1})
    return dict(row)

@router.patch(BASE + "/tasks/{task_id}")
def change_task(organization_id: UUID, project_id: UUID, task_id: UUID, body: StatusRequest, auth: WriteAuth, session: DB):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    args = {"o": organization_id, "p": project_id, "id": task_id, "actor": ctx.actor.actor_id, "rev": body.revision, "status": body.status}
    previous = session.execute(text(f"SELECT creator_id,assignee_id,status,revision FROM collaboration.tasks WHERE {WHERE} AND id=:id"), args).mappings().one_or_none()
    if previous is None:
        raise HTTPException(404, "Task not found")
    if ctx.actor.actor_id not in (previous["creator_id"], previous["assignee_id"]):
        raise HTTPException(403, "Only creator or assignee may change status")
    row = session.execute(text(f"""UPDATE collaboration.tasks SET status=:status,revision=revision+1,updated_at=now()
      WHERE {WHERE} AND id=:id AND revision=:rev AND :actor IN (creator_id,assignee_id) RETURNING {TASK_COLS}"""), args).mappings().one_or_none()
    if row is None:
        raise HTTPException(409, "Task revision changed; refresh and retry")
    audit(session, ctx, task_id, "collaboration.task.status", {"before": previous["status"], "after": body.status, "revision": row["revision"]})
    return dict(row)
