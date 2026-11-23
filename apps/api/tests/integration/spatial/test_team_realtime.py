"""Real Postgres and network SSE tests for collaboration V2."""

import socket
import threading
import time
from uuid import uuid4

import httpx
import uvicorn
from sqlalchemy import text

from openvietgis_api.adapters.http.team_realtime import matches
from openvietgis_api.main import create_app
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_private_messages_conversion_and_notification(migrated_database):
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    base = f"{org}/projects/{world.project_id}/collaboration"
    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as member,
        _client(world.wrong_user_id) as third,
    ):

        def check(r, status=200):
            assert r.status_code == status, r.text
            return r.json()

        def grant(membership_id):
            check(
                owner.post(
                    org + "/grants",
                    json={
                        "subject_kind": "membership",
                        "subject_id": str(membership_id),
                        "scope_kind": "project",
                        "project_id": str(world.project_id),
                        "role": "editor",
                    },
                ),
                201,
            )

        grant(world.member_membership_id)
        third_membership = uuid4()
        with get_admin_engine().begin() as db:
            db.execute(
                text("""INSERT INTO
                  org.organization_memberships(id,organization_id,user_id,role,status,
                  created_at,activated_at)
                VALUES(:id,:org,:uid,'member','active',now(),now())"""),
                {"id": third_membership, "org": world.organization_id, "uid": world.wrong_user_id},
            )
        grant(third_membership)
        public = check(owner.post(base + "/messages", json={"body": "Check road"}), 201)
        pm = check(
            member.post(
                base + "/messages",
                json={"body": "Private details", "recipient_id": str(world.wrong_user_id)},
            ),
            201,
        )
        assert check(third.get(base + "/messages", params={"peer": str(world.member_user_id)}))[
            "items"
        ] == [pm]
        assert check(member.get(base + "/messages", params={"peer": str(world.wrong_user_id)}))[
            "items"
        ] == [pm]
        assert check(owner.get(base + "/messages"))["items"] == [public]
        assert (
            check(owner.get(base + "/messages", params={"peer": str(world.member_user_id)}))[
                "items"
            ]
            == []
        )
        assert (
            check(owner.get(base + "/messages", params={"peer": str(world.wrong_user_id)}))["items"]
            == []
        )
        assert (
            member.post(
                base + "/messages", json={"body": "self", "recipient_id": str(world.member_user_id)}
            ).status_code
            == 422
        )
        payload = {
            "title": "Check road",
            "assignee_id": str(world.wrong_user_id),
            "source_message_id": pm["id"],
        }
        assert member.post(base + "/tasks", json=payload).status_code == 422
        task = check(
            owner.post(base + "/tasks", json=payload | {"source_message_id": public["id"]}), 201
        )
        assert task["source_message_id"] == public["id"]
        notices = check(third.get(base + "/notifications"))
        assert notices["unread"] == 1 and notices["items"][0]["id"] == task["id"]
        assert check(owner.get(base + "/notifications"))["unread"] == 0
        assert member.post(base + "/notifications/" + task["id"] + "/read").status_code == 404
        check(third.post(base + "/notifications/" + task["id"] + "/read"))
        check(third.post(base + "/notifications/" + task["id"] + "/read"))
        assert check(third.get(base + "/notifications"))["unread"] == 0
        assert check(third.get(base + "/notifications"))["items"][0]["read_at"] is not None


def test_sse_pushes_committed_changes(migrated_database):
    world = seed_access_world()
    base = (
        f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}/collaboration"
    )
    server = uvicorn.Server(
        uvicorn.Config(
            create_app(), host="127.0.0.1", port=0, log_level="warning", access_log=False
        )
    )
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    thread = threading.Thread(target=server.run, kwargs={"sockets": [sock]}, daemon=True)
    thread.start()
    try:
        deadline = time.monotonic() + 10
        while not server.started and time.monotonic() < deadline:
            time.sleep(0.025)
        assert server.started
        with _client(world.owner_user_id) as owner:
            with httpx.Client(cookies=dict(owner.cookies), timeout=6) as network:
                with network.stream(
                    "GET", f"http://127.0.0.1:{port}" + base + "/events"
                ) as response:
                    assert response.status_code == 200, response.read()
                    events = (line for line in response.iter_lines() if line.startswith("event: "))
                    assert next(events) == "event: ready"
                    result = owner.post(base + "/messages", json={"body": "SSE immediate"})
                    assert result.status_code == 201, result.text
                    start = time.monotonic()  # measure after the REST mutation/commit
                    assert next(events) == "event: change"
                    assert time.monotonic() - start < 4
                    result = owner.post(
                        base + "/tasks",
                        json={"title": "SSE task", "assignee_id": str(world.owner_user_id)},
                    )
                    assert result.status_code == 201, result.text
                    start = time.monotonic()  # measure after the REST mutation/commit
                    assert next(events) == "event: change"
                    assert time.monotonic() - start < 4
                    root = base.removesuffix("/collaboration")
                    layer = owner.post(
                        root + "/layers/manual",
                        json={
                            "operation_id": str(uuid4()),
                            "name": "SSE spatial",
                            "geometry_family": "Point",
                            "fields": [
                                {
                                    "field_id": "nm",
                                    "name": "Name",
                                    "type": "text",
                                    "nullable": False,
                                }
                            ],
                        },
                    )
                    assert layer.status_code == 201, layer.text
                    layer = layer.json()
                    src = owner.get(root + "/layers/" + layer["layer_id"]).json()["source"]
                    result = owner.post(
                        root + "/sources/" + layer["source_id"] + "/changesets",
                        json={
                            "operation_id": str(uuid4()),
                            "base_content_revision": src["content_revision"],
                            "base_schema_revision": src["schema_revision"],
                            "summary": "Live spatial change",
                            "operations": [
                                {
                                    "op_type": "create",
                                    "feature_id": "A",
                                    "geometry": {"type": "Point", "coordinates": [106.5, 10.7]},
                                    "properties": {"nm": "A"},
                                }
                            ],
                        },
                    )
                    assert result.status_code == 201, result.text
                    assert next(events) == "event: spatial_change"
                    result = owner.post(
                        base + "/comments",
                        json={
                            "source_id": layer["source_id"],
                            "feature_id": "A",
                            "body": "SSE comment",
                        },
                    )
                    assert result.status_code == 201, result.text
                    assert next(events) == "event: change"
                    lease = {
                        "source_id": layer["source_id"],
                        "feature_id": "A",
                        "token": str(uuid4()),
                    }
                    result = owner.post(base + "/edit-locks", json=lease)
                    assert result.status_code == 200, result.text
                    assert next(events) == "event: lock_change"
                    result = owner.post(base + "/edit-locks/release", json=lease)
                    assert result.status_code == 200, result.text
                    assert next(events) == "event: lock_change"
        assert not matches(
            '{"o":"x","p":"y","a":null,"b":null}',
            world.organization_id,
            world.project_id,
            world.owner_user_id,
        )
    finally:
        server.should_exit = True
        thread.join(timeout=5)
        sock.close()
