"""Partner-operated OpenVietGIS activity → signed webhook relay (stdlib only).

Run on the partner backend, not in the browser or the GIS API process.
"""
from __future__ import annotations

import hashlib
import hmac
import json
import os
import sqlite3
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from typing import Any
from uuid import UUID

TOPICS = frozenset({
    "changeset.committed", "changeset.reverted", "review.requested",
    "review.commented", "review.approved", "review.changes_requested",
})
MAX_PAGES = 50


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, "Redirect denied", headers, fp)


OPENER = urllib.request.build_opener(NoRedirect(), urllib.request.ProxyHandler({}))


def https_url(value: str) -> str:
    p = urllib.parse.urlsplit(value)
    if p.scheme != "https" or not p.hostname or p.username or p.password or p.fragment or p.query:
        raise ValueError("Configure an explicit HTTPS URL without credentials/query/fragment")
    return value.rstrip("/")


def signature(secret: bytes, timestamp: str, body: bytes) -> str:
    return "sha256=" + hmac.new(secret, timestamp.encode() + b"." + body, hashlib.sha256).hexdigest()


def event_body(item: dict[str, Any], project_id: str) -> bytes | None:
    if item["event_kind"] not in TOPICS:
        return None
    # No actor, summary, arbitrary metadata, attributes, or geometry.
    event = {
        "schema": "openvietgis.activity.v1",
        "id": str(UUID(item["id"])),
        "type": item["event_kind"],
        "project_id": str(UUID(project_id)),
        "resource_type": item["resource_type"],
        "resource_id": str(UUID(item["resource_id"])),
        "revision": item.get("revision_ref"),
        "created_at": item["created_at"],
    }
    return json.dumps(event, separators=(",", ":"), sort_keys=True).encode()


class Relay:
    def __init__(self, db: sqlite3.Connection, project_id: str,
                 page: Callable[[str | None], dict], deliver: Callable[[str, bytes], None]):
        self.db, self.project_id, self.page, self.deliver = db, str(UUID(project_id)), page, deliver
        db.executescript("""
          CREATE TABLE IF NOT EXISTS relay_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS relay_queue (
            seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
            body BLOB NOT NULL, delivered INTEGER NOT NULL DEFAULT 0
          );
        """)
        bound = db.execute("SELECT value FROM relay_state WHERE key='project'").fetchone()
        if bound and bound[0] != self.project_id:
            raise ValueError("Relay state already belongs to another project")
        with db:
            db.execute("INSERT OR IGNORE INTO relay_state VALUES ('project',?)", (self.project_id,))

    def poll(self) -> int:
        state = self.db.execute("SELECT value FROM relay_state WHERE key='checkpoint'").fetchone()
        previous = state[0] if state else None
        cursor, head, found = None, "", False
        entries: list[tuple[str, bytes]] = []
        for page_index in range(MAX_PAGES):
            response = self.page(cursor)
            items = response["items"]
            if not isinstance(items, list) or len(items) > 200:
                raise ValueError("Invalid activity page")
            if page_index == 0 and items:
                head = str(UUID(items[0]["id"]))
            # First run starts at current head; historical records are not sent.
            if previous is None:
                found = True
                break
            for item in items:
                event_id = str(UUID(item["id"]))
                if event_id == previous:
                    found = True
                    break
                body = event_body(item, self.project_id)
                if body is not None:
                    entries.append((event_id, body))
            if found:
                break
            cursor = response.get("next_cursor")
            if cursor is None:
                found = previous == ""
                break
        if not found:
            raise RuntimeError("Checkpoint unavailable or backlog exceeds scan limit; no checkpoint advanced")
        with self.db:
            for event_id, body in reversed(entries):
                self.db.execute("INSERT OR IGNORE INTO relay_queue(id,body) VALUES (?,?)", (event_id, body))
            self.db.execute("INSERT INTO relay_state VALUES ('checkpoint',?) "
                            "ON CONFLICT(key) DO UPDATE SET value=excluded.value", (head,))
        return len(entries)

    def flush(self, limit: int = 100) -> int:
        rows = self.db.execute("SELECT id, body FROM relay_queue WHERE delivered=0 ORDER BY seq LIMIT ?",
                               (limit,)).fetchall()
        count = 0
        for event_id, body in rows:
            # Ack only after successful HTTP. Retries can deliver duplicate event IDs.
            self.deliver(event_id, body)
            with self.db:
                self.db.execute("UPDATE relay_queue SET delivered=1 WHERE id=?", (event_id,))
            count += 1
        return count


def main() -> None:
    root = https_url(os.environ["OVG_API_BASE"])
    org, project = (str(UUID(os.environ[k])) for k in ("OVG_ORG_ID", "OVG_PROJECT_ID"))
    token = os.environ["OVG_SERVICE_TOKEN"]
    target = https_url(os.environ["WEBHOOK_URL"])
    secret = os.environ["WEBHOOK_SECRET"].encode()
    if len(secret) < 32:
        raise ValueError("WEBHOOK_SECRET must be at least 32 bytes")
    endpoint = f"{root}/organizations/{org}/projects/{project}/collaboration/activities"
    state = os.environ.get("RELAY_STATE_DB", "openvietgis-webhook.sqlite")
    poll_seconds = max(10, min(3600, int(os.environ.get("POLL_SECONDS", "30"))))
    fd = os.open(state, os.O_CREAT | os.O_WRONLY, 0o600)
    os.close(fd)
    if os.stat(state).st_mode & 0o077:
        raise ValueError("Relay state file must be private (chmod 600)")

    def read_page(cursor: str | None) -> dict:
        query = {"limit": 200}
        if cursor:
            query["cursor"] = cursor
        req = urllib.request.Request(endpoint + "?" + urllib.parse.urlencode(query),
                                     headers={"Authorization": "Bearer " + token, "Accept": "application/json"})
        with OPENER.open(req, timeout=10) as response:
            raw = response.read(4_000_001)
            if len(raw) > 4_000_000:
                raise ValueError("Activity response too large")
            return json.loads(raw)

    def deliver(event_id: str, body: bytes) -> None:
        stamp = str(int(time.time()))
        req = urllib.request.Request(target, data=body, method="POST", headers={
            "Content-Type": "application/json", "X-OVG-Event-Id": event_id,
            "X-OVG-Timestamp": stamp, "X-OVG-Signature": signature(secret, stamp, body),
        })
        with OPENER.open(req, timeout=10) as res:
            if not 200 <= res.status < 300:
                raise RuntimeError("Webhook delivery not acknowledged")

    with sqlite3.connect(state) as db:
        relay = Relay(db, project, read_page, deliver)
        failures = 0
        while True:
            try:
                # Always recheck API authorization before delivering queued events.
                relay.poll()
                relay.flush()
                failures = 0
                time.sleep(poll_seconds)
            except (OSError, ValueError, RuntimeError, KeyError, TypeError) as e:
                failures += 1
                # Do not log URLs, tokens, response bodies or event payloads.
                print(f"Relay paused ({type(e).__name__}); retry {failures}", flush=True)
                time.sleep(min(300, poll_seconds * 2 ** min(failures, 4)))


if __name__ == "__main__":
    main()
