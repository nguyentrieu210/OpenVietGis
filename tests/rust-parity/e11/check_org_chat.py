"""E11 organization chat REST differential parity (FastAPI oracle): rooms, messages,
image attachments, reactions, pins, people search and contacts."""

import argparse
import os
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import uuid4

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import run as parity
from check_managed import blank, same
from check_realtime import Pair, fastapi_server, service_bearers, validation_shape
from check_team import editor_member
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

PNG = (
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk"
    "YPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="
)


def message_minted(body):
    out = blank("id", "created_at")(body)
    out["attachments"] = [blank("id")(item) for item in body.get("attachments", [])]
    return out


@parity.case
def same_bodies(label, responses, status, normalize=None):
    for response in responses:
        require(response.status_code == status,
                f"status {response.status_code}, wanted {status}: {response.text[:300]}")
    if status == 204:
        require(all(not response.content for response in responses), "204 with a body")
    else:
        left, right = (response.json() for response in responses)
        if normalize is not None:
            left, right = normalize(left), normalize(right)
        mismatch = parity.difference(left, right)
        require(mismatch is None, mismatch)
    print(f"PASS {label}", flush=True)


@parity.case
def same_bytes(label, pair, path, headers):
    responses = [client.get(path, headers=headers) for client in (pair.oracle, pair.rust)]
    for response in responses:
        require(response.status_code == 200, f"status {response.status_code}")
    left, right = responses
    require(left.content == right.content, "attachment bytes differ")
    for name in ("content-type", "cache-control", "x-content-type-options",
                 "content-disposition"):
        require(left.headers.get(name) == right.headers.get(name),
                f"{name}: {left.headers.get(name)!r} != {right.headers.get(name)!r}")
    print(f"PASS {label}", flush=True)


def compare_rooms(pair, fixture, a, csrf, editor, editor2, scoped_id, bearers):
    org = f"/api/v1/organizations/{fixture['org_a_id']}/chat"
    editor_id, editor_read, editor_csrf = editor
    editor2_id, _, _ = editor2
    rooms = f"{org}/rooms"
    same("ORG-CHAT rooms (lazy organization room)", pair, "GET", rooms, 200, headers=a)
    same("ORG-CHAT rooms 403 service principal", pair, "GET", rooms, 403, headers=bearers[0])
    same("ORG-CHAT rooms 401 anonymous", pair, "GET", rooms, 401, headers={})
    same("ORG-CHAT rooms 404 foreign organization", pair, "GET",
         f"/api/v1/organizations/{fixture['org_b_id']}/chat/rooms", 404, headers=a)
    same("ORG-CHAT rooms 422 malformed organization", pair, "GET",
         "/api/v1/organizations/nope/chat/rooms", 422, headers=a, normalize=validation_shape)

    group = {"name": "  Tổ đo đạc  ", "member_ids": [editor_id, editor2_id, editor_id]}
    created = same("ORG-CHAT room create", pair, "POST", rooms, 201, headers=csrf, body=group,
                   normalize=blank("id"))
    oracle_room, rust_room = (body["id"] for body in created)
    for name, body in {
        "blank name": {"name": "  ", "member_ids": [editor_id]},
        "no members": {"name": "x", "member_ids": []},
    }.items():
        same(f"ORG-CHAT room create 422 {name}", pair, "POST", rooms, 422, headers=csrf,
             body=body, normalize=validation_shape)
    same("ORG-CHAT room create 422 only self", pair, "POST", rooms, 422, headers=csrf,
         body={"name": "x", "member_ids": [fixture["user_a_id"]]})
    same("ORG-CHAT room create 422 unknown member", pair, "POST", rooms, 422, headers=csrf,
         body={"name": "x", "member_ids": [str(uuid4())]})
    same("ORG-CHAT room create 403 missing CSRF", pair, "POST", rooms, 403, headers=a,
         body=group)

    direct = f"{rooms}/direct"
    same("ORG-CHAT direct room is idempotent", pair, "POST", direct, 200, headers=csrf,
         body={"user_id": editor_id})
    same("ORG-CHAT direct room 404 self", pair, "POST", direct, 404, headers=csrf,
         body={"user_id": fixture["user_a_id"]})
    same("ORG-CHAT direct room 404 unknown person", pair, "POST", direct, 404, headers=csrf,
         body={"user_id": str(uuid4())})
    same("ORG-CHAT rooms after creation", pair, "GET", rooms, 200, headers=a)
    same("ORG-CHAT rooms seen by a member", pair, "GET", rooms, 200, headers=editor_read)

    org_room = next(r for r in pair.oracle.get(rooms, headers=a).json()["items"]
                    if r["kind"] == "org")["id"]
    same("ORG-CHAT rename group", pair, "PATCH", f"{rooms}/{oracle_room}", 200, headers=csrf,
         body={"name": "Tổ bản đồ"})
    same("ORG-CHAT rename 422 organization room", pair, "PATCH", f"{rooms}/{org_room}", 422,
         headers=csrf, body={"name": "x"})
    same("ORG-CHAT rename 403 not room admin", pair, "PATCH", f"{rooms}/{oracle_room}", 403,
         headers=editor_csrf, body={"name": "x"})
    same("ORG-CHAT rename 404 unknown room", pair, "PATCH", f"{rooms}/{uuid4()}", 404,
         headers=csrf, body={"name": "x"})
    members = f"{rooms}/{oracle_room}/members"
    same("ORG-CHAT invite is idempotent", pair, "POST", members, 200, headers=csrf,
         body={"user_ids": [scoped_id]})
    same("ORG-CHAT invite 422 unknown person", pair, "POST", members, 422, headers=csrf,
         body={"user_ids": [str(uuid4())]})
    same("ORG-CHAT invite 422 organization room", pair, "POST",
         f"{rooms}/{org_room}/members", 422, headers=csrf, body={"user_ids": [scoped_id]})

    removed = [
        client.delete(f"{rooms}/{room}/members/{editor2_id}", headers=csrf)
        for client, room in ((pair.oracle, oracle_room), (pair.rust, rust_room))
    ]
    same_bodies("ORG-CHAT remove member", removed, 204)
    same("ORG-CHAT remove member 404 already removed", pair, "DELETE",
         f"{rooms}/{oracle_room}/members/{editor2_id}", 404, headers=csrf)
    same("ORG-CHAT remove member 403 not room admin", pair, "DELETE",
         f"{rooms}/{oracle_room}/members/{fixture['user_a_id']}", 403, headers=editor_csrf)
    return oracle_room, rust_room, org_room


def compare_messages(pair, fixture, a, csrf, editor, rooms_):
    org = f"/api/v1/organizations/{fixture['org_a_id']}/chat"
    editor_id, editor_read, editor_csrf = editor
    oracle_room, rust_room, org_room = rooms_
    messages = f"{org}/rooms/{oracle_room}/messages"
    seed = pair.oracle.post(messages, headers=csrf, json={"body": "Mở đầu"})
    require(seed.status_code == 201, f"seed chat message: {seed.text}")
    seed_id = seed.json()["id"]
    same("ORG-CHAT message with mention and reply", pair, "POST", messages, 201, headers=csrf,
         body={"body": " Xem @biên ", "mention_ids": [editor_id], "reply_to_id": seed_id},
         normalize=message_minted)
    same("ORG-CHAT message with an image", pair, "POST", messages, 201, headers=csrf,
         body={"attachments": [{"kind": "image", "data_url": PNG, "width": 1, "height": 1}]},
         normalize=message_minted)
    bad_image = {"kind": "image", "width": 1, "height": 1}
    for name, url in {
        "not a data URL": "https://example.test/a.png",
        "bad base64 padding": "data:image/png;base64,iVBORw0KGgo",
        "content not PNG": "data:image/png;base64,/9j/4AAQSkZJRgABAQ==",
    }.items():
        same(f"ORG-CHAT message 422 image {name}", pair, "POST", messages, 422, headers=csrf,
             body={"attachments": [bad_image | {"data_url": url}]})
    same("ORG-CHAT message 422 empty", pair, "POST", messages, 422, headers=csrf,
         body={"body": "   "}, normalize=validation_shape)
    same("ORG-CHAT message 422 mention outside room", pair, "POST", messages, 422,
         headers=csrf, body={"body": "x", "mention_ids": [str(uuid4())]})
    same("ORG-CHAT message 422 reply outside room", pair, "POST", messages, 422,
         headers=csrf, body={"body": "x", "reply_to_id": str(uuid4())})
    same("ORG-CHAT message 404 hidden room", pair, "POST", f"{org}/rooms/{uuid4()}/messages",
         404, headers=csrf, body={"body": "x"})
    same("ORG-CHAT messages", pair, "GET", messages, 200, headers=a)
    same("ORG-CHAT messages seen by a member", pair, "GET", messages, 200, headers=editor_read)
    same("ORG-CHAT messages limit=1", pair, "GET", messages, 200, headers=a,
         params={"limit": 1})
    latest = pair.oracle.get(messages, headers=a, params={"limit": 1}).json()["items"][0]
    same("ORG-CHAT messages before cursor", pair, "GET", messages, 200, headers=a,
         params={"before": latest["id"]})
    same("ORG-CHAT messages 422 limit=0", pair, "GET", messages, 422, headers=a,
         params={"limit": 0}, normalize=validation_shape)
    same("ORG-CHAT rooms show unread and mention", pair, "GET", f"{org}/rooms", 200,
         headers=editor_read)

    image_message = next(m for m in pair.oracle.get(messages, headers=a).json()["items"]
                         if m["attachments"])
    attachment = f"{org}/attachments/{image_message['attachments'][0]['id']}"
    same_bytes("ORG-CHAT attachment bytes and headers", pair, attachment, a)
    same("ORG-CHAT attachment 404 unknown", pair, "GET", f"{org}/attachments/{uuid4()}", 404,
         headers=a)
    same("ORG-CHAT mark read", pair, "POST", f"{org}/rooms/{oracle_room}/read", 200,
         headers=editor_csrf)
    same("ORG-CHAT rooms after read", pair, "GET", f"{org}/rooms", 200, headers=editor_read)

    # Reactions toggle: each server toggles its own fresh message from empty.
    toggled = []
    for client in (pair.oracle, pair.rust):
        fresh = pair.oracle.post(messages, headers=csrf, json={"body": "React"}).json()["id"]
        toggled.append((client, f"{org}/messages/{fresh}/reactions"))
    for step in ("adds", "removes"):
        same_bodies(f"ORG-CHAT reaction toggle {step}", [
            client.post(path, headers=csrf, json={"emoji": " 👍 "}) for client, path in toggled
        ], 200)
    same("ORG-CHAT reaction 422 blank emoji", pair, "POST", toggled[0][1], 422, headers=csrf,
         body={"emoji": "  "}, normalize=validation_shape)
    same("ORG-CHAT reaction 404 unknown message", pair, "POST",
         f"{org}/messages/{uuid4()}/reactions", 404, headers=csrf, body={"emoji": "x"})

    pin = f"{org}/messages/{seed_id}/pin"
    same("ORG-CHAT pin is idempotent", pair, "POST", pin, 200, headers=csrf)
    same("ORG-CHAT pins", pair, "GET", f"{org}/rooms/{oracle_room}/pins", 200, headers=a)
    same("ORG-CHAT unpin 403 not admin nor author", pair, "DELETE", pin, 403,
         headers=editor_csrf)
    same("ORG-CHAT unpin by author", pair, "DELETE", pin, 200, headers=csrf)
    org_message = pair.oracle.post(f"{org}/rooms/{org_room}/messages", headers=csrf,
                                   json={"body": "Toàn tổ chức"}).json()["id"]
    pair.oracle.post(f"{org}/messages/{org_message}/pin", headers=csrf)
    same("ORG-CHAT unpin 403 organization room non-author", pair, "DELETE",
         f"{org}/messages/{org_message}/pin", 403, headers=editor_csrf)
    same("ORG-CHAT organization room pins", pair, "GET", f"{org}/rooms/{org_room}/pins", 200,
         headers=a)


def compare_people(pair, fixture, a, csrf, editor, editor2):
    org = f"/api/v1/organizations/{fixture['org_a_id']}/chat"
    editor_id, editor_read, editor_csrf = editor
    editor2_id, editor2_read, editor2_csrf = editor2
    people = f"{org}/people"
    same("ORG-CHAT people", pair, "GET", people, 200, headers=a)
    same("ORG-CHAT people search folds accents", pair, "GET", people, 200, headers=a,
         params={"q": " bien TAP "})
    same("ORG-CHAT people 422 query too long", pair, "GET", people, 422, headers=a,
         params={"q": "x" * 81}, normalize=validation_shape)
    same("ORG-CHAT avatar", pair, "GET", f"{people}/{editor_id}/avatar", 200, headers=a)
    same("ORG-CHAT avatar 404 unknown person", pair, "GET", f"{people}/{uuid4()}/avatar", 404,
         headers=a)

    requests = f"{org}/contacts/requests"
    same("ORG-CHAT contact request is idempotent", pair, "POST", requests, 201, headers=csrf,
         body={"user_id": editor_id})
    second = pair.rust.post(requests, headers=csrf, json={"user_id": editor2_id})
    require(second.status_code == 201, f"Rust contact request: {second.text}")
    same("ORG-CHAT contact request 404 self", pair, "POST", requests, 404, headers=csrf,
         body={"user_id": fixture["user_a_id"]})
    same("ORG-CHAT contacts with outgoing requests", pair, "GET", f"{org}/contacts", 200,
         headers=a)
    same("ORG-CHAT contacts with incoming request", pair, "GET", f"{org}/contacts", 200,
         headers=editor_read)
    same("ORG-CHAT people show pending requests", pair, "GET", people, 200, headers=a)
    first = pair.oracle.get(f"{org}/contacts", headers=editor_read).json()["incoming"][0]["id"]
    accepted = [
        pair.oracle.post(f"{requests}/{first}/accept", headers=editor_csrf),
        pair.rust.post(f"{requests}/{second.json()['id']}/accept", headers=editor2_csrf),
    ]
    same_bodies("ORG-CHAT contact accept", accepted, 200)
    same("ORG-CHAT contact accept 404 answered", pair, "POST", f"{requests}/{first}/accept",
         404, headers=editor_csrf)
    same("ORG-CHAT contact decline 404 unknown", pair, "POST",
         f"{requests}/{uuid4()}/decline", 404, headers=editor_csrf)
    same("ORG-CHAT contact request after acceptance", pair, "POST", requests, 201,
         headers=csrf, body={"user_id": editor_id})
    same("ORG-CHAT contacts after acceptance", pair, "GET", f"{org}/contacts", 200, headers=a)
    same_bodies("ORG-CHAT contact remove is idempotent", [
        client.delete(f"{org}/contacts/{editor_id}", headers=csrf)
        for client in (pair.oracle, pair.rust)
    ], 204)
    same("ORG-CHAT contacts after removal", pair, "GET", f"{org}/contacts", 200, headers=a)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-chat-") as directory,
        database(admin) as (owner, runtime),
    ):
        work = Path(directory)
        configure_python(owner, runtime, work)
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner, seed=20261126, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
        )
        a = session_headers(fixture["session_a_token"])
        csrf = session_headers(
            fixture["session_a_token"], **{"x-openvietgis-csrf": fixture["csrf_a_token"]}
        )
        with fastapi_server(work) as oracle, rust_client(binary, runtime, work) as rust:
            pair = Pair(oracle, rust)
            user, membership, _ = scoped_member(owner, fixture)
            setup_gis_scope(
                oracle,
                fixture,
                project_path(fixture["org_a_id"], fixture["project_a_id"]),
                fixture["layers"]["epsg4326_1k"]["layer_id"],
                membership,
                user,
            )
            editor = editor_member(owner, oracle, fixture, csrf)
            editor2 = editor_member(owner, oracle, fixture, csrf)
            bearers = service_bearers(oracle, fixture, csrf)
            rooms = compare_rooms(pair, fixture, a, csrf, editor, editor2, user, bearers)
            compare_messages(pair, fixture, a, csrf, editor, rooms)
            compare_people(pair, fixture, a, csrf, editor, editor2)
    print("E11 organization chat parity finished", flush=True)


if __name__ == "__main__":
    main()
