"""Spec 060: organisation chat rooms, reactions, pins, contacts and SSE."""

from __future__ import annotations

import asyncio
import base64
import json
import re
import time
import unicodedata
from collections.abc import AsyncIterator
from datetime import UTC, datetime
from typing import Annotated, Literal
from uuid import UUID, uuid4

import anyio
import psycopg
from fastapi import APIRouter, HTTPException, Query, Request, Response
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, ConfigDict, Field, StringConstraints, model_validator
from sqlalchemy import text
from sqlalchemy.engine import make_url
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    _authenticate_human_session,
)
from openvietgis_api.adapters.http.team_chat import DB, ReadAuth, WriteAuth
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.adapters.persistence.rls import (
    bind_authenticated_actor,
    require_member_context,
)
from openvietgis_api.adapters.persistence.scope_helper import append_project_audit
from openvietgis_api.config import get_settings
from openvietgis_api.domains.authorization.context import AuthorizationContext

router = APIRouter(tags=["org-chat"])
BASE = "/organizations/{organization_id}/chat"
CHANNEL = "ovg_chat"
MAX_ROOM_MEMBERS = 50
MAX_PINS = 20
MAX_ATTACHMENT_BYTES = 400 * 1024
IMAGE_URL = re.compile(r"data:image/(jpeg|png|webp);base64,([A-Za-z0-9+/=]+)")
# Limit dedicated PostgreSQL listener connections in each process.
MAX_LISTENERS = 40
_slots = asyncio.Semaphore(MAX_LISTENERS)
T = "collaboration."
Name = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=80)]
MEMBERS_SQL = """SELECT u.id,u.display_name FROM org.organization_memberships m
  JOIN identity.users u ON u.id=m.user_id AND u.status='active'
  WHERE m.organization_id=:o AND m.status='active'"""
MESSAGE_SQL = """SELECT m.id,m.author_id,m.author_name,m.body,m.created_at,m.mentions,
  CASE WHEN q.id IS NULL THEN NULL ELSE json_build_object('id',q.id,'author_name',q.author_name,
    'body',left(q.body,140)) END AS reply_to,
  EXISTS(SELECT 1 FROM collaboration.chat_pins p WHERE p.organization_id=m.organization_id
    AND p.message_id=m.id) AS pinned,
  COALESCE((SELECT json_agg(json_build_object('id',a.id,'kind','image','width',a.width,
    'height',a.height) ORDER BY a.position) FROM collaboration.chat_attachments a
    WHERE a.organization_id=m.organization_id AND a.message_id=m.id),'[]') AS attachments
  FROM collaboration.chat_messages m LEFT JOIN collaboration.chat_messages q
    ON q.organization_id=m.organization_id AND q.id=m.reply_to_id
  WHERE m.organization_id=:o"""


class RoomRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: Name
    member_ids: list[UUID] = Field(min_length=1, max_length=49)


class RenameRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: Name


class UsersRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    user_ids: list[UUID] = Field(min_length=1, max_length=49)


class UserRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    user_id: UUID


class AttachmentRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    kind: Literal["image"]
    # 400 KB decoded is about 546 KB of base64 plus the prefix.
    data_url: str = Field(min_length=1, max_length=560_000)
    width: int = Field(ge=1, le=4096)
    height: int = Field(ge=1, le=4096)


class ChatMessageRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    body: Annotated[str, StringConstraints(strip_whitespace=True, max_length=4000)] = ""
    reply_to_id: UUID | None = None
    mention_ids: list[UUID] = Field(default_factory=list, max_length=20)
    attachments: list[AttachmentRequest] = Field(default_factory=list, max_length=4)

    @model_validator(mode="after")
    def not_empty(self):
        if not self.body and not self.attachments:
            raise ValueError("body or an attachment is required")
        return self


def decode_image(data_url: str) -> tuple[str, bytes]:
    match = IMAGE_URL.fullmatch(data_url)
    if match is None:
        raise HTTPException(422, "Attachments must be JPEG, PNG or WebP data URLs")
    try:
        raw = base64.b64decode(match[2], validate=True)
    except ValueError:
        raise HTTPException(422, "Attachment is not valid base64") from None
    kind = match[1]
    magic = {
        "jpeg": raw[:3] == b"\xff\xd8\xff",
        "png": raw[:8] == b"\x89PNG\r\n\x1a\n",
        "webp": raw[:4] == b"RIFF" and raw[8:12] == b"WEBP",
    }[kind]
    if not raw or len(raw) > MAX_ATTACHMENT_BYTES or not magic:
        raise HTTPException(422, "Attachment content does not match its image type or is too big")
    return "image/" + kind, raw


class ReactionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    emoji: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=16)]


def org_scope(session: Session, auth: AuthenticatedRequest, o: UUID) -> AuthorizationContext:
    if auth.actor.actor_type != "human" or auth.actor.actor_id is None:
        raise HTTPException(403, "Human organisation member required")
    ctx = context_for(auth, organization_id=o)
    bind_authenticated_actor(session, auth.actor)
    require_member_context(session, ctx, o)  # 404 unless an active member
    return ctx


def human_actor_id(ctx: AuthorizationContext) -> UUID:
    actor_id = ctx.actor.actor_id
    if actor_id is None:
        raise HTTPException(403, "Human organisation member required")
    return actor_id


def people(session: Session, o: UUID) -> dict[UUID, str]:
    return {r[0]: r[1] for r in session.execute(text(MEMBERS_SQL), {"o": o}).all()}


def audit(session, ctx, rid, action, metadata, resource_type="chat_room"):
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action=action,
        resource_type=resource_type,
        resource_id=rid,
        metadata=metadata,
        created_at=datetime.now(UTC),
    )


def org_admin(session: Session, o: UUID, user: UUID) -> bool:
    return (
        session.execute(
            text(
                "SELECT 1 FROM org.organization_memberships WHERE organization_id=:o "
                "AND user_id=:u AND status='active' AND role IN ('owner','admin')"
            ),
            {"o": o, "u": user},
        ).first()
        is not None
    )


def org_room(session: Session, o: UUID) -> UUID:
    # Lazy and idempotent; the partial unique index serialises concurrent creators.
    session.execute(
        text(
            f"INSERT INTO {T}chat_rooms(organization_id,id,kind) VALUES(:o,:id,'org') "
            "ON CONFLICT (organization_id) WHERE kind='org' DO NOTHING"
        ),
        {"o": o, "id": uuid4()},
    )
    return session.execute(
        text(f"SELECT id FROM {T}chat_rooms WHERE organization_id=:o AND kind='org'"), {"o": o}
    ).scalar_one()


def member_role(session: Session, o: UUID, room: UUID, user: UUID) -> str | None:
    return session.execute(
        text(
            f"SELECT role FROM {T}chat_room_members WHERE organization_id=:o AND room_id=:r "
            "AND user_id=:u"
        ),
        {"o": o, "r": room, "u": user},
    ).scalar_one_or_none()


def room_access(session: Session, o: UUID, room: UUID, me: UUID) -> tuple[str, str | None]:
    kind = session.execute(
        text(f"SELECT kind FROM {T}chat_rooms WHERE organization_id=:o AND id=:r"),
        {"o": o, "r": room},
    ).scalar_one_or_none()
    role = None if kind is None else member_role(session, o, room, me)
    if kind is None or (kind != "org" and role is None):
        raise HTTPException(404, "Room not found")
    return kind, role


def list_rooms(session: Session, o: UUID, me: UUID, room: UUID | None = None) -> list[dict]:
    rows = (
        session.execute(
            text(f"""
      SELECT r.id,r.kind,r.name,
        CASE WHEN r.kind='org' THEN (SELECT count(*) FROM ({MEMBERS_SQL}) a)
          ELSE (SELECT count(*) FROM {T}chat_room_members c
            WHERE c.organization_id=r.organization_id
            AND c.room_id=r.id) END AS member_count,
        (SELECT count(*) FROM {T}chat_messages x WHERE x.organization_id=r.organization_id
          AND x.room_id=r.id AND x.author_id<>:me AND x.created_at>COALESCE(rm.last_read_at,
          rm.joined_at,'-infinity')) AS unread,
        EXISTS(SELECT 1 FROM {T}chat_messages x WHERE x.organization_id=r.organization_id
          AND x.room_id=r.id AND x.author_id<>:me AND x.created_at>COALESCE(rm.last_read_at,
          rm.joined_at,'-infinity') AND x.mentions @> jsonb_build_array(jsonb_build_object(
          'id',CAST(:me AS text)))) AS mentioned,
        lm.id AS lm_id,lm.body AS lm_body,lm.author_name AS lm_author,lm.created_at AS lm_at,
        p.id AS peer_id,p.display_name AS peer_name,
        EXISTS(SELECT 1 FROM identity.user_avatars av WHERE av.user_id=p.id) AS peer_avatar
      FROM {T}chat_rooms r
      LEFT JOIN {T}chat_room_members rm ON rm.organization_id=r.organization_id
        AND rm.room_id=r.id AND rm.user_id=:me
      LEFT JOIN LATERAL (SELECT id,body,author_name,created_at FROM {T}chat_messages x
        WHERE x.organization_id=r.organization_id AND x.room_id=r.id
        ORDER BY x.created_at DESC,x.id DESC LIMIT 1) lm ON true
      LEFT JOIN identity.users p ON r.kind='direct'
        AND p.id=CASE WHEN r.direct_low=:me THEN r.direct_high ELSE r.direct_low END
      WHERE r.organization_id=:o AND (r.kind='org' OR rm.user_id IS NOT NULL)
        AND (CAST(:room AS uuid) IS NULL OR r.id=:room)
      ORDER BY COALESCE(lm.created_at,r.created_at) DESC,r.id"""),
            {"o": o, "me": me, "room": room},
        )
        .mappings()
        .all()
    )
    return [
        {
            "id": r["id"],
            "kind": r["kind"],
            "name": r["peer_name"] if r["kind"] == "direct" else r["name"],
            "member_count": r["member_count"],
            "unread": r["unread"],
            "mentioned": r["mentioned"],
            "last_message": None
            if r["lm_id"] is None
            else {
                "id": r["lm_id"],
                "body": r["lm_body"],
                "author_name": r["lm_author"],
                "created_at": r["lm_at"],
            },
            "peer": None
            if r["peer_id"] is None
            else {
                "id": r["peer_id"],
                "display_name": r["peer_name"],
                "has_avatar": r["peer_avatar"],
            },
        }
        for r in rows
    ]


def one_room(session, o, me, room):
    return list_rooms(session, o, me, room)[0]


def reactions(session: Session, o: UUID, ids: list[UUID], me: UUID) -> dict[UUID, list]:
    out: dict[UUID, list] = {i: [] for i in ids}
    if not ids:
        return out
    for r in session.execute(
        text(f"""SELECT message_id,emoji,count(*) AS count,bool_or(user_id=:me) AS mine
          FROM {T}chat_reactions WHERE organization_id=:o AND message_id=ANY(:ids)
          GROUP BY message_id,emoji ORDER BY min(created_at),emoji"""),
        {"o": o, "ids": ids, "me": me},
    ).mappings():
        out[r["message_id"]].append({"emoji": r["emoji"], "count": r["count"], "mine": r["mine"]})
    return out


def shape(session, o, me, rows) -> list[dict]:
    found = reactions(session, o, [r["id"] for r in rows], me)
    return [dict(r) | {"reactions": found[r["id"]]} for r in rows]


def message_room(session: Session, o: UUID, message: UUID, me: UUID):
    row = session.execute(
        text(f"SELECT room_id,author_id FROM {T}chat_messages WHERE organization_id=:o AND id=:m"),
        {"o": o, "m": message},
    ).one_or_none()
    if row is None:
        raise HTTPException(404, "Message not found")
    kind, role = room_access(session, o, row[0], me)
    return row[0], row[1], kind, role


@router.get(BASE + "/rooms")
def rooms(organization_id: UUID, auth: ReadAuth, session: DB, response: Response):
    ctx = org_scope(session, auth, organization_id)
    response.headers["Cache-Control"] = "no-store"
    org_room(session, organization_id)
    return {"items": list_rooms(session, organization_id, human_actor_id(ctx))}


@router.post(BASE + "/rooms", status_code=201)
def create_room(organization_id: UUID, body: RoomRequest, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    ids = [i for i in dict.fromkeys(body.member_ids) if i != me]
    known = people(session, organization_id)
    if not ids or any(i not in known for i in ids):
        raise HTTPException(422, "Members must be other active organisation members")
    rid = uuid4()
    session.execute(
        text(
            f"INSERT INTO {T}chat_rooms(organization_id,id,kind,name,created_by) "
            "VALUES(:o,:id,'group',:name,:me)"
        ),
        {"o": organization_id, "id": rid, "name": body.name, "me": me},
    )
    for user, role in [(me, "admin"), *((i, "member") for i in ids)]:
        session.execute(
            text(
                f"INSERT INTO {T}chat_room_members(organization_id,room_id,user_id,role) "
                "VALUES(:o,:r,:u,:role)"
            ),
            {"o": organization_id, "r": rid, "u": user, "role": role},
        )
    audit(session, ctx, rid, "chat.room.create", {"members": len(ids) + 1})
    return one_room(session, organization_id, me, rid)


@router.post(BASE + "/rooms/direct")
def direct_room(organization_id: UUID, body: UserRequest, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    if body.user_id == me or body.user_id not in people(session, organization_id):
        raise HTTPException(404, "Person not found")
    low, high = min(me, body.user_id), max(me, body.user_id)
    args = {"o": organization_id, "id": uuid4(), "low": low, "high": high, "me": me}
    session.execute(
        text(f"""INSERT INTO {T}chat_rooms(organization_id,id,kind,direct_low,direct_high,
          created_by) VALUES(:o,:id,'direct',:low,:high,:me)
          ON CONFLICT (organization_id,direct_low,direct_high) WHERE kind='direct' DO NOTHING"""),
        args,
    )
    rid = session.execute(
        text(
            f"SELECT id FROM {T}chat_rooms WHERE organization_id=:o AND kind='direct' "
            "AND direct_low=:low AND direct_high=:high"
        ),
        args,
    ).scalar_one()
    for user in (low, high):
        session.execute(
            text(
                f"INSERT INTO {T}chat_room_members(organization_id,room_id,user_id) "
                "VALUES(:o,:r,:u) ON CONFLICT DO NOTHING"
            ),
            {"o": organization_id, "r": rid, "u": user},
        )
    return one_room(session, organization_id, me, rid)


@router.patch(BASE + "/rooms/{room_id}")
def rename_room(
    organization_id: UUID, room_id: UUID, body: RenameRequest, auth: WriteAuth, session: DB
):
    ctx = org_scope(session, auth, organization_id)
    kind, role = room_access(session, organization_id, room_id, human_actor_id(ctx))
    if kind != "group":
        raise HTTPException(422, "Only group rooms can be renamed")
    if role != "admin":
        raise HTTPException(403, "Room admin required")
    session.execute(
        text(f"UPDATE {T}chat_rooms SET name=:name WHERE organization_id=:o AND id=:r"),
        {"o": organization_id, "r": room_id, "name": body.name},
    )
    audit(session, ctx, room_id, "chat.room.rename", {})
    return one_room(session, organization_id, human_actor_id(ctx), room_id)


@router.post(BASE + "/rooms/{room_id}/members")
def invite(organization_id: UUID, room_id: UUID, body: UsersRequest, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    kind, _ = room_access(session, organization_id, room_id, human_actor_id(ctx))
    if kind != "group":
        raise HTTPException(422, "Only group rooms accept members")
    known = people(session, organization_id)
    ids = list(dict.fromkeys(body.user_ids))
    if any(i not in known for i in ids):
        raise HTTPException(422, "Members must be active organisation members")
    args = {"o": organization_id, "r": room_id}
    # Serialise invites so concurrent requests cannot exceed the member cap.
    session.execute(
        text(f"SELECT 1 FROM {T}chat_rooms WHERE organization_id=:o AND id=:r FOR UPDATE"), args
    )
    current = {
        r[0]
        for r in session.execute(
            text(
                f"SELECT user_id FROM {T}chat_room_members WHERE organization_id=:o AND room_id=:r"
            ),
            args,
        )
    }
    added = [i for i in ids if i not in current]
    if len(current) + len(added) > MAX_ROOM_MEMBERS:
        raise HTTPException(422, f"A room holds at most {MAX_ROOM_MEMBERS} members")
    for user in added:
        session.execute(
            text(
                f"INSERT INTO {T}chat_room_members(organization_id,room_id,user_id) "
                "VALUES(:o,:r,:u) ON CONFLICT DO NOTHING"
            ),
            args | {"u": user},
        )
    audit(session, ctx, room_id, "chat.room.members.add", {"added": [str(i) for i in added]})
    return one_room(session, organization_id, human_actor_id(ctx), room_id)


@router.delete(BASE + "/rooms/{room_id}/members/{user_id}", status_code=204)
def remove_member(
    organization_id: UUID, room_id: UUID, user_id: UUID, auth: WriteAuth, session: DB
):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    kind, role = room_access(session, organization_id, room_id, me)
    if kind != "group":
        raise HTTPException(422, "Only group rooms have removable members")
    if user_id != me and role != "admin":
        raise HTTPException(403, "Room admin required")
    args = {"o": organization_id, "r": room_id, "u": user_id}
    if (
        session.execute(
            text(
                f"DELETE FROM {T}chat_room_members WHERE organization_id=:o AND room_id=:r "
                "AND user_id=:u RETURNING user_id"
            ),
            args,
        ).first()
        is None
    ):
        raise HTTPException(404, "Member not found")
    # Never leave a group without an admin: promote the longest-standing member.
    session.execute(
        text(f"""UPDATE {T}chat_room_members SET role='admin' WHERE organization_id=:o
          AND room_id=:r AND user_id=(SELECT user_id FROM {T}chat_room_members
            WHERE organization_id=:o AND room_id=:r ORDER BY joined_at,user_id LIMIT 1)
          AND NOT EXISTS(SELECT 1 FROM {T}chat_room_members WHERE organization_id=:o
            AND room_id=:r AND role='admin')"""),
        args,
    )
    audit(session, ctx, room_id, "chat.room.members.remove", {"user_id": str(user_id)})


@router.get(BASE + "/rooms/{room_id}/messages")
def messages(
    organization_id: UUID,
    room_id: UUID,
    auth: ReadAuth,
    session: DB,
    response: Response,
    limit: Annotated[int, Query(ge=1, le=100)] = 50,
    before: UUID | None = None,
):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    room_access(session, organization_id, room_id, me)
    response.headers["Cache-Control"] = "no-store"
    cursor = (
        ""
        if before is None
        else f""" AND (m.created_at,m.id)<(SELECT b.created_at,b.id FROM {T}chat_messages b
          WHERE b.organization_id=:o AND b.room_id=:r AND b.id=:before)"""
    )
    rows = (
        session.execute(
            text(
                f"{MESSAGE_SQL} AND m.room_id=:r{cursor} ORDER BY m.created_at DESC,m.id DESC "
                "LIMIT :limit"
            ),
            {"o": organization_id, "r": room_id, "before": before, "limit": limit},
        )
        .mappings()
        .all()
    )
    return {"items": shape(session, organization_id, me, list(reversed(rows)))}


@router.post(BASE + "/rooms/{room_id}/messages", status_code=201)
def post_message(
    organization_id: UUID, room_id: UUID, body: ChatMessageRequest, auth: WriteAuth, session: DB
):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    kind, _ = room_access(session, organization_id, room_id, me)
    args = {"o": organization_id, "r": room_id}
    if body.reply_to_id is not None and (
        session.execute(
            text(
                f"SELECT 1 FROM {T}chat_messages WHERE organization_id=:o AND room_id=:r AND id=:q"
            ),
            args | {"q": body.reply_to_id},
        ).first()
        is None
    ):
        raise HTTPException(422, "Reply target must be a message in this room")
    known = people(session, organization_id)
    if kind != "org":
        members = {
            r[0]
            for r in session.execute(
                text(
                    f"SELECT user_id FROM {T}chat_room_members WHERE organization_id=:o "
                    "AND room_id=:r"
                ),
                args,
            )
        }
        known = {k: v for k, v in known.items() if k in members}
    ids = list(dict.fromkeys(body.mention_ids))
    if any(i not in known for i in ids):
        raise HTTPException(422, "Mentions must target members of this room")
    images = [(decode_image(a.data_url), a) for a in body.attachments]
    mid = uuid4()
    session.execute(
        text(f"""INSERT INTO {T}chat_messages(organization_id,room_id,id,author_id,author_name,
          body,reply_to_id,mentions)
          VALUES(:o,:r,:id,:me,:name,:body,:q,CAST(:mentions AS jsonb))"""),
        args
        | {
            "id": mid,
            "me": me,
            "name": ctx.actor.display_name[:200],
            "body": body.body,
            "q": body.reply_to_id,
            "mentions": json.dumps([{"id": str(i), "display_name": known[i]} for i in ids]),
        },
    )
    for position, ((content_type, raw), a) in enumerate(images):
        session.execute(
            text(f"""INSERT INTO {T}chat_attachments(organization_id,id,message_id,position,
              content_type,width,height,data) VALUES(:o,:id,:m,:pos,:ct,:w,:h,:data)"""),
            {
                "o": organization_id,
                "id": uuid4(),
                "m": mid,
                "pos": position,
                "ct": content_type,
                "w": a.width,
                "h": a.height,
                "data": raw,
            },
        )
    session.execute(
        text(f"UPDATE {T}chat_rooms SET last_activity_at=now() WHERE organization_id=:o AND id=:r"),
        args,
    )
    row = session.execute(text(f"{MESSAGE_SQL} AND m.id=:id"), args | {"id": mid}).mappings().one()
    return shape(session, organization_id, me, [row])[0]


@router.get(BASE + "/attachments/{attachment_id}")
def attachment(organization_id: UUID, attachment_id: UUID, auth: ReadAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    row = session.execute(
        text(f"""SELECT a.message_id,a.content_type,a.data FROM {T}chat_attachments a
          WHERE a.organization_id=:o AND a.id=:id"""),
        {"o": organization_id, "id": attachment_id},
    ).one_or_none()
    if row is None:
        raise HTTPException(404, "Attachment not found")
    message_room(session, organization_id, row[0], human_actor_id(ctx))
    return Response(
        bytes(row[2]),
        media_type=row[1],
        headers={
            "Cache-Control": "private, max-age=86400",
            "X-Content-Type-Options": "nosniff",
            "Content-Disposition": "inline",
        },
    )


@router.post(BASE + "/rooms/{room_id}/read")
def mark_read(organization_id: UUID, room_id: UUID, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    room_access(session, organization_id, room_id, human_actor_id(ctx))
    session.execute(
        text(f"""INSERT INTO {T}chat_room_members(organization_id,room_id,user_id,last_read_at)
          VALUES(:o,:r,:me,clock_timestamp()) ON CONFLICT (organization_id,room_id,user_id)
          DO UPDATE SET last_read_at=clock_timestamp()"""),
        {"o": organization_id, "r": room_id, "me": human_actor_id(ctx)},
    )
    return {"read": True}


@router.post(BASE + "/messages/{message_id}/reactions")
def react(
    organization_id: UUID, message_id: UUID, body: ReactionRequest, auth: WriteAuth, session: DB
):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    message_room(session, organization_id, message_id, me)
    args = {"o": organization_id, "m": message_id, "me": me, "e": body.emoji}
    removed = session.execute(
        text(
            f"DELETE FROM {T}chat_reactions WHERE organization_id=:o AND message_id=:m "
            "AND user_id=:me AND emoji=:e RETURNING emoji"
        ),
        args,
    ).first()
    if removed is None:
        session.execute(
            text(
                f"INSERT INTO {T}chat_reactions(organization_id,message_id,user_id,emoji) "
                "VALUES(:o,:m,:me,:e) ON CONFLICT DO NOTHING"
            ),
            args,
        )
    return {"items": reactions(session, organization_id, [message_id], me)[message_id]}


@router.post(BASE + "/messages/{message_id}/pin")
def pin(organization_id: UUID, message_id: UUID, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    room, _, _, _ = message_room(session, organization_id, message_id, human_actor_id(ctx))
    args = {"o": organization_id, "r": room, "m": message_id}
    # Lock the room row so concurrent pins cannot exceed the cap.
    session.execute(
        text(f"SELECT 1 FROM {T}chat_rooms WHERE organization_id=:o AND id=:r FOR UPDATE"), args
    )
    count = session.execute(
        text(f"SELECT count(*) FROM {T}chat_pins WHERE organization_id=:o AND room_id=:r"), args
    ).scalar_one()
    exists = session.execute(
        text(f"SELECT 1 FROM {T}chat_pins WHERE organization_id=:o AND message_id=:m"), args
    ).first()
    if exists is None:
        if count >= MAX_PINS:
            raise HTTPException(409, f"A room holds at most {MAX_PINS} pins")
        session.execute(
            text(f"""INSERT INTO {T}chat_pins(organization_id,room_id,message_id,pinned_by,
              pinned_by_name) VALUES(:o,:r,:m,:me,:name)"""),
            args | {"me": human_actor_id(ctx), "name": ctx.actor.display_name[:200]},
        )
    return {"pinned": True}


@router.delete(BASE + "/messages/{message_id}/pin")
def unpin(organization_id: UUID, message_id: UUID, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    _, author, kind, role = message_room(session, organization_id, message_id, me)
    pinner = session.execute(
        text(f"SELECT pinned_by FROM {T}chat_pins WHERE organization_id=:o AND message_id=:m"),
        {"o": organization_id, "m": message_id},
    ).scalar_one_or_none()
    if kind == "group" and role != "admin" and author != me:
        raise HTTPException(403, "Room admin or message author required")
    if kind == "org" and me not in (author, pinner) and not org_admin(session, organization_id, me):
        raise HTTPException(403, "Message author, pinner or organisation admin required")
    session.execute(
        text(f"DELETE FROM {T}chat_pins WHERE organization_id=:o AND message_id=:m"),
        {"o": organization_id, "m": message_id},
    )
    return {"pinned": False}


@router.get(BASE + "/rooms/{room_id}/pins")
def pins(organization_id: UUID, room_id: UUID, auth: ReadAuth, session: DB, response: Response):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    room_access(session, organization_id, room_id, me)
    response.headers["Cache-Control"] = "no-store"
    rows = (
        session.execute(
            text(f"""SELECT * FROM ({MESSAGE_SQL} AND m.room_id=:r) msg JOIN (SELECT message_id,
              pinned_by_name,pinned_at FROM {T}chat_pins WHERE organization_id=:o AND room_id=:r)
              p ON p.message_id=msg.id ORDER BY p.pinned_at DESC LIMIT {MAX_PINS}"""),
            {"o": organization_id, "r": room_id},
        )
        .mappings()
        .all()
    )
    shaped_rows = [{k: v for k, v in r.items() if k != "message_id"} for r in rows]
    return {"items": shape(session, organization_id, me, shaped_rows)}


def fold(value: str) -> str:
    value = value.casefold().replace("đ", "d")
    return "".join(c for c in unicodedata.normalize("NFD", value) if not unicodedata.combining(c))


def contact_state(session: Session, me: UUID):
    contacts = {
        r[0]
        for r in session.execute(
            text(f"""SELECT CASE WHEN user_low=:me THEN user_high ELSE user_low END
              FROM {T}chat_contacts WHERE :me IN (user_low,user_high)"""),
            {"me": me},
        )
    }
    requests = (
        session.execute(
            text(f"""SELECT id,from_user,to_user,created_at FROM {T}chat_contact_requests
              WHERE :me IN (from_user,to_user) ORDER BY created_at DESC"""),
            {"me": me},
        )
        .mappings()
        .all()
    )
    return contacts, requests


@router.get(BASE + "/people")
def list_people(
    organization_id: UUID,
    auth: ReadAuth,
    session: DB,
    response: Response,
    q: Annotated[str | None, Query(max_length=80)] = None,
):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    response.headers["Cache-Control"] = "no-store"
    contacts, requests = contact_state(session, me)
    pending = {r["from_user"]: "incoming" for r in requests if r["to_user"] == me} | {
        r["to_user"]: "outgoing" for r in requests if r["from_user"] == me
    }
    avatars = {
        r[0]
        for r in session.execute(
            text(f"""SELECT a.user_id FROM identity.user_avatars a JOIN ({MEMBERS_SQL}) x
              ON x.id=a.user_id"""),
            {"o": organization_id},
        )
    }
    needle = fold(q.strip()) if q else ""
    # shortcut: filters in Python after loading the org roster; move to SQL (unaccent) for
    # organisations with many thousands of members.
    found = sorted(
        (
            (i, n)
            for i, n in people(session, organization_id).items()
            if i != me and needle in fold(n)
        ),
        key=lambda x: (fold(x[1]), str(x[0])),
    )[:50]
    return {
        "items": [
            {
                "id": i,
                "display_name": n,
                "has_avatar": i in avatars,
                "contact": i in contacts,
                "request": pending.get(i),
            }
            for i, n in found
        ]
    }


@router.get(BASE + "/people/{user_id}/avatar")
def person_avatar(
    organization_id: UUID, user_id: UUID, auth: ReadAuth, session: DB, response: Response
):
    org_scope(session, auth, organization_id)
    if user_id not in people(session, organization_id):
        raise HTTPException(404, "Person not found")
    response.headers["Cache-Control"] = "private, max-age=300"
    value = session.execute(
        text("SELECT data_url FROM identity.user_avatars WHERE user_id=:u"), {"u": user_id}
    ).scalar_one_or_none()
    return {"data_url": value}


@router.get(BASE + "/contacts")
def contacts(organization_id: UUID, auth: ReadAuth, session: DB, response: Response):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    response.headers["Cache-Control"] = "no-store"
    known = people(session, organization_id)
    mine, requests = contact_state(session, me)

    def side(direction: str, key: str) -> list[dict]:
        return [
            {
                "id": r["id"],
                "user_id": r[key],
                "display_name": known[r[key]],
                "created_at": r["created_at"],
            }
            for r in requests
            if r[direction] == me and r[key] in known
        ]

    return {
        "items": sorted(
            ({"user_id": i, "display_name": known[i]} for i in mine if i in known),
            key=lambda x: fold(x["display_name"]),
        ),
        "incoming": side("to_user", "from_user"),
        "outgoing": side("from_user", "to_user"),
    }


def make_contact(session: Session, a: UUID, b: UUID) -> None:
    session.execute(
        text(
            f"INSERT INTO {T}chat_contacts(user_low,user_high) VALUES(:low,:high) "
            "ON CONFLICT DO NOTHING"
        ),
        {"low": min(a, b), "high": max(a, b)},
    )


@router.post(BASE + "/contacts/requests", status_code=201)
def request_contact(organization_id: UUID, body: UserRequest, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    if body.user_id == me or body.user_id not in people(session, organization_id):
        raise HTTPException(404, "Person not found")
    # Per-pair lock: two crossing requests must end as one contact, not two pending.
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtext(:pair))"),
        {"pair": f"chat-contact:{min(me, body.user_id)}:{max(me, body.user_id)}"},
    )
    mine, requests = contact_state(session, me)
    if body.user_id in mine:
        return {"id": None, "status": "contact"}
    reverse = next(
        (r for r in requests if r["from_user"] == body.user_id and r["to_user"] == me), None
    )
    if reverse is not None:
        session.execute(
            text(f"DELETE FROM {T}chat_contact_requests WHERE id=:id"), {"id": reverse["id"]}
        )
        make_contact(session, me, body.user_id)
        audit(session, ctx, body.user_id, "chat.contact.accept", {}, "chat_contact")
        return {"id": reverse["id"], "status": "contact"}
    session.execute(
        text(f"""INSERT INTO {T}chat_contact_requests(id,from_user,to_user)
          VALUES(:id,:me,:u) ON CONFLICT (from_user,to_user) DO NOTHING"""),
        {"id": uuid4(), "me": me, "u": body.user_id},
    )
    rid = session.execute(
        text(f"SELECT id FROM {T}chat_contact_requests WHERE from_user=:me AND to_user=:u"),
        {"me": me, "u": body.user_id},
    ).scalar_one()
    audit(session, ctx, rid, "chat.contact.request", {"to": str(body.user_id)}, "chat_contact")
    return {"id": rid, "status": "pending"}


def answer(session: Session, ctx, request_id: UUID, accept: bool) -> dict:
    me = human_actor_id(ctx)
    sender = session.execute(
        text(
            f"DELETE FROM {T}chat_contact_requests WHERE id=:id AND to_user=:me RETURNING from_user"
        ),
        {"id": request_id, "me": me},
    ).scalar_one_or_none()
    if sender is None:
        raise HTTPException(404, "Request not found")
    if accept:
        make_contact(session, me, sender)
    action = "chat.contact.accept" if accept else "chat.contact.decline"
    audit(session, ctx, request_id, action, {"from": str(sender)}, "chat_contact")
    return {"status": "contact" if accept else "declined"}


@router.post(BASE + "/contacts/requests/{request_id}/accept")
def accept(organization_id: UUID, request_id: UUID, auth: WriteAuth, session: DB):
    return answer(session, org_scope(session, auth, organization_id), request_id, True)


@router.post(BASE + "/contacts/requests/{request_id}/decline")
def decline(organization_id: UUID, request_id: UUID, auth: WriteAuth, session: DB):
    return answer(session, org_scope(session, auth, organization_id), request_id, False)


@router.delete(BASE + "/contacts/{user_id}", status_code=204)
def remove_contact(organization_id: UUID, user_id: UUID, auth: WriteAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    me = human_actor_id(ctx)
    if (
        session.execute(
            text(
                f"DELETE FROM {T}chat_contacts WHERE user_low=:low AND user_high=:high "
                "RETURNING user_low"
            ),
            {"low": min(me, user_id), "high": max(me, user_id)},
        ).first()
        is not None
    ):
        audit(session, ctx, user_id, "chat.contact.remove", {}, "chat_contact")


def allowed_rooms(request: Request, org: UUID, actor: UUID) -> set[str] | None:
    # No long-lived DB transaction. Re-authenticate before each delivery/heartbeat.
    try:
        with get_session_factory()() as session:
            auth = _authenticate_human_session(request, session, get_settings())
            if auth.actor.actor_id != actor:
                return None
            org_scope(session, auth, org)
            return {
                str(r[0])
                for r in session.execute(
                    text(f"""SELECT r.id FROM {T}chat_rooms r WHERE r.organization_id=:o AND
                      (r.kind='org' OR EXISTS(SELECT 1 FROM {T}chat_room_members m
                        WHERE m.organization_id=r.organization_id AND m.room_id=r.id
                        AND m.user_id=:me))"""),
                    {"o": org, "me": actor},
                )
            }
    except Exception:
        return None


def parse(payload: str, org: UUID, actor: UUID) -> tuple[str, str | None] | None:
    try:
        event = json.loads(payload)
        if event["k"] == "chat" and event["o"] == str(org):
            return "chat", event["r"]
        if event["k"] == "contacts" and str(actor) in event["u"]:
            return "contacts", None
    except (TypeError, ValueError, KeyError):
        pass
    return None


async def stream(request: Request, org: UUID, actor: UUID) -> AsyncIterator[str]:
    if _slots.locked():
        yield "event: unavailable\ndata: {}\n\n"
        return
    async with _slots:
        dsn = (
            make_url(get_settings().runtime_db_url)
            .set(drivername="postgresql")
            .render_as_string(hide_password=False)
        )
        try:
            async with await psycopg.AsyncConnection.connect(
                dsn, autocommit=True, connect_timeout=5
            ) as conn:
                await conn.execute("LISTEN " + CHANNEL)
                if await anyio.to_thread.run_sync(allowed_rooms, request, org, actor) is None:
                    yield "event: revoked\ndata: {}\n\n"
                    return
                yield "retry: 3000\nevent: ready\ndata: {}\n\n"
                heartbeat = time.monotonic() + 15
                while not await request.is_disconnected():
                    event = None
                    async for notice in conn.notifies(
                        timeout=max(0.01, heartbeat - time.monotonic()), stop_after=1
                    ):
                        event = parse(notice.payload, org, actor)
                    if event is None and time.monotonic() < heartbeat:
                        continue
                    rooms = await anyio.to_thread.run_sync(allowed_rooms, request, org, actor)
                    if rooms is None:
                        yield "event: revoked\ndata: {}\n\n"
                        return
                    if event is None:
                        yield ": heartbeat\n\n"
                    elif event[0] == "contacts":
                        yield "event: contacts\ndata: {}\n\n"
                    elif event[1] in rooms:
                        yield f"event: chat\ndata: {json.dumps({'room_id': event[1]})}\n\n"
                    heartbeat = time.monotonic() + 15
        except psycopg.Error:
            yield "event: unavailable\ndata: {}\n\n"


@router.get(BASE + "/events")
def events(organization_id: UUID, request: Request, auth: ReadAuth, session: DB):
    ctx = org_scope(session, auth, organization_id)
    return StreamingResponse(
        stream(request, organization_id, human_actor_id(ctx)),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-store, no-transform",
            "X-Accel-Buffering": "no",
            "Connection": "keep-alive",
        },
    )
