"""Opt-in, ephemeral WebSocket presence. No spatial writes."""
from __future__ import annotations

import asyncio
import hashlib
import json
import time
from contextlib import suppress
from dataclasses import dataclass, field
from urllib.parse import urlsplit
from uuid import UUID, uuid4

import anyio
from fastapi import APIRouter, WebSocket, WebSocketDisconnect
from openvietgis_api.adapters.http.security import _authenticate_human_session
from openvietgis_api.adapters.http.team_chat import BASE, scope
from openvietgis_api.adapters.persistence.db import get_session_factory
from openvietgis_api.config import get_settings

router = APIRouter(tags=["team-presence"])
COLORS = ("#2563eb", "#db2777", "#059669", "#9333ea", "#b45309", "#0891b2")
MAX_PEERS = 64
MAX_ROOM = 40

@dataclass
class Peer:
    id: str
    actor: str
    name: str
    color: str
    cursor: list | None = None
    view: list | None = None
    queue: asyncio.Queue = field(default_factory=lambda: asyncio.Queue(maxsize=1))

    def public(self):
        return {k: getattr(self, k) for k in ("id", "actor", "name", "color", "cursor", "view")}

rooms: dict[tuple[UUID, UUID], dict[str, Peer]] = {}


def authorize(socket, org, project):
    try:
        with get_session_factory()() as session:
            auth = _authenticate_human_session(socket, session, get_settings())
            ctx = scope(session, auth, org, project)
            if ctx.actor.actor_id:
                return str(ctx.actor.actor_id), ctx.actor.display_name[:100]
    except Exception:
        return None
    return None


def origin_allowed(origin):
    # Cookies are ambient credentials: reject cross-site WebSocket requests.
    url = urlsplit(get_settings().web_base_url)
    return origin == f"{url.scheme}://{url.netloc}"


def numbers(value, limits):
    if not isinstance(value, list) or len(value) != len(limits):
        raise ValueError("Invalid coordinates")
    if any(type(v) not in (float, int) or not lo <= v <= hi
           for v, (lo, hi) in zip(value, limits)):
        raise ValueError("Invalid coordinates")
    return value


def update(peer, raw):
    if len(raw) > 2048:
        raise ValueError("Message too large")
    data = json.loads(raw)
    if not isinstance(data, dict) or set(data) - {"cursor", "view"} or not data:
        raise ValueError("Invalid presence")
    cursor = peer.cursor
    view = peer.view
    if "cursor" in data:
        cursor = None if data["cursor"] is None else numbers(data["cursor"], [(-180, 180), (-90, 90)])
    if "view" in data:
        view = numbers(data["view"], [(-180, 180), (-85, 85), (0, 24), (-180, 180), (0, 85)])
    peer.cursor, peer.view = cursor, view


def broadcast(room):
    users = [p.public() for p in room.values()]
    for p in room.values():
        if p.queue.full():
            p.queue.get_nowait()
        p.queue.put_nowait({"type": "presence", "self_id": p.id, "users": users})


async def sender(socket, peer):
    while True:
        await asyncio.wait_for(socket.send_json(await peer.queue.get()), 3)


@router.websocket(BASE + "/presence")
async def presence(socket: WebSocket, organization_id: UUID, project_id: UUID):
    if not origin_allowed(socket.headers.get("origin")):
        await socket.close(code=1008)
        return
    identity = await anyio.to_thread.run_sync(authorize, socket, organization_id, project_id)
    if not identity:
        await socket.close(code=1008)
        return
    key = (organization_id, project_id)
    if sum(map(len, rooms.values())) >= MAX_PEERS or len(rooms.get(key, {})) >= MAX_ROOM:
        await socket.close(code=1013)
        return
    actor, name = identity
    peer = Peer(str(uuid4()), actor, name, COLORS[int(hashlib.sha256(actor.encode()).hexdigest(), 16) % len(COLORS)])
    room = rooms.setdefault(key, {})
    room[peer.id] = peer
    task = None
    try:
        await socket.accept()
        task = asyncio.create_task(sender(socket, peer))
        broadcast(room)
        check = time.monotonic() + 5
        last = 0.0
        while True:
            if task.done():
                break
            if time.monotonic() >= check:
                current = await anyio.to_thread.run_sync(authorize, socket, organization_id, project_id)
                if not current or current[0] != actor:
                    await socket.close(code=1008)
                    break
                check = time.monotonic() + 5
            try:
                raw = await asyncio.wait_for(socket.receive_text(), timeout=max(0.01, check-time.monotonic()))
            except asyncio.TimeoutError:
                continue
            except KeyError:
                # Binary frames are not part of the presence protocol.
                await socket.close(code=1003)
                break
            now = time.monotonic()
            if now - last < 0.08:
                continue
            last = now
            try:
                update(peer, raw)
            except (ValueError, TypeError, OverflowError):
                await socket.close(code=1008)
                break
            broadcast(room)
    except (WebSocketDisconnect, RuntimeError):
        pass
    finally:
        if task:
            task.cancel()
            with suppress(asyncio.CancelledError, Exception):
                await task
        room.pop(peer.id, None)
        if room:
            broadcast(room)
        else:
            rooms.pop(key, None)
