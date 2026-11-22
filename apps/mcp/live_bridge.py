"""Ephemeral, scope-limited live map relay. Single process; no GIS credentials."""
from __future__ import annotations
import hashlib
import math
import secrets
import threading
import time
from dataclasses import dataclass, field
from typing import Any

TTL = 900
HEARTBEAT_TTL = 15
COMMAND_TTL = 40
MAX_SESSIONS = 128
METHODS = {"map_view", "map_selection", "map_navigate"}


class BridgeError(Exception):
    def __init__(self, status: int, message: str):
        super().__init__(message)
        self.status = status


def digest(secret: str) -> str:
    return hashlib.sha256(secret.encode()).hexdigest()


def number(value: Any, low: float, high: float) -> float:
    if type(value) not in (float, int) or not low <= value <= high or not math.isfinite(value):
        raise BridgeError(422, "Invalid coordinate or zoom")
    return float(value)


def command_params(method: str, params: Any) -> dict:
    if method not in METHODS or not isinstance(params, dict):
        raise BridgeError(422, "Unsupported map command")
    if method == "map_navigate":
        if set(params) != {"longitude", "latitude", "zoom"}:
            raise BridgeError(422, "Invalid navigation arguments")
        return {"longitude": number(params["longitude"], -180, 180),
                "latitude": number(params["latitude"], -85, 85),
                "zoom": number(params["zoom"], 0, 22)}
    if params:
        raise BridgeError(422, "Unexpected command arguments")
    return {}


def clean_result(method: str, result: Any) -> Any:
    if method == "map_navigate":
        if result is not None:
            raise BridgeError(422, "Unexpected navigation response")
        return None
    if not isinstance(result, dict) or result.get("crs") != "EPSG:4326":
        raise BridgeError(422, "Expected WGS84 map result")
    if method == "map_view":
        center, bbox = result.get("center"), result.get("bbox")
        if not isinstance(center, list) or len(center) != 2 or not isinstance(bbox, list) or len(bbox) != 4:
            raise BridgeError(422, "Invalid viewport")
        return {"crs": "EPSG:4326", "center": [number(center[0], -180, 180), number(center[1], -90, 90)],
                "bbox": [number(bbox[0], -10000, 10000), number(bbox[1], -90, 90),
                         number(bbox[2], -10000, 10000), number(bbox[3], -90, 90)],
                "zoom": number(result.get("zoom"), 0, 22)}
    geometry = result.get("geometry")
    depths = {"Point": 0, "MultiPoint": 1, "LineString": 1, "MultiLineString": 2, "Polygon": 2, "MultiPolygon": 3}
    if not isinstance(geometry, dict) or not isinstance(geometry.get("type"), str) or geometry.get("type") not in depths:
        raise BridgeError(422, "Invalid selected geometry")
    count = 0
    nodes = 0
    def walk(coords, depth):
        nonlocal count, nodes
        nodes += 1
        if nodes > 4096 or not isinstance(coords, list) or not 1 <= len(coords) <= 2048:
            raise BridgeError(422, "Invalid or oversized selection")
        if depth == 0:
            count += 1
            if count > 2048 or len(coords) != 2:
                raise BridgeError(422, "Invalid or oversized selection")
            return [number(coords[0], -180, 180), number(coords[1], -90, 90)]
        return [walk(item, depth - 1) for item in coords]
    return {"crs": "EPSG:4326", "geometry": {"type": geometry["type"], "coordinates": walk(geometry.get("coordinates"), depths[geometry["type"]])}}


@dataclass
class Command:
    id: str
    method: str
    params: dict
    expires: float
    state: str = "queued"
    result: Any = None


@dataclass
class Session:
    browser_hash: str
    ai_hash: str
    expires: float
    last_seen: float
    command: Command | None = None
    last_command: float = -1000


class BridgeStore:
    def __init__(self, clock=time.monotonic):
        self.clock = clock
        self.sessions: dict[str, Session] = {}
        self.lock = threading.RLock()

    def _purge(self):
        now = self.clock()
        for sid in list(self.sessions):
            s = self.sessions[sid]
            if now >= s.expires or now - s.last_seen > HEARTBEAT_TTL:
                del self.sessions[sid]

    def create(self):
        with self.lock:
            self._purge()
            if len(self.sessions) >= MAX_SESSIONS:
                raise BridgeError(429, "Bridge is busy")
            sid, browser, ai = secrets.token_urlsafe(18), secrets.token_urlsafe(32), secrets.token_urlsafe(32)
            now = self.clock()
            self.sessions[sid] = Session(digest(browser), digest(ai), now + TTL, now)
            return {"session_id": sid, "browser_token": browser, "ai_token": ai, "expires_in": TTL}

    def _get(self, sid, token, role):
        self._purge()
        s = self.sessions.get(sid)
        hash_value = getattr(s, role + "_hash", "") if s else ""
        if not hash_value or not secrets.compare_digest(hash_value, digest(token)):
            raise BridgeError(404, "Session unavailable")
        return s

    def _expire_command(self, s):
        c = s.command
        if c and c.state in {"queued", "claimed"} and self.clock() >= c.expires:
            c.state = "expired"

    def ping(self, sid, token):
        with self.lock:
            s = self._get(sid, token, "browser")
            s.last_seen = self.clock()
            self._expire_command(s)
            return {"alive": True}

    def poll(self, sid, token):
        with self.lock:
            s = self._get(sid, token, "browser")
            s.last_seen = self.clock()
            self._expire_command(s)
            c = s.command
            if c is None or c.state != "queued":
                return {"command": None}
            c.state = "claimed"
            return {"command": {"id": c.id, "method": c.method, "params": c.params,
                                "expires_in": max(0, c.expires - self.clock())}}

    def submit(self, sid, token, method, params):
        with self.lock:
            s = self._get(sid, token, "ai")
            self._expire_command(s)
            params = command_params(method, params)
            if s.command and s.command.state in {"queued", "claimed", "done", "denied"}:
                raise BridgeError(409, "Previous command still pending or unread")
            if self.clock() - s.last_command < 2:
                raise BridgeError(429, "Try again after two seconds")
            s.last_command = self.clock()
            c = s.command = Command(secrets.token_urlsafe(18), method, params, self.clock() + COMMAND_TTL)
            return {"command_id": c.id}

    def ack(self, sid, token, cid, ok, result):
        with self.lock:
            s = self._get(sid, token, "browser")
            s.last_seen = self.clock()
            self._expire_command(s)
            c = s.command
            if not c or c.id != cid or c.state != "claimed":
                raise BridgeError(409, "Command unavailable")
            if type(ok) is not bool:
                raise BridgeError(422, "Invalid acknowledgement")
            c.result = clean_result(c.method, result) if ok else None
            c.state = "done" if ok else "denied"
            return {"accepted": True}

    def result(self, sid, token, cid):
        with self.lock:
            s = self._get(sid, token, "ai")
            self._expire_command(s)
            c = s.command
            if not c or c.id != cid:
                raise BridgeError(404, "Command unavailable")
            if c.state in {"done", "denied", "expired"}:
                out = {"state": c.state, "result": c.result}
                s.command = None
                return out
            return {"state": c.state}

    def close(self, sid, token):
        with self.lock:
            self._get(sid, token, "browser")
            del self.sessions[sid]
            return {"closed": True}
