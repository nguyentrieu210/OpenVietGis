"""E11 WebSocket presence differential parity (FastAPI oracle): handshake denials,
room broadcasts, cursor/view updates, protocol closes and live session revocation."""

import argparse
import json
import os
import sys
import time
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import uuid4

import psycopg
from websockets.exceptions import ConnectionClosed, InvalidStatus
from websockets.sync.client import connect

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import run as parity
from check_realtime import fastapi_server
from check_team import editor_member
from run import (
    COOKIE_NAME,
    baseline,
    configure_python,
    database,
    project_path,
    require,
    rust_client,
    scoped_member,
    session_headers,
    setup_gis_scope,
)

ORIGIN = "http://127.0.0.1:5173"


def fresh_session(owner, user_id):
    """A new web session for an existing user, so it can be revoked on its own."""
    sid = uuid4()
    token, digest, _ = baseline.make_session_and_csrf(sid, baseline.DEFAULT_CSRF_SECRET)
    with psycopg.connect(owner) as conn:
        link = conn.execute(
            "SELECT id FROM identity.identity_links WHERE user_id=%s LIMIT 1", (user_id,)
        ).fetchone()[0]
        conn.execute(
            """INSERT INTO identity.web_sessions (id, user_id, identity_link_id, secret_hash,
               issued_at, expires_at)
               VALUES (%s, %s, %s, %s, now(), now() + interval '1 hour')""",
            (sid, user_id, link, digest),
        )
    return sid, token


class Scenario:
    """One server's run; peer IDs are replaced by stable labels per actor."""

    def __init__(self, base_url):
        self.ws_base = base_url.replace("http://", "ws://", 1)
        self.events = []
        self.labels = {}

    def open(self, path, token, origin=ORIGIN):
        headers = {"Origin": origin} if origin else {}
        if token:
            headers["Cookie"] = f"{COOKIE_NAME}={token}"
        return connect(self.ws_base + path, additional_headers=headers, open_timeout=10,
                       close_timeout=5)

    def denied(self, label, path, token, origin=ORIGIN):
        try:
            socket = self.open(path, token, origin)
        except InvalidStatus as error:
            self.events.append((label, error.response.status_code))
            return
        socket.close()
        self.events.append((label, "accepted"))

    def presence(self, label, socket):
        message = json.loads(socket.recv(timeout=10))
        users = message["users"]
        for user in users:
            self.labels.setdefault(user["id"], f"peer-{user['actor']}")
        message["self_id"] = self.labels.get(message["self_id"], "unknown")
        for user in users:
            user["id"] = self.labels[user["id"]]
        self.events.append((label, message))

    def closed(self, label, socket, timeout=10):
        try:
            while True:
                socket.recv(timeout=timeout)
        except ConnectionClosed as error:
            self.events.append((label, error.rcvd.code if error.rcvd else None))
        except TimeoutError:
            self.events.append((label, "still open"))


def run_scenario(base_url, owner, fixture, a_token, editor_id, scoped_token):
    project = project_path(fixture["org_a_id"], fixture["project_a_id"])
    path = f"{project}/collaboration/presence"
    foreign = f"{project_path(fixture['org_b_id'], fixture['project_b_id'])}"
    s = Scenario(base_url)
    s.denied("403 cross-site origin", path, a_token, origin="https://evil.example")
    s.denied("403 missing origin", path, a_token, origin=None)
    s.denied("403 anonymous", path, None)
    s.denied("403 foreign project", f"{foreign}/collaboration/presence", a_token)
    s.denied("403 GIS-scoped member", path, scoped_token)
    s.denied("403 malformed project id",
             f"{project_path(fixture['org_a_id'], 'nope')}/collaboration/presence", a_token)

    first = s.open(path, a_token)
    s.presence("owner joins", first)
    _, editor_token = fresh_session(owner, editor_id)
    second = s.open(path, editor_token)
    s.presence("owner sees editor join", first)
    s.presence("editor joins", second)
    time.sleep(0.15)
    first.send(json.dumps({"cursor": [105.8, 21.0]}))
    s.presence("owner cursor (owner)", first)
    s.presence("owner cursor (editor)", second)
    time.sleep(0.15)
    second.send(json.dumps({"view": [105, 21, 12.5, 0, 45], "cursor": None}))
    s.presence("editor view (owner)", first)
    s.presence("editor view (editor)", second)
    time.sleep(0.15)
    first.send(json.dumps({"cursor": [200, 0]}))
    s.closed("owner invalid cursor closes", first)
    s.presence("editor sees owner leave", second)
    time.sleep(0.15)
    second.send(b"\x00")
    s.closed("binary frame closes", second)

    revoked_id, revoked_token = fresh_session(owner, editor_id)
    third = s.open(path, revoked_token)
    s.presence("editor rejoins", third)
    with psycopg.connect(owner) as conn:
        conn.execute("UPDATE identity.web_sessions SET revoked_at=now() WHERE id=%s",
                     (revoked_id,))
    s.closed("revoked session closes within 5 s", third, timeout=12)
    return s.events


@parity.case
def same_scenario(label, expected, actual):
    require(len(expected) == len(actual), f"{len(actual)} Rust events, {len(expected)} FastAPI")
    for (name, left), (_, right) in zip(expected, actual, strict=True):
        mismatch = parity.difference(left, right) if isinstance(left, dict) else (
            None if left == right else f"{left!r} != {right!r}")
        require(mismatch is None, f"{name}: {mismatch}")
    print(f"PASS {label} ({len(expected)} events)", flush=True)


@parity.case
def expected_codes(label, events):
    codes = dict(event for event in events if not isinstance(event[1], dict))
    wanted = {
        "403 cross-site origin": 403,
        "403 anonymous": 403,
        "403 GIS-scoped member": 403,
        "owner invalid cursor closes": 1008,
        "binary frame closes": 1003,
        "revoked session closes within 5 s": 1008,
    }
    for name, code in wanted.items():
        require(codes.get(name) == code, f"{name}: {codes.get(name)!r}, wanted {code}")
    print(f"PASS {label}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-presence-") as directory,
        database(admin) as (owner, runtime),
    ):
        work = Path(directory)
        configure_python(owner, runtime, work)
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner, seed=20261126, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
        )
        csrf = session_headers(
            fixture["session_a_token"], **{"x-openvietgis-csrf": fixture["csrf_a_token"]}
        )
        with fastapi_server(work) as oracle, rust_client(binary, runtime, work) as rust:
            user, membership, scoped_token = scoped_member(owner, fixture)
            setup_gis_scope(
                oracle,
                fixture,
                project_path(fixture["org_a_id"], fixture["project_a_id"]),
                fixture["layers"]["epsg4326_1k"]["layer_id"],
                membership,
                user,
            )
            editor_id, _, _ = editor_member(owner, oracle, fixture, csrf)
            token = fixture["session_a_token"]
            runs = [
                run_scenario(str(client.base_url).rstrip("/"), owner, fixture, token, editor_id,
                             scoped_token)
                for client in (oracle, rust)
            ]
            expected_codes("E11 presence FastAPI close codes", runs[0])
            expected_codes("E11 presence Rust close codes", runs[1])
            same_scenario("E11 presence scenario", runs[0], runs[1])
    print("E11 presence parity finished", flush=True)


if __name__ == "__main__":
    main()
