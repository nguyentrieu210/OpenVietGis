"""Run via tools.run_with_test_database against a dedicated test database."""

import os
import queue
import threading
import time

import httpx
import uvicorn
from openvietgis_api.main import create_app
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_org_chat import add_member
from tests.integration.spatial.test_sources import _client

from tools.test_database import assert_safe_test_target

assert os.environ.get("OPENVIETGIS_ENVIRONMENT") == "test"
assert_safe_test_target(os.environ["OPENVIETGIS_TEST_DATABASE_URL"])
assert (
    os.environ["OPENVIETGIS_DATABASE_URL"]
    == os.environ["OPENVIETGIS_TEST_DATABASE_URL"]
)
world = seed_access_world()
third = add_member(world.organization_id, "SSE observer")
base = f"/api/v1/organizations/{world.organization_id}/chat"
server = uvicorn.Server(
    uvicorn.Config(create_app(), host="127.0.0.1", port=5197, log_level="warning")
)
thread = threading.Thread(target=server.run, daemon=True)
thread.start()
for i in range(100):
    if server.started:
        break
    time.sleep(0.05)
assert server.started
origin = "http://127.0.0.1:5197"


def http(user):
    with _client(user) as c:
        return httpx.Client(
            base_url=origin,
            cookies={"ovg_session": c.cookies.get("ovg_session")},
            headers={"X-OpenVietGIS-CSRF": c.headers["X-OpenVietGIS-CSRF"]},
            timeout=8,
        )


def check(r):
    assert r.is_success, (r.status_code, r.text)
    return r.json()


def listen(c, out):
    try:
        with c.stream("GET", base + "/events", timeout=12) as r:
            assert r.status_code == 200, r.status_code
            event = None
            for line in r.iter_lines():
                if line.startswith("event: "):
                    event = line[7:]
                if line.startswith("data: "):
                    out.put((event, line[6:], time.monotonic()))
                if server.should_exit:
                    return
    except (httpx.HTTPError, AssertionError, RuntimeError) as e:
        out.put(("error", str(e), time.monotonic()))


try:
    with (
        http(world.owner_user_id) as a,
        http(world.member_user_id) as b,
        http(third) as outsider,
    ):
        check(a.get(base + "/rooms"))
        check(b.get(base + "/rooms"))
        check(outsider.get(base + "/rooms"))
        room = check(
            a.post(base + "/rooms/direct", json={"user_id": str(world.member_user_id)})
        )["id"]
        assert outsider.get(base + f"/rooms/{room}/messages").status_code == 404
        qb, qo = queue.Queue(), queue.Queue()
        tb = threading.Thread(target=listen, args=(b, qb), daemon=True)
        to = threading.Thread(target=listen, args=(outsider, qo), daemon=True)
        tb.start()
        to.start()
        assert qb.get(timeout=6)[0] == "ready"
        assert qo.get(timeout=6)[0] == "ready"
        start = time.monotonic()
        check(
            a.post(
                base + f"/rooms/{room}/messages",
                json={"body": "isolated realtime test"},
            )
        )
        event, data, end = qb.get(timeout=6)
        assert event == "chat" and room in data, (event, data)
        assert (
            check(b.get(base + f"/rooms/{room}/messages"))["items"][-1]["body"]
            == "isolated realtime test"
        )
        try:
            unexpected = qo.get(timeout=2)
            assert False, unexpected
        except queue.Empty:
            pass
        print(
            f"PASS real HTTP SSE: recipient received room event in {(end - start) * 1000:.0f} ms; nonmember received no event in 2s"
        )
finally:
    server.should_exit = True
