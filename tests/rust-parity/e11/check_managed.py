"""E11 managed QGIS sync (R029) differential parity: sessions, bootstrap, durable delta
replay, commit receipts, desktop commands and the managed SSE stream (FastAPI oracle)."""

import argparse
import os
import sys
import time
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import uuid4

import psycopg

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import run as parity
from check_realtime import (
    Frames,
    Pair,
    fastapi_server,
    pg_notify,
    same_frame,
    same_headers,
    service_bearers,
    validation_shape,
)
from run import (
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

SESSION_TIMES = ("id", "expires_at", "last_seen_at", "created_at", "revoked_at")
COMMAND_TIMES = ("id", "expires_at", "created_at", "resolved_at")


def blank(*keys):
    def normalize(body):
        return {key: f"<{key}>" if key in keys else value for key, value in body.items()}

    return normalize


@parity.case
def same(label, pair, method, path, status, *, headers, body=None, params=None, normalize=None):
    """One request per server; `normalize` blanks values each write mints on its own."""
    responses = [
        client.request(method, path, headers=headers, json=body, params=params)
        for client in (pair.oracle, pair.rust)
    ]
    for name, response in zip(("FastAPI", "Rust"), responses, strict=True):
        require(
            response.status_code == status,
            f"{name} status {response.status_code}, wanted {status}: {response.text[:300]}",
        )
    try:
        left, right = (response.json() for response in responses)
    except ValueError as error:
        raise AssertionError(f"response is not JSON: {error}") from None
    if normalize is not None:
        left, right = normalize(left), normalize(right)
    mismatch = parity.difference(left, right)
    require(mismatch is None, mismatch)
    print(f"PASS {label}", flush=True)
    return responses[0].json(), responses[1].json()


def revision(owner, source):
    with psycopg.connect(owner) as conn:
        return conn.execute(
            "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (source,)
        ).fetchone()[0]


def commit_body(owner, source, session_id, **extra):
    feature = {"code": "R029", "ward": "W1", "metric_val": 1, "active": True}
    body = {
        "session_id": session_id,
        "source_id": source,
        "base_content_revision": revision(owner, source),
        "operation_id": f"r029-{uuid4()}",
        "operations": [
            {
                "op_type": "create",
                "feature_id": f"r029-{uuid4()}",
                "geometry": {"type": "Point", "coordinates": [105.9, 21.1]},
                "properties": feature,
            }
        ],
    }
    return body | extra


def compare_managed(pair, owner, fixture, csrf, bearers, scoped_token):
    org, project = fixture["org_a_id"], fixture["project_a_id"]
    base = f"{project_path(org, project)}/integration-hub/managed"
    human = session_headers(fixture["session_a_token"])
    reader, ungranted = bearers
    source = fixture["layers"]["edit_4326"]["source_id"]
    layer = fixture["layers"]["epsg4326_1k"]["layer_id"]
    sessions = f"{base}/sessions"

    # Sessions: every write mints its own row on each server.
    created = same("R029 sessions create human", pair, "POST", sessions, 201, headers=csrf,
                   body={"device_label": "  QGIS 3.40  ", "client_version": "1.0.0"},
                   normalize=blank(*SESSION_TIMES))
    py_session, rs_session = (body["id"] for body in created)
    service = same("R029 sessions create service reader", pair, "POST", sessions, 201,
                   headers=reader, body={}, normalize=blank(*SESSION_TIMES))
    svc_session = service[0]["id"]
    same("R029 sessions create 403 missing CSRF", pair, "POST", sessions, 403,
         headers=human, body={})
    same("R029 sessions create 403 inactive binding", pair, "POST", sessions, 403,
         headers=csrf, body={"binding_id": str(uuid4())})
    same("R029 sessions create 403 service principal without grant", pair, "POST", sessions,
         403, headers=ungranted, body={})
    for name, body in {
        "bad mode": {"mode": "push"},
        "expires_in_hours=0": {"expires_in_hours": 0},
        "blank device label": {"device_label": "   "},
        "unknown field": {"extra": 1},
    }.items():
        same(f"R029 sessions create 422 {name}", pair, "POST", sessions, 422, headers=csrf,
             body=body, normalize=validation_shape)
    same("R029 sessions create 401 anonymous", pair, "POST", sessions, 401, headers={}, body={})

    same("R029 sessions list owner", pair, "GET", sessions, 200, headers=human)
    same("R029 sessions list service reader", pair, "GET", sessions, 200, headers=reader)
    same("R029 sessions list GIS-scoped member", pair, "GET", sessions, 200,
         headers=session_headers(scoped_token))
    same("R029 session get owner", pair, "GET", f"{sessions}/{py_session}", 200, headers=human)
    same("R029 session get manager sees service session", pair, "GET",
         f"{sessions}/{svc_session}", 200, headers=human)
    same("R029 session get 404 other actor", pair, "GET", f"{sessions}/{py_session}", 404,
         headers=reader)
    same("R029 session get 404 unknown", pair, "GET", f"{sessions}/{uuid4()}", 404,
         headers=human)
    same("R029 session get 422 malformed id", pair, "GET", f"{sessions}/not-a-uuid", 422,
         headers=human, normalize=validation_shape)

    # Bootstrap: one pinned snapshot page.
    boot = f"{base}/bootstrap"
    same("R029 bootstrap page", pair, "GET", boot, 200, headers=human,
         params={"layer_id": layer, "session_id": py_session, "limit": 3})
    same("R029 bootstrap second page", pair, "GET", boot, 200, headers=human,
         params={"layer_id": layer, "session_id": py_session, "limit": 3, "offset": 3})
    same("R029 bootstrap 404 unknown layer", pair, "GET", boot, 404, headers=human,
         params={"layer_id": str(uuid4()), "session_id": py_session})
    same("R029 bootstrap 404 another actor's session", pair, "GET", boot, 404, headers=reader,
         params={"layer_id": layer, "session_id": py_session})
    for name, params in {
        "missing session": {"layer_id": layer},
        "limit=0": {"layer_id": layer, "session_id": py_session, "limit": 0},
        "offset=-1": {"layer_id": layer, "session_id": py_session, "offset": -1},
    }.items():
        same(f"R029 bootstrap 422 {name}", pair, "GET", boot, 422, headers=human,
             params=params, normalize=validation_shape)

    # Commits: each server replays the other's receipt idempotently.
    commits = f"{base}/commits"
    first = commit_body(owner, source, py_session)
    oracle_receipt = pair.oracle.post(commits, headers=csrf, json=first)
    require(oracle_receipt.status_code == 201, f"oracle managed commit: {oracle_receipt.text}")
    same("R029 commit replay of FastAPI receipt", pair, "POST", commits, 201, headers=csrf,
         body=first)
    second = commit_body(owner, source, rs_session)
    rust_receipt = pair.rust.post(commits, headers=csrf, json=second)
    require(rust_receipt.status_code == 201, f"Rust managed commit: {rust_receipt.text}")
    require(rust_receipt.json()["event_cursor"] > oracle_receipt.json()["event_cursor"],
            "Rust commit did not advance the durable cursor")
    same("R029 commit replay of Rust receipt", pair, "POST", commits, 201, headers=csrf,
         body=second)
    for operation_id in (first["operation_id"], second["operation_id"]):
        same(f"R029 receipt {operation_id[:9]}", pair, "GET", f"{commits}/{operation_id}", 200,
             headers=human)
    same("R029 receipt 404 another actor", pair, "GET", f"{commits}/{first['operation_id']}",
         404, headers=reader)
    same("R029 receipt 404 unknown", pair, "GET", f"{commits}/missing-op", 404, headers=human)
    delete_op = commit_body(owner, source, py_session)
    delete_op["operations"][0]["op_type"] = "delete"
    same("R029 commit 403 deletes need allow_deletes", pair, "POST", commits, 403,
         headers=csrf, body=delete_op)
    same("R029 commit 422 reserved operation prefix", pair, "POST", commits, 422, headers=csrf,
         body=commit_body(owner, source, py_session, operation_id="field-sync-1"))
    same("R029 commit 409 stale revision", pair, "POST", commits, 409, headers=csrf,
         body=commit_body(owner, source, py_session, base_content_revision="0" * 64))
    same("R029 commit 403 service reader lacks project.edit", pair, "POST", commits, 403,
         headers=reader, body=commit_body(owner, source, svc_session))
    for name, extra in {
        "short revision": {"base_content_revision": "abc"},
        "no operations": {"operations": []},
        "blank operation id": {"operation_id": "  "},
    }.items():
        same(f"R029 commit 422 {name}", pair, "POST", commits, 422, headers=csrf,
             body=commit_body(owner, source, py_session, **extra), normalize=validation_shape)

    # Durable delta replay from before both commits.
    delta = f"{base}/delta"
    before = oracle_receipt.json()["event_cursor"] - 1
    same("R029 delta replays both commits", pair, "GET", delta, 200, headers=human,
         params={"session_id": py_session, "cursor": before})
    same("R029 delta paged limit=1", pair, "GET", delta, 200, headers=human,
         params={"session_id": py_session, "cursor": before, "limit": 1})
    latest = rust_receipt.json()["event_cursor"]
    same("R029 delta quiet poll", pair, "GET", delta, 200, headers=human,
         params={"session_id": py_session, "cursor": latest})
    same("R029 delta reset beyond latest cursor", pair, "GET", delta, 200, headers=human,
         params={"session_id": py_session, "cursor": latest + 10})
    same("R029 delta 422 negative cursor", pair, "GET", delta, 422, headers=human,
         params={"session_id": py_session, "cursor": -1}, normalize=validation_shape)
    same("R029 delta 422 limit=501", pair, "GET", delta, 422, headers=human,
         params={"session_id": py_session, "limit": 501}, normalize=validation_shape)

    # Desktop command bridge.
    commands = f"{base}/desktop-sessions/{py_session}/commands"
    queued = same("R029 command enqueue", pair, "POST", commands, 201, headers=csrf,
                  body={"tool_name": " zoom_to_extent ", "arguments": {"bbox": [1, 2, 3, 4]}},
                  normalize=blank(*COMMAND_TIMES))
    same("R029 command 403 tool outside allowlist", pair, "POST", commands, 403,
         headers=csrf, body={"tool_name": "run_python"})
    same("R029 command 422 ttl below 10", pair, "POST", commands, 422, headers=csrf,
         body={"tool_name": "inspect_project", "ttl_seconds": 5}, normalize=validation_shape)
    same("R029 command 403 stage_edits without project.edit", pair, "POST",
         f"{base}/desktop-sessions/{svc_session}/commands", 403, headers=reader,
         body={"tool_name": "stage_edits"})
    same("R029 commands list", pair, "GET", commands, 200, headers=human)
    same("R029 commands list status=pending", pair, "GET", commands, 200, headers=human,
         params={"status": "pending"})
    # Each server resolves the command the other one queued.
    py_command, rs_command = (body["id"] for body in queued)
    responses = []
    for client, command in ((pair.rust, py_command), (pair.oracle, rs_command)):
        responses.append(client.post(f"{commands}/{command}/respond", headers=csrf,
                                     json={"status": "completed", "result": {"ok": True}}))
    respond_same(responses)
    same("R029 command respond 409 already resolved", pair, "POST",
         f"{commands}/{py_command}/respond", 409, headers=csrf, body={"status": "failed"})
    same("R029 command respond 422 bad status", pair, "POST",
         f"{commands}/{py_command}/respond", 422, headers=csrf, body={"status": "done"},
         normalize=validation_shape)

    compare_managed_events(pair, owner, fixture, base, py_session, csrf, human, reader)

    # Revocation: a non-manager cannot revoke; each server revokes its own session.
    same("R029 session revoke 403 not owner nor manager", pair, "DELETE",
         f"{sessions}/{rs_session}", 403, headers=reader)
    same("R029 session revoke 422 bad bump_epoch", pair, "DELETE",
         f"{sessions}/{rs_session}", 422, headers=csrf, params={"bump_epoch": "maybe"},
         normalize=validation_shape)
    revoked = [
        client.delete(f"{sessions}/{session}", headers=csrf)
        for client, session in ((pair.oracle, svc_session), (pair.rust, rs_session))
    ]
    revoke_same(revoked)
    same("R029 delta 403 revoked session", pair, "GET", delta, 403, headers=human,
         params={"session_id": rs_session})
    fresh = same("R029 sessions create before epoch bump", pair, "POST", sessions, 201,
                 headers=csrf, body={}, normalize=blank(*SESSION_TIMES))
    bumped = pair.rust.delete(f"{sessions}/{fresh[0]['id']}", headers=csrf,
                              params={"bump_epoch": "true"})
    require(bumped.status_code == 200, f"Rust bump_epoch revoke: {bumped.text}")
    same("R029 delta resets after permission epoch bump", pair, "GET", delta, 200,
         headers=human, params={"session_id": fresh[1]["id"], "cursor": 0})
    same("R029 bootstrap 403 stale permission epoch", pair, "GET", boot, 403, headers=human,
         params={"layer_id": layer, "session_id": fresh[1]["id"]})


@parity.case
def respond_same_case(label, responses):
    for response in responses:
        require(response.status_code == 200, f"status {response.status_code}: {response.text}")
    left, right = (blank(*COMMAND_TIMES)(response.json()) for response in responses)
    mismatch = parity.difference(left, right)
    require(mismatch is None, mismatch)
    print(f"PASS {label}", flush=True)


def respond_same(responses):
    respond_same_case("R029 command respond on the other server", responses)


@parity.case
def revoke_same_case(label, responses):
    for response in responses:
        require(response.status_code == 200, f"status {response.status_code}: {response.text}")
    for response in responses:
        body = response.json()
        require(body["state"] == "revoked" and body["revoked_at"], f"not revoked: {body}")
    print(f"PASS {label}", flush=True)


def revoke_same(responses):
    revoke_same_case("R029 session revoke by owner/manager", responses)


def compare_managed_events(pair, owner, fixture, base, session_id, csrf, human, reader):
    path = f"{base}/events"
    org, project = fixture["org_a_id"], fixture["project_a_id"]
    same("R029 events 422 missing session", pair, "GET", path, 422, headers=human,
         normalize=validation_shape)
    same("R029 events 404 another actor's session", pair, "GET", path, 404, headers=reader,
         params={"session_id": session_id})
    same("R029 events 401 anonymous", pair, "GET", path, 401, headers={},
         params={"session_id": session_id})
    url = f"{path}?session_id={session_id}"
    streams = [Frames(client, url, human) for client in (pair.oracle, pair.rust)]

    def notify(kind):
        pg_notify(owner, "ovg_team_collaboration",
                  {"o": org, "p": project, "a": None, "b": None, "kind": kind})

    try:
        same_headers("R029 events stream headers", streams)
        same_frame("R029 events retry frame", streams, "retry: 3000\n\n")
        # Both servers send `retry` before LISTEN is live; let the listeners attach.
        time.sleep(1)
        notify("spatial_change")
        same_frame("R029 events spatial_change", streams,
                   'event: spatial_change\ndata: {"kind": "spatial_change"}\n\n')
        # Another session's revoke notice and other kinds are rechecked but not sent.
        notify("session_revoked")
        notify("lock_change")
        notify("desktop_command")
        same_frame("R029 events desktop_command", streams,
                   'event: desktop_command\ndata: {"kind": "desktop_command"}\n\n')
        same_frame("R029 events heartbeat", streams, 'event: heartbeat\ndata: {"ok":true}\n\n',
                   timeout=30)
        revoked = pair.oracle.delete(f"{base}/sessions/{session_id}", headers=csrf)
        require(revoked.status_code == 200, f"revoke for stream: {revoked.text}")
        same_frame("R029 events session revoked", streams,
                   'event: session_revoked\ndata: {"revoked":true}\n\n')
        same_frame("R029 events stream closes after revoke", streams, "<closed>")
    finally:
        for stream in streams:
            stream.close()
    same("R029 events 403 revoked session", pair, "GET", path, 403, headers=human,
         params={"session_id": session_id})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-managed-") as directory,
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
            bearers = service_bearers(oracle, fixture, csrf)
            compare_managed(pair, owner, fixture, csrf, bearers, scoped_token)
    print("E11 managed sync parity finished", flush=True)


if __name__ == "__main__":
    main()
