"""WebSocket presence identity, access checks and peer lifecycle."""

import pytest
from sqlalchemy import text
from starlette.websockets import WebSocketDisconnect

from openvietgis_api.adapters.http.team_presence import Peer, update
from openvietgis_api.config import get_settings
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_presence_payload():
    p = Peer("s", "a", "Name", "#2563eb")
    update(p, '{"cursor":[106,10],"view":[106,10,15,0,0]}')
    assert p.cursor == [106, 10]
    assert p.view == [106, 10, 15, 0, 0]
    for raw in (
        '{"actor":"forged"}',
        '{"cursor":[181,10]}',
        '{"cursor":[NaN,10]}',
        '{"cursor":[true,10]}',
        '{"view":[106,10,100,0,0]}',
    ):
        with pytest.raises(ValueError):
            update(p, raw)
    update(p, '{"cursor":null}')
    assert p.cursor is None


def test_presence_access_and_broadcast(migrated_database):
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    path = f"{org}/projects/{world.project_id}/collaboration/presence"
    origin = get_settings().web_base_url.rstrip("/")
    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as member,
        _client(world.wrong_user_id) as outsider,
    ):

        def headers(client, site=origin):
            return {
                "origin": site,
                "cookie": "; ".join(f"{k}={v}" for k, v in client.cookies.items()),
            }

        response = owner.post(
            org + "/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "editor",
            },
        )
        assert response.status_code == 201, response.text
        for h in (headers(owner, "https://attacker.invalid"), headers(outsider)):
            with pytest.raises(WebSocketDisconnect):
                with owner.websocket_connect(path, headers=h):
                    pass
        with owner.websocket_connect(path, headers=headers(owner)) as a:
            first = a.receive_json()
            assert len(first["users"]) == 1
            assert first["users"][0]["actor"] == str(world.owner_user_id)
            with owner.websocket_connect(path, headers=headers(member)) as b:
                joined = b.receive_json()
                assert len(joined["users"]) == 2
                assert next(u for u in joined["users"] if u["id"] == joined["self_id"])[
                    "actor"
                ] == str(world.member_user_id)
                assert len(a.receive_json()["users"]) == 2
                b.send_json({"cursor": [106.7, 10.8], "view": [106.7, 10.8, 13, 0, 0]})
                changed = a.receive_json()
                assert next(u for u in changed["users"] if u["actor"] == str(world.member_user_id))[
                    "cursor"
                ] == [106.7, 10.8]
            assert len(a.receive_json()["users"]) == 1

        # The WebSocket uses its own scope guard, not HTTP-only dependencies.
        # A restricted editor must still be unable to subscribe to presence.
        with get_admin_engine().begin() as db:
            db.execute(
                text(
                    "INSERT INTO gis_access.scoped_members(organization_id,project_id,user_id) "
                    "VALUES(:o,:p,:u)"
                ),
                {"o": world.organization_id, "p": world.project_id, "u": world.member_user_id},
            )
        with pytest.raises(WebSocketDisconnect):
            with owner.websocket_connect(path, headers=headers(member)):
                pass
