from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_team_chat_and_task_isolation(migrated_database):
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"
    base = f"{project}/collaboration"
    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as member,
        _client(world.wrong_user_id) as outsider,
    ):

        def check(r, status=200):
            assert r.status_code == status, r.text
            return r.json()

        assert check(owner.get(base + "/messages"))["items"] == []
        assert check(owner.get(base + "/tasks"))["items"] == []
        assert outsider.get(base + "/messages").status_code in (403, 404)
        assert outsider.post(base + "/messages", json={"body": "unauthorized"}).status_code in (
            403,
            404,
        )
        assert outsider.get(base + "/tasks").status_code in (403, 404)
        assert owner.post(base + "/messages", json={"body": "   "}).status_code == 422
        assert (
            owner.post(
                base + "/tasks", json={"title": "invalid", "assignee_id": str(world.wrong_user_id)}
            ).status_code
            == 422
        )

        check(
            owner.post(
                org + "/grants",
                json={
                    "subject_kind": "membership",
                    "subject_id": str(world.member_membership_id),
                    "scope_kind": "project",
                    "project_id": str(world.project_id),
                    "role": "editor",
                },
            ),
            201,
        )
        assert {m["id"] for m in check(member.get(base + "/members"))["items"]} == {
            str(world.owner_user_id),
            str(world.member_user_id),
        }
        sent = check(
            owner.post(base + "/messages", json={"body": "Kiểm tra ranh giới <script>"}), 201
        )
        assert sent["author_id"] == str(world.owner_user_id)
        assert check(member.get(base + "/messages"))["items"] == [sent]
        message = check(member.post(base + "/messages", json={"body": "Đã nhận!"}), 201)
        assert check(owner.get(base + "/messages"))["items"][-1] == message
        payload = {
            "title": "Kiểm tra lô 4",
            "assignee_id": str(world.member_user_id),
            "priority": "high",
            "due_date": "2026-12-01",
            "longitude": 106.6,
            "latitude": 10.7,
        }
        assert owner.post(base + "/tasks", json=payload | {"latitude": None}).status_code == 422
        task = check(owner.post(base + "/tasks", json=payload), 201)
        task_url = base + "/tasks/" + task["id"]
        assert task["status"] == "todo" and task["revision"] == 1
        assert check(member.get(base + "/tasks", params={"mine": "true"}))["items"] == [task]
        assert check(owner.get(base + "/tasks", params={"mine": "true"}))["items"] == []
        assert outsider.patch(task_url, json={"status": "done", "revision": 1}).status_code in (
            403,
            404,
        )
        changed = check(member.patch(task_url, json={"status": "in_progress", "revision": 1}))
        assert changed["revision"] == 2
        assert owner.patch(task_url, json={"status": "done", "revision": 1}).status_code == 409
        assert (
            check(owner.patch(task_url, json={"status": "done", "revision": 2}))["status"] == "done"
        )
        self_task = check(
            owner.post(
                base + "/tasks", json={"title": "My task", "assignee_id": str(world.owner_user_id)}
            ),
            201,
        )
        assert (
            member.patch(
                base + "/tasks/" + self_task["id"], json={"revision": 1, "status": "done"}
            ).status_code
            == 403
        )
        old_csrf = owner.headers.pop("X-OpenVietGIS-CSRF")
        assert owner.post(base + "/messages", json={"body": "invalid csrf"}).status_code == 403
        owner.headers["X-OpenVietGIS-CSRF"] = old_csrf

        layer = check(
            owner.post(
                project + "/layers/manual",
                json={
                    "operation_id": str(uuid4()),
                    "name": "Scope test",
                    "geometry_family": "Point",
                    "fields": [],
                },
            ),
            201,
        )
        check(
            owner.post(
                project + "/access/rules",
                json={
                    "user_id": str(world.member_user_id),
                    "layer_id": layer["layer_id"],
                    "actions": ["read"],
                },
            ),
            201,
        )
        assert member.get(base + "/messages").status_code == 403
        assert member.get(base + "/tasks").status_code == 403
        assert str(world.member_user_id) not in {
            m["id"] for m in check(owner.get(base + "/members"))["items"]
        }
        assert owner.post(base + "/tasks", json=payload).status_code == 422
