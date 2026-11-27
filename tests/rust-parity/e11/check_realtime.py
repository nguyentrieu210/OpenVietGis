"""E11 collaboration activity cursor/replay and SSE differential parity (FastAPI oracle)."""

import argparse
import contextlib
import hashlib
import json
import os
import queue
import socket
import subprocess
import sys
import threading
import time
from datetime import UTC, datetime, timedelta
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import uuid4

import httpx
import psycopg

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests/rust-parity"))
import run as parity
from run import (
    baseline,
    configure_python,
    database,
    project_path,
    require,
    rust_client,
    scoped_member,
    session_headers,
    session_id,
    setup_gis_scope,
)

READY = "retry: 3000\nevent: ready\ndata: {}\n\n"
CHANNEL = "ovg_team_collaboration"
SSE_HEADERS = ("content-type", "cache-control", "x-accel-buffering")


@contextlib.contextmanager
def fastapi_server(work):
    """The oracle under uvicorn: TestClient buffers whole bodies, so it cannot stream SSE."""
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    command = [sys.executable, "-m", "uvicorn", "openvietgis_api.main:app"]
    command += ["--host", "127.0.0.1", "--port", str(port), "--log-level", "warning"]
    with (
        (work / "fastapi.log").open("w+") as log,
        subprocess.Popen(command, cwd=work, env=dict(os.environ), stdout=log, stderr=log) as proc,
    ):
        try:
            with httpx.Client(
                base_url=f"http://127.0.0.1:{port}", trust_env=False, timeout=10
            ) as client:
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline and proc.poll() is None:
                    try:
                        if client.get("/api/v1/health/live").status_code == 200:
                            break
                    except httpx.TransportError:
                        time.sleep(0.2)
                else:
                    log.seek(0)
                    raise RuntimeError(f"FastAPI oracle did not start: {log.read()[-2000:]}")
                yield client
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


class Pair:
    def __init__(self, oracle, rust):
        self.oracle, self.rust = oracle, rust

    def both(self, method, path, **kwargs):
        return (
            self.oracle.request(method, path, **kwargs),
            self.rust.request(method, path, **kwargs),
        )


def validation_shape(body):
    """422 request validation: FastAPI {detail:[...]}; Rust keeps the shape, not pydantic text."""
    return {"detail": isinstance(body.get("detail"), list)} if "detail" in body else body


def first_page(body):
    return body | {"cutoff": "<cutoff>", "next_cursor": body["next_cursor"] is not None}


@parity.case
def check(label, pair, path, status, *, headers, params=None, normalize=None):
    expected, actual = pair.both("GET", path, headers=headers, params=params)
    require(
        expected.status_code == status,
        f"FastAPI status {expected.status_code}, wanted {status}: {expected.text[:300]}",
    )
    require(
        actual.status_code == status,
        f"Rust status {actual.status_code}, wanted {status}: {actual.text[:300]}",
    )
    left, right = expected.json(), actual.json()
    if normalize is not None:
        left, right = normalize(left), normalize(right)
    mismatch = parity.difference(left, right)
    require(mismatch is None, mismatch)
    require(
        expected.headers.get("cache-control") == actual.headers.get("cache-control"),
        "Cache-Control differs from FastAPI",
    )
    print(f"PASS {label}", flush=True)
    return expected.json(), actual.json()


def commit_changes(oracle, owner, fixture, headers, count):
    base = project_path(fixture["org_a_id"], fixture["project_a_id"])
    source = fixture["layers"]["edit_4326"]["source_id"]
    receipts = []
    for index in range(count):
        with psycopg.connect(owner) as conn:
            revision = conn.execute(
                "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (source,)
            ).fetchone()[0]
        feature = {"code": "E11", "ward": "W1", "metric_val": index, "active": True}
        response = oracle.post(
            f"{base}/sources/{source}/changesets",
            headers=headers,
            json={
                "operation_id": f"e11-activity-{uuid4()}",
                "base_content_revision": revision,
                "summary": f"E11 activity {index}",
                "operations": [
                    {
                        "op_type": "create",
                        "feature_id": f"e11-activity-{uuid4()}",
                        "geometry": {"type": "Point", "coordinates": [105.8 + index / 100, 21]},
                        "properties": feature,
                    }
                ],
            },
        )
        require(response.status_code == 201, f"fixture commit failed: {response.text}")
        receipts.append(response.json())
    return receipts


def seed_activities(oracle, owner, fixture, headers):
    base = project_path(fixture["org_a_id"], fixture["project_a_id"])
    _, latest = commit_changes(oracle, owner, fixture, headers, 2)
    reverted = oracle.post(
        f"{base}/changesets/{latest['id']}/revert",
        headers=headers,
        json={"operation_id": f"e11-revert-{uuid4()}"},
    )
    require(reverted.status_code == 201, f"fixture revert failed: {reverted.text}")
    (last,) = commit_changes(oracle, owner, fixture, headers, 1)
    review = oracle.post(
        f"{base}/collaboration/reviews",
        headers=headers,
        json={"changeset_id": last["id"], "assignee_user_id": fixture["user_a_id"]},
    )
    require(review.status_code == 201, f"fixture review failed: {review.text}")
    comment = oracle.post(
        f"{base}/collaboration/reviews/{review.json()['id']}/comments",
        headers=headers,
        json={"body": "E11 parity comment"},
    )
    require(comment.status_code == 201, f"fixture review comment failed: {comment.text}")


@parity.case
def walk(label, client, path, headers, full):
    seen, cursor = [], None
    while True:
        params = {"limit": 1} | ({"cursor": cursor} if cursor else {})
        response = client.get(path, headers=headers, params=params)
        require(response.status_code == 200, f"status {response.status_code}: {response.text}")
        body = response.json()
        seen += [item["id"] for item in body["items"]]
        cursor = body["next_cursor"]
        if not cursor:
            break
    require(seen == [item["id"] for item in full["items"]], "paged walk differs from page one")
    print(f"PASS {label}", flush=True)


def service_bearers(oracle, fixture, csrf_headers):
    """A project-reader machine credential and one whose principal holds no grant."""
    org = f"/api/v1/organizations/{fixture['org_a_id']}"
    bearers = []
    for name, role in (("E11 reader", "viewer"), ("E11 no grant", None)):
        principal = oracle.post(f"{org}/service-principals", headers=csrf_headers,
                                json={"name": name})
        require(principal.status_code == 201, f"service principal: {principal.text}")
        pid = principal.json()["id"]
        if role:
            grant = oracle.post(
                f"{org}/grants", headers=csrf_headers,
                json={"subject_kind": "service_principal", "subject_id": pid,
                      "scope_kind": "project", "project_id": fixture["project_a_id"],
                      "role": role},
            )
            require(grant.status_code == 201, f"service grant: {grant.text}")
        credential = oracle.post(f"{org}/service-principals/{pid}/credentials",
                                 headers=csrf_headers, json={"name": name})
        require(credential.status_code == 201, f"service credential: {credential.text}")
        bearers.append({"authorization": f"Bearer {credential.json()['secret']}"})
    return bearers


def compare_service_activities(pair, fixture, bearers):
    reader, ungranted = bearers
    path = f"{project_path(fixture['org_a_id'], fixture['project_a_id'])}"
    path += "/collaboration/activities"
    check("E11 activities service reader first page", pair, path, 200, headers=reader,
          normalize=first_page)
    full = pair.oracle.get(path, headers=reader).json()
    for name, client in (("FastAPI", pair.oracle), ("Rust", pair.rust)):
        body = client.get(path, headers=reader, params={"limit": 2}).json()
        check(f"E11 activities service continues {name} cursor", pair, path, 200,
              headers=reader, params={"limit": 2, "cursor": body["next_cursor"]})
        walk(f"E11 activities service keyset walk on {name}", client, path, reader, full)
        # A cursor binds the credential: a human session cannot continue it.
        check(f"E11 activities 422 service cursor replayed by human ({name})", pair, path,
              422, headers=session_headers(fixture["session_a_token"]),
              params={"cursor": body["next_cursor"]})
    check("E11 activities 403 service principal without grant", pair, path, 403,
          headers=ungranted)
    check("E11 activities 401 invalid bearer does not fall back to cookie", pair, path, 401,
          headers=session_headers(fixture["session_a_token"])
          | {"authorization": "Bearer unavailable"})


def compare_activities(pair, fixture, scoped_token):
    a = session_headers(fixture["session_a_token"])
    base = project_path(fixture["org_a_id"], fixture["project_a_id"])
    path = f"{base}/collaboration/activities"
    foreign = project_path(fixture["org_b_id"], fixture["project_b_id"])
    foreign += "/collaboration/activities"

    check("E11 activities first page", pair, path, 200, headers=a, normalize=first_page)
    full = pair.oracle.get(path, headers=a).json()
    kinds = {item["event_kind"] for item in full["items"]}
    require(
        {"changeset.committed", "changeset.reverted", "review.requested"} <= kinds,
        f"activity fixture incomplete: {sorted(kinds)}",
    )
    check("E11 activities limit=2", pair, path, 200, headers=a, params={"limit": 2},
          normalize=first_page)

    # Cursors are wire compatible: each server continues the other's page exactly.
    issued = {}
    for name, client in (("FastAPI", pair.oracle), ("Rust", pair.rust)):
        body = client.get(path, headers=a, params={"limit": 2}).json()
        require(body["next_cursor"], f"{name} issued no continuation cursor")
        issued[name] = body["next_cursor"]
        check(f"E11 activities continue {name} cursor", pair, path, 200, headers=a,
              params={"limit": 2, "cursor": body["next_cursor"]})

    for name, client in (("FastAPI", pair.oracle), ("Rust", pair.rust)):
        walk(f"E11 activities full keyset walk on {name}", client, path, a, full)

    scope = hashlib.sha256(
        f"human|{fixture['user_a_id']}|None|{fixture['org_a_id']}|{fixture['project_a_id']}"
        .encode("ascii")
    ).hexdigest()
    from openvietgis_api.domains.collaboration.activity_cursor import (
        ActivityCursor,
        encode_cursor,
    )

    now = datetime.now(UTC)
    forged = {
        "expired": ActivityCursor(now - timedelta(minutes=20), now - timedelta(minutes=21),
                                  1, uuid4()),
        "invisible boundary": ActivityCursor(now, now - timedelta(seconds=1), 1, uuid4()),
        "future cutoff": ActivityCursor(now + timedelta(minutes=5), now, 0, uuid4()),
    }
    invalid = {name: encode_cursor(cursor, scope) for name, cursor in forged.items()}
    invalid["other scope"] = encode_cursor(forged["invisible boundary"], "0" * 64)
    invalid["garbage"] = "not-a-cursor"
    invalid["padded"] = issued["FastAPI"] + "="
    tail = "BB" if issued["FastAPI"].endswith("AA") else "AA"
    invalid["tampered"] = issued["FastAPI"][:-2] + tail
    for name, token in invalid.items():
        check(f"E11 activities 422 INVALID_ACTIVITY_CURSOR {name}", pair, path, 422,
              headers=a, params={"cursor": token})

    for name, params in {
        "limit=0": {"limit": 0},
        "limit=201": {"limit": 201},
        "limit=abc": {"limit": "abc"},
        "empty cursor": {"cursor": ""},
        "cursor 513 chars": {"cursor": "A" * 513},
    }.items():
        check(f"E11 activities 422 validation {name}", pair, path, 422, headers=a,
              params=params, normalize=validation_shape)
    check("E11 activities 422 malformed project id", pair,
          f"{project_path(fixture['org_a_id'], 'not-a-uuid')}/collaboration/activities", 422,
          headers=a, normalize=validation_shape)
    check("E11 activities 401 anonymous", pair, path, 401, headers={})
    check("E11 activities 401 invalid bearer", pair, path, 401,
          headers={"authorization": "Bearer unavailable"})
    check("E11 activities 404 foreign project", pair, foreign, 404, headers=a)
    check("E11 activities 404 unknown project", pair,
          f"{project_path(fixture['org_a_id'], uuid4())}/collaboration/activities", 404, headers=a)
    check("E11 activities 403 GIS-scoped member", pair, path, 403,
          headers=session_headers(scoped_token))


class Frames:
    """Read one SSE response on a thread so the test can wait with a deadline."""

    def __init__(self, client, path, headers):
        self._stream = client.stream(
            "GET", path, headers=headers, timeout=httpx.Timeout(10, read=None)
        )
        self.response = self._stream.__enter__()
        self._frames = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        buffer = ""
        try:
            for chunk in self.response.iter_text():
                buffer += chunk
                while "\n\n" in buffer:
                    frame, buffer = buffer.split("\n\n", 1)
                    self._frames.put(frame + "\n\n")
        except (httpx.HTTPError, RuntimeError, ValueError):
            pass
        self._frames.put("<closed>")

    def next(self, timeout=10):
        try:
            return self._frames.get(timeout=timeout)
        except queue.Empty:
            return "<timeout>"

    def close(self):
        with contextlib.suppress(httpx.HTTPError, RuntimeError):
            self._stream.__exit__(None, None, None)


@parity.case
def same_frame(label, streams, wanted, timeout=10):
    frames = [stream.next(timeout) for stream in streams]
    require(frames[0] == wanted, f"FastAPI frame {frames[0]!r}, wanted {wanted!r}")
    require(frames[1] == wanted, f"Rust frame {frames[1]!r}, wanted {wanted!r}")
    print(f"PASS {label}", flush=True)


def compare_events(pair, owner, fixture, member, csrf_headers, bearers):
    scoped_token = member[2]
    a = session_headers(fixture["session_a_token"])
    org, project = fixture["org_a_id"], fixture["project_a_id"]
    path = f"{project_path(org, project)}/collaboration/events"
    foreign = f"{project_path(fixture['org_b_id'], fixture['project_b_id'])}/collaboration/events"
    check("E11 events 401 anonymous", pair, path, 401, headers={})
    check("E11 events 401 invalid bearer", pair, path, 401,
          headers={"authorization": "Bearer unavailable"})
    check("E11 events 404 foreign project", pair, foreign, 404, headers=a)
    check("E11 events 403 service principal", pair, path, 403, headers=bearers[0])
    check("E11 events 422 service principal malformed project id", pair,
          f"{project_path(org, 'not-a-uuid')}/collaboration/events", 422,
          headers=bearers[0], normalize=validation_shape)
    check("E11 events 403 GIS-scoped member", pair, path, 403,
          headers=session_headers(scoped_token))
    check("E11 events 422 malformed project id", pair,
          f"{project_path(org, 'not-a-uuid')}/collaboration/events", 422, headers=a,
          normalize=validation_shape)
    compare_chat_events(pair, owner, fixture, member, csrf_headers, bearers)

    streams = [Frames(client, path, a) for client in (pair.oracle, pair.rust)]
    try:
        compare_stream(streams, pair, owner, fixture, csrf_headers)
    finally:
        for stream in streams:
            stream.close()


def pg_notify(owner, channel, payload):
    text = payload if isinstance(payload, str) else json.dumps(payload)
    with psycopg.connect(owner, autocommit=True) as conn:
        conn.execute("SELECT pg_notify(%s, %s)", (channel, text))


def compare_chat_events(pair, owner, fixture, member, csrf_headers, bearers):
    """org_chat.events for a (GIS-scoped) organization member; ends by revoking it."""
    user, membership, token = member
    org = fixture["org_a_id"]
    path = f"/api/v1/organizations/{org}/chat/events"
    mine = session_headers(token)
    check("E11 chat events 401 anonymous", pair, path, 401, headers={})
    check("E11 chat events 401 invalid bearer", pair, path, 401,
          headers={"authorization": "Bearer unavailable"})
    check("E11 chat events 401 bearer on malformed organization id", pair,
          "/api/v1/organizations/not-a-uuid/chat/events", 401, headers=bearers[0])
    check("E11 chat events 403 service principal", pair, path, 403, headers=bearers[0])
    check("E11 chat events 404 foreign organization", pair,
          f"/api/v1/organizations/{fixture['org_b_id']}/chat/events", 404, headers=mine)
    check("E11 chat events 422 malformed organization id", pair,
          "/api/v1/organizations/not-a-uuid/chat/events", 422, headers=mine,
          normalize=validation_shape)

    rooms = f"/api/v1/organizations/{org}/chat/rooms"
    room = pair.oracle.post(rooms, headers=csrf_headers,
                            json={"name": "E11 parity room", "member_ids": [user]})
    require(room.status_code == 201, f"fixture chat room failed: {room.text}")
    room_id = room.json()["id"]
    chat_frame = f'event: chat\ndata: {{"room_id": "{room_id}"}}\n\n'

    def notify(payload):
        pg_notify(owner, "ovg_chat", payload)

    streams = [Frames(client, path, mine) for client in (pair.oracle, pair.rust)]
    try:
        same_headers("E11 chat events stream headers", streams)
        same_frame("E11 chat events ready frame", streams, READY)
        notify({"k": "chat", "o": org, "r": room_id})
        same_frame("E11 chat events member room", streams, chat_frame)
        # Ignored: a room this member cannot read, malformed and foreign notices.
        notify({"k": "chat", "o": org, "r": str(uuid4())})
        notify("not json")
        notify({"k": "chat", "o": fixture["org_b_id"], "r": room_id})
        notify({"k": "contacts", "u": [str(uuid4()), str(uuid4())]})
        notify({"k": "contacts", "u": [str(uuid4()), user]})
        same_frame("E11 chat events contact change naming this member", streams,
                   "event: contacts\ndata: {}\n\n")
        posted = pair.oracle.post(f"{rooms}/{room_id}/messages", headers=csrf_headers,
                                  json={"body": "E11 parity message"})
        require(posted.status_code == 201, f"fixture chat message failed: {posted.text}")
        same_frame("E11 chat events committed message", streams, chat_frame)
        with psycopg.connect(owner) as conn:
            conn.execute(
                "UPDATE org.organization_memberships SET status='revoked', revoked_at=now() "
                "WHERE id=%s",
                (membership,),
            )
        notify({"k": "contacts", "u": [user]})
        same_frame("E11 chat events revoked membership", streams, "event: revoked\ndata: {}\n\n")
        same_frame("E11 chat events stream closes after revoke", streams, "<closed>")
    finally:
        for stream in streams:
            stream.close()


@parity.case
def same_headers(label, streams):
    for name in SSE_HEADERS:
        left, right = (stream.response.headers.get(name) for stream in streams)
        require(left == right, f"{name}: FastAPI {left!r}, Rust {right!r}")
    require(all(stream.response.status_code == 200 for stream in streams), "stream status")
    print(f"PASS {label}", flush=True)


def compare_stream(streams, pair, owner, fixture, csrf_headers):
    org, project, me = fixture["org_a_id"], fixture["project_a_id"], fixture["user_a_id"]
    same_headers("E11 events stream headers", streams)
    same_frame("E11 events ready frame", streams, READY)

    def notify(payload):
        pg_notify(owner, CHANNEL, payload)

    public = {"o": org, "p": project, "a": None, "b": None}
    notify(public | {"kind": "spatial_change"})
    same_frame("E11 events spatial_change", streams, "event: spatial_change\ndata: {}\n\n")
    notify(public | {"kind": "lock_change"})
    same_frame("E11 events lock_change", streams, "event: lock_change\ndata: {}\n\n")
    # Ignored: malformed, other project, private to other users. Then a public chat change.
    notify("not json")
    notify(public | {"p": str(uuid4()), "kind": "spatial_change"})
    notify(public | {"a": str(uuid4()), "b": str(uuid4()), "kind": "spatial_change"})
    notify(public | {"kind": "chat"})
    same_frame("E11 events ignores foreign/private, delivers change", streams,
               "event: change\ndata: {}\n\n")
    notify(public | {"a": str(uuid4()), "b": me})
    same_frame("E11 events private receipt for this actor", streams,
               "event: change\ndata: {}\n\n")
    same_frame("E11 events heartbeat after 15 s idle", streams, ": heartbeat\n\n", timeout=25)

    (_receipt,) = commit_changes(pair.oracle, owner, fixture, csrf_headers, 1)
    same_frame("E11 events committed edit delivers spatial_change", streams,
               "event: spatial_change\ndata: {}\n\n")

    with psycopg.connect(owner) as conn:
        conn.execute(
            "UPDATE identity.web_sessions SET revoked_at=now() WHERE id=%s",
            (session_id(fixture["session_a_token"]),),
        )
    notify(public | {"kind": "chat"})
    same_frame("E11 events revoked session", streams, "event: revoked\ndata: {}\n\n")
    same_frame("E11 events stream closes after revoke", streams, "<closed>")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-rt-") as directory,
        database(admin) as (owner, runtime),
    ):
        work = Path(directory)
        configure_python(owner, runtime, work)
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner, seed=20261126, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
        )
        csrf_headers = session_headers(
            fixture["session_a_token"], **{"x-openvietgis-csrf": fixture["csrf_a_token"]}
        )
        with fastapi_server(work) as oracle, rust_client(binary, runtime, work) as rust:
            pair = Pair(oracle, rust)
            user, membership, scoped_token = scoped_member(owner, fixture)
            setup_gis_scope(
                oracle,
                fixture,
                project_path(fixture["org_a_id"], fixture["project_a_id"]),
                fixture["layers"]["epsg4326_1k"]["layer_id"],
                membership,
                user,
            )
            seed_activities(oracle, owner, fixture, csrf_headers)
            compare_activities(pair, fixture, scoped_token)
            bearers = service_bearers(oracle, fixture, csrf_headers)
            compare_service_activities(pair, fixture, bearers)
            compare_events(
                pair, owner, fixture, (user, membership, scoped_token), csrf_headers, bearers
            )
    print("E11 realtime parity finished", flush=True)


if __name__ == "__main__":
    main()
