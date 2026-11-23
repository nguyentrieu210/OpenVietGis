"""Project-scoped human chat and task routes."""

import json
from datetime import UTC, date, datetime
from typing import Annotated, Literal
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException, Query, Response
from pydantic import BaseModel, ConfigDict, Field, StringConstraints, model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.gis_access import scoped
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.editing.domain import validate_feature_id

router = APIRouter(tags=["team-chat"])
BASE = "/organizations/{organization_id}/projects/{project_id}/collaboration"
ReadAuth = Annotated[AuthenticatedRequest, Depends(authenticate_request)]
WriteAuth = Annotated[AuthenticatedRequest, Depends(require_csrf)]
DB = Annotated[Session, Depends(get_db_session, scope="function")]
Status = Literal["todo", "in_progress", "done"]
Priority = Literal["low", "normal", "high"]
WHERE = "organization_id=:o AND project_id=:p"
MESSAGE_COLS = (
    "id,author_id,author_name,body,created_at,recipient_id,source_id,feature_id,longitude,"
    "latitude,mentions"
)
TASK_COLS = (
    "id,creator_id,creator_name,assignee_id,assignee_name,title,description,status,priority,"
    "due_date,longitude,latitude,revision,created_at,updated_at,source_message_id,source_id,"
    "feature_id"
)


class MessageRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    body: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=4000)]

    recipient_id: UUID | None = None


class CommentRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=128)
    body: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=4000)]
    mention_ids: list[UUID] = Field(default_factory=list, max_length=20)


class TaskRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    title: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=200)]
    description: str = Field(default="", max_length=2000)
    assignee_id: UUID
    source_message_id: UUID | None = None
    source_id: UUID | None = None
    feature_id: str | None = Field(default=None, min_length=1, max_length=128)
    priority: Priority = "normal"
    due_date: date | None = None
    longitude: float | None = Field(default=None, ge=-180, le=180, allow_inf_nan=False)
    latitude: float | None = Field(default=None, ge=-90, le=90, allow_inf_nan=False)

    @model_validator(mode="after")
    def validate_location(self):
        if (self.source_id is None) != (self.feature_id is None):
            raise ValueError("source_id and feature_id must both be supplied")
        if (self.longitude is None) != (self.latitude is None):
            raise ValueError("longitude and latitude must both be supplied")
        return self


class StatusRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    status: Status
    revision: int = Field(ge=1)


def scope(
    session: Session, auth: AuthenticatedRequest, o: UUID, p: UUID, *, write: bool = False
) -> AuthorizationContext:
    if auth.actor.actor_type != "human":
        raise HTTPException(403, "Human project member required")
    ctx = context_for(auth, organization_id=o, project_id=p)
    prepare_project_scope(
        session, ctx, permission=Permission.PROJECT_EDIT if write else Permission.PROJECT_READ
    )
    if scoped(session, ctx):
        raise HTTPException(403, "Full project access required")
    return ctx


def eligible(session: Session, o: UUID, p: UUID):
    # Only active, unrestricted project editors (or owners/admins).
    return (
        session.execute(
            text("""
      SELECT DISTINCT u.id,u.display_name FROM org.organization_memberships m
      JOIN identity.users u ON u.id=m.user_id AND u.status='active'
      JOIN workspace.projects pr ON pr.organization_id=m.organization_id AND pr.id=:p
      WHERE m.organization_id=:o AND m.status='active' AND (
        m.role IN ('owner','admin') OR (
          NOT EXISTS(SELECT 1 FROM gis_access.scoped_members sm WHERE sm.organization_id=:o AND
            sm.project_id=:p AND sm.user_id=u.id)
          AND EXISTS(SELECT 1 FROM org.resource_grants g WHERE g.organization_id=:o AND
            g.membership_id=m.id
            AND g.subject_kind='membership' AND g.revoked_at IS NULL
            AND g.role IN ('manager','editor','contributor')
            AND (g.scope_kind='organization' OR (g.scope_kind='workspace' AND
              g.workspace_id=pr.workspace_id)
              OR (g.scope_kind='project' AND g.project_id=pr.id)))
        )
      ) ORDER BY u.display_name,u.id
    """),
            {"o": o, "p": p},
        )
        .mappings()
        .all()
    )


def audit(session, ctx, rid, action, metadata):
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action=action,
        resource_type="collaboration_task",
        resource_id=rid,
        metadata=metadata,
        created_at=datetime.now(UTC),
    )


@router.get(BASE + "/members")
def members(
    organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB, response: Response
):
    scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    return {"items": [dict(r) for r in eligible(session, organization_id, project_id)]}


@router.get(BASE + "/messages")
def messages(
    organization_id: UUID,
    project_id: UUID,
    auth: ReadAuth,
    session: DB,
    response: Response,
    limit: Annotated[int, Query(ge=1, le=100)] = 100,
    peer: UUID | None = None,
):
    ctx = scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    condition = (
        " AND recipient_id IS NULL AND source_id IS NULL"
        if peer is None
        else (
            " AND ((author_id=:actor AND recipient_id=:peer) OR (author_id=:peer AND "
            "recipient_id=:actor))"
        )
    )
    rows = (
        session.execute(
            text(
                f"SELECT {MESSAGE_COLS} FROM collaboration.messages "
                f"WHERE {WHERE}{condition} ORDER BY created_at DESC,id DESC LIMIT :limit"
            ),
            {
                "o": organization_id,
                "p": project_id,
                "limit": limit,
                "actor": ctx.actor.actor_id,
                "peer": peer,
            },
        )
        .mappings()
        .all()
    )
    return {"items": [dict(r) for r in reversed(rows)]}


@router.post(BASE + "/messages", status_code=201)
def post_message(
    organization_id: UUID, project_id: UUID, body: MessageRequest, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    if body.recipient_id is not None and (
        body.recipient_id == ctx.actor.actor_id
        or not any(
            r["id"] == body.recipient_id for r in eligible(session, organization_id, project_id)
        )
    ):
        raise HTTPException(422, "Recipient must be another unrestricted project editor")
    row = (
        session.execute(
            text(f"""INSERT INTO
              collaboration.messages(organization_id,project_id,id,author_id,author_name,body,
              recipient_id)
      VALUES(:o,:p,:id,:actor,:name,:body,:recipient_id) RETURNING {MESSAGE_COLS}"""),
            {
                "o": organization_id,
                "p": project_id,
                "id": uuid4(),
                "actor": ctx.actor.actor_id,
                "name": ctx.actor.display_name[:200],
                "body": body.body,
                "recipient_id": body.recipient_id,
            },
        )
        .mappings()
        .one()
    )
    return dict(row)


def feature_anchor(session, o, p, source, feature):
    fid = validate_feature_id(feature)
    row = (
        session.execute(
            text("""
      SELECT ST_X(anchor) AS longitude, ST_Y(anchor) AS latitude FROM (
        SELECT CASE WHEN f.geometry IS NULL OR ST_IsEmpty(f.geometry) OR ST_SRID(f.geometry)<=0
        THEN NULL
          ELSE ST_Transform(ST_PointOnSurface(f.geometry),4326) END AS anchor
        FROM spatial.reference_features f JOIN spatial.data_sources s ON
        s.organization_id=f.organization_id AND s.project_id=f.project_id AND s.id=f.source_id
        WHERE f.organization_id=:o AND f.project_id=:p AND f.source_id=:s AND f.feature_id=:f AND
        s.lifecycle='active'
      ) AS target
    """),
            {"o": o, "p": p, "s": source, "f": fid},
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(404, "Feature not found")
    lng, lat = row["longitude"], row["latitude"]
    if lng is None or lat is None or not (-180 <= lng <= 180 and -90 <= lat <= 90):
        lng = lat = None
    return {"source_id": source, "feature_id": fid, "longitude": lng, "latitude": lat}


@router.get(BASE + "/comments")
def comments(
    organization_id: UUID,
    project_id: UUID,
    source_id: UUID,
    feature_id: Annotated[str, Query(min_length=1, max_length=128)],
    auth: ReadAuth,
    session: DB,
    response: Response,
    limit: Annotated[int, Query(ge=1, le=100)] = 100,
):
    scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    rows = (
        session.execute(
            text(
                f"SELECT {MESSAGE_COLS} FROM collaboration.messages WHERE {WHERE} AND "
                f"source_id=:s AND feature_id=:f ORDER BY created_at DESC,id DESC LIMIT :limit"
            ),
            {
                "o": organization_id,
                "p": project_id,
                "s": source_id,
                "f": validate_feature_id(feature_id),
                "limit": limit,
            },
        )
        .mappings()
        .all()
    )
    return {"items": [dict(r) for r in reversed(rows)]}


@router.get(BASE + "/comment-pins")
def comment_pins(
    organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB, response: Response
):
    scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    rows = (
        session.execute(
            text(f"""SELECT source_id,feature_id,longitude,latitude,comment_count FROM (
        SELECT DISTINCT ON (source_id,feature_id) source_id,feature_id,longitude,latitude,
        created_at,
          count(*) OVER(PARTITION BY source_id,feature_id) AS comment_count
        FROM collaboration.messages WHERE {WHERE} AND source_id IS NOT NULL AND longitude IS NOT
        NULL AND latitude IS NOT NULL
        ORDER BY source_id,feature_id,created_at DESC,id DESC
      ) pins ORDER BY created_at DESC,source_id,feature_id LIMIT 200"""),
            {"o": organization_id, "p": project_id},
        )
        .mappings()
        .all()
    )
    return {"items": [dict(r) for r in rows]}


@router.post(BASE + "/comments", status_code=201)
def post_comment(
    organization_id: UUID, project_id: UUID, body: CommentRequest, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    anchor = feature_anchor(session, organization_id, project_id, body.source_id, body.feature_id)
    people = {r["id"]: r["display_name"] for r in eligible(session, organization_id, project_id)}
    ids = list(dict.fromkeys(body.mention_ids))
    if any(i == ctx.actor.actor_id or i not in people for i in ids):
        raise HTTPException(422, "Mentions must target other unrestricted project editors")
    mentions = [{"id": str(i), "display_name": people[i]} for i in ids]
    args = {
        "o": organization_id,
        "p": project_id,
        "id": uuid4(),
        "actor": ctx.actor.actor_id,
        "name": ctx.actor.display_name[:200],
        "body": body.body,
        "mentions": json.dumps(mentions),
        **anchor,
    }
    row = (
        session.execute(
            text(f"""INSERT INTO collaboration.messages(organization_id,project_id,id,author_id,
            author_name,body,source_id,feature_id,longitude,latitude,mentions)
      VALUES(:o,:p,:id,:actor,:name,:body,:source_id,:feature_id,:longitude,:latitude,
      CAST(:mentions AS jsonb)) RETURNING {MESSAGE_COLS}"""),
            args,
        )
        .mappings()
        .one()
    )
    for i in ids:
        session.execute(
            text(
                "INSERT INTO collaboration.comment_mentions(organization_id,project_id,"
                "message_id,user_id) VALUES(:o,:p,:id,:user)"
            ),
            {**args, "user": i},
        )
    return dict(row)


@router.get(BASE + "/tasks")
def tasks(
    organization_id: UUID,
    project_id: UUID,
    auth: ReadAuth,
    session: DB,
    response: Response,
    mine: bool = False,
    limit: Annotated[int, Query(ge=1, le=200)] = 200,
):
    ctx = scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    condition = " AND assignee_id=:actor" if mine else ""
    rows = (
        session.execute(
            text(
                f"SELECT {TASK_COLS} FROM collaboration.tasks WHERE {WHERE}{condition} ORDER BY "
                f"(status='done'),due_date NULLS LAST,updated_at DESC,id LIMIT :limit"
            ),
            {"o": organization_id, "p": project_id, "actor": ctx.actor.actor_id, "limit": limit},
        )
        .mappings()
        .all()
    )
    return {"items": [dict(r) for r in rows]}


@router.post(BASE + "/tasks", status_code=201)
def create_task(
    organization_id: UUID, project_id: UUID, body: TaskRequest, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    member = next(
        (r for r in eligible(session, organization_id, project_id) if r["id"] == body.assignee_id),
        None,
    )
    if member is None:
        raise HTTPException(422, "Assignee must be an active unrestricted project editor")
    anchor = {
        "source_id": body.source_id,
        "feature_id": body.feature_id,
        "longitude": body.longitude,
        "latitude": body.latitude,
    }
    public_message = None
    if body.source_message_id is not None:
        public_message = (
            session.execute(
                text(
                    f"SELECT source_id,feature_id,longitude,latitude FROM collaboration.messages "
                    f"WHERE {WHERE} AND id=:source AND recipient_id IS NULL"
                ),
                {"o": organization_id, "p": project_id, "source": body.source_message_id},
            )
            .mappings()
            .one_or_none()
        )
        if public_message is None:
            raise HTTPException(
                422, "Only a project message or comment can be linked to a project task"
            )
        if public_message["source_id"] is not None:
            if body.source_id is not None and (
                body.source_id != public_message["source_id"]
                or body.feature_id != public_message["feature_id"]
            ):
                raise HTTPException(422, "Task feature must match comment")
            anchor = dict(public_message)
    if body.source_id is not None and not (
        public_message is not None and public_message["source_id"] is not None
    ):
        anchor = feature_anchor(
            session, organization_id, project_id, body.source_id, body.feature_id
        )
    rid = uuid4()
    row = (
        session.execute(
            text(f"""INSERT INTO collaboration.tasks(organization_id,project_id,id,creator_id,
            creator_name,assignee_id,assignee_name,title,description,priority,due_date,longitude,
            latitude,source_message_id,source_id,feature_id)
      VALUES(:o,:p,:id,:actor,:name,:assignee_id,:assignee_name,:title,:description,:priority,:due_date,:longitude,:latitude,:source_message_id,:source_id,:feature_id)
      RETURNING {TASK_COLS}"""),
            {
                "o": organization_id,
                "p": project_id,
                "id": rid,
                "actor": ctx.actor.actor_id,
                "name": ctx.actor.display_name[:200],
                "assignee_name": member["display_name"],
                **body.model_dump(),
                **anchor,
            },
        )
        .mappings()
        .one()
    )
    audit(
        session,
        ctx,
        rid,
        "collaboration.task.create",
        {"assignee_id": str(body.assignee_id), "revision": 1},
    )
    return dict(row)


@router.patch(BASE + "/tasks/{task_id}")
def change_task(
    organization_id: UUID,
    project_id: UUID,
    task_id: UUID,
    body: StatusRequest,
    auth: WriteAuth,
    session: DB,
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    args = {
        "o": organization_id,
        "p": project_id,
        "id": task_id,
        "actor": ctx.actor.actor_id,
        "rev": body.revision,
        "status": body.status,
    }
    previous = (
        session.execute(
            text(
                "SELECT creator_id,assignee_id,status,revision FROM collaboration.tasks "
                f"WHERE {WHERE} AND id=:id"
            ),
            args,
        )
        .mappings()
        .one_or_none()
    )
    if previous is None:
        raise HTTPException(404, "Task not found")
    if ctx.actor.actor_id not in (previous["creator_id"], previous["assignee_id"]):
        raise HTTPException(403, "Only creator or assignee may change status")
    row = (
        session.execute(
            text(f"""UPDATE collaboration.tasks SET
              status=:status,revision=revision+1,updated_at=now()
      WHERE {WHERE} AND id=:id AND revision=:rev AND :actor IN (creator_id,assignee_id)
        RETURNING {TASK_COLS}"""),
            args,
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise HTTPException(409, "Task revision changed; refresh and retry")
    audit(
        session,
        ctx,
        task_id,
        "collaboration.task.status",
        {"before": previous["status"], "after": body.status, "revision": row["revision"]},
    )
    return dict(row)


@router.get(BASE + "/notifications")
def notifications(
    organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB, response: Response
):
    ctx = scope(session, auth, organization_id, project_id)
    response.headers["Cache-Control"] = "no-store"
    args = {"o": organization_id, "p": project_id, "actor": ctx.actor.actor_id}
    query = """FROM collaboration.tasks t LEFT JOIN collaboration.task_notification_reads r
      ON r.organization_id=t.organization_id AND r.project_id=t.project_id AND r.task_id=t.id AND
      r.user_id=:actor
      WHERE t.organization_id=:o AND t.project_id=:p AND t.assignee_id=:actor AND
      t.creator_id<>:actor"""
    mention_query = """FROM collaboration.comment_mentions r JOIN collaboration.messages m
      ON m.organization_id=r.organization_id AND m.project_id=r.project_id AND m.id=r.message_id
      WHERE r.organization_id=:o AND r.project_id=:p AND r.user_id=:actor"""
    combined = (
        "SELECT t.id,t.title,t.creator_name,t.created_at,r.read_at,'task' AS kind,t.source_id,"
        "t.feature_id,t.longitude,t.latitude "
        + query
        + " UNION ALL SELECT m.id,left(m.body,200) AS title,m.author_name AS creator_name,"
        "m.created_at,r.read_at,'mention' AS kind,m.source_id,m.feature_id,m.longitude,"
        "m.latitude " + mention_query
    )
    rows = (
        session.execute(
            text(
                "SELECT * FROM ("
                + combined
                + ") AS inbox ORDER BY (read_at IS NULL) DESC,created_at DESC,id LIMIT 100"
            ),
            args,
        )
        .mappings()
        .all()
    )
    count = session.execute(
        text("SELECT count(*) FROM (" + combined + ") AS inbox WHERE read_at IS NULL"), args
    ).scalar_one()
    return {"items": [dict(r) for r in rows], "unread": count}


@router.post(BASE + "/notifications/{task_id}/read")
def read_notification(
    organization_id: UUID, project_id: UUID, task_id: UUID, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    args = {"o": organization_id, "p": project_id, "id": task_id, "actor": ctx.actor.actor_id}
    exists = session.execute(
        text(
            f"SELECT id FROM collaboration.tasks WHERE {WHERE} AND id=:id AND assignee_id=:actor "
            f"AND creator_id<>:actor"
        ),
        args,
    ).scalar_one_or_none()
    if exists is None:
        raise HTTPException(404, "Notification not found")
    session.execute(
        text("""INSERT INTO
          collaboration.task_notification_reads(organization_id,project_id,task_id,user_id)
      VALUES(:o,:p,:id,:actor) ON CONFLICT DO NOTHING"""),
        args,
    )
    return {"read": True}


@router.post(BASE + "/mentions/{message_id}/read")
def read_mention(
    organization_id: UUID, project_id: UUID, message_id: UUID, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    row = session.execute(
        text(
            "UPDATE collaboration.comment_mentions SET read_at=COALESCE(read_at,now()) WHERE "
            "organization_id=:o AND project_id=:p AND message_id=:id AND user_id=:actor "
            "RETURNING message_id"
        ),
        {"o": organization_id, "p": project_id, "id": message_id, "actor": ctx.actor.actor_id},
    ).first()
    if row is None:
        raise HTTPException(404, "Mention not found")
    return {"read": True}
