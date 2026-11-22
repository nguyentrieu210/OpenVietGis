"""Commit-delivered SSE invalidations for project chat, tasks and private receipts."""
from __future__ import annotations

import asyncio
import json
import time
from collections.abc import AsyncIterator
from uuid import UUID

import anyio
import psycopg
from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import StreamingResponse
from sqlalchemy.engine import make_url

from openvietgis_api.adapters.http.security import _authenticate_human_session
from openvietgis_api.adapters.http.team_chat import BASE, DB, ReadAuth, scope
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.config import get_settings

router = APIRouter(tags=["team-realtime"])
CHANNEL = "ovg_team_collaboration"
# Limit dedicated PostgreSQL listener connections in each process.
MAX_LISTENERS = 40
_slots = asyncio.Semaphore(MAX_LISTENERS)


def matches(payload: str, org: UUID, project: UUID, actor: UUID) -> bool:
    try:
        event = json.loads(payload)
        return (
            event["o"] == str(org) and event["p"] == str(project)
            and (event["a"] is None or str(actor) in (event["a"], event["b"]))
        )
    except (TypeError, ValueError, KeyError):
        return False


def still_allowed(request: Request, org: UUID, project: UUID, actor: UUID) -> bool:
    # No long-lived DB transaction. Re-authenticate before each delivery/heartbeat.
    try:
        with get_session_factory()() as session:
            auth = _authenticate_human_session(request, session, get_settings())
            if auth.actor.actor_id != actor:
                return False
            scope(session, auth, org, project)
            return True
    except Exception:
        return False


async def stream(request: Request, org: UUID, project: UUID, actor: UUID) -> AsyncIterator[str]:
    if _slots.locked():
        yield "event: unavailable\ndata: {}\n\n"
        return
    async with _slots:
        settings = get_settings()
        dsn = make_url(settings.runtime_db_url).set(drivername="postgresql").render_as_string(hide_password=False)
        # Authenticate prior to LISTEN and before initial refresh, then every
        # relevant commit or heartbeat. Payload contains no user-facing content.
        try:
            async with await psycopg.AsyncConnection.connect(dsn, autocommit=True, connect_timeout=5) as conn:
                await conn.execute("LISTEN " + CHANNEL)
                if not await anyio.to_thread.run_sync(still_allowed, request, org, project, actor):
                    yield "event: revoked\ndata: {}\n\n"
                    return
                yield "retry: 3000\nevent: ready\ndata: {}\n\n"
                heartbeat = time.monotonic() + 15
                while not await request.is_disconnected():
                    relevant = False
                    kind = "change"
                    async for notice in conn.notifies(timeout=max(0.01, heartbeat-time.monotonic()), stop_after=1):
                        relevant = matches(notice.payload, org, project, actor)
                        if relevant and json.loads(notice.payload).get("kind") in ("spatial_change", "lock_change"):
                            kind = json.loads(notice.payload)["kind"]
                    if not relevant and time.monotonic() < heartbeat:
                        continue
                    if not await anyio.to_thread.run_sync(still_allowed, request, org, project, actor):
                        yield "event: revoked\ndata: {}\n\n"
                        return
                    if relevant:
                        yield f"event: {kind}\ndata: {{}}\n\n"
                    else:
                        yield ": heartbeat\n\n"
                    heartbeat = time.monotonic() + 15
        except psycopg.Error:
            yield "event: unavailable\ndata: {}\n\n"


@router.get(BASE + "/events")
def events(organization_id: UUID, project_id: UUID, request: Request, auth: ReadAuth, session: DB):
    ctx = scope(session, auth, organization_id, project_id)
    if ctx.actor.actor_id is None:
        raise HTTPException(403)
    return StreamingResponse(
        stream(request, organization_id, project_id, ctx.actor.actor_id),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-store, no-transform", "X-Accel-Buffering": "no", "Connection": "keep-alive"},
    )
