"""Managed QGIS session scopes against isolated real PostGIS and service credentials."""
from __future__ import annotations

from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.integration_hub.test_integration_hub_rls_acl import (
    _credential,
    _grant,
    _principal,
    _root,
)
from tests.integration.service_identity.test_service_authorization import _service_client
from tests.integration.spatial.test_sources import _client


def test_managed_qgis_session_preserves_project_scope_and_edit_capability(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    url = f"{_root(world)}/integration-hub/managed/sessions"
    with _client(world.owner_user_id) as owner:
        viewer_id = _principal(owner, world)
        _grant(owner, world, "service_principal", viewer_id, "viewer")
        _, viewer_token = _credential(owner, world, viewer_id, ["project.read"])

        editor_id = _principal(owner, world)
        _grant(owner, world, "service_principal", editor_id, "editor")
        _, editor_token = _credential(
            owner, world, editor_id, ["project.read", "project.edit"]
        )

    for token, want_edit in ((viewer_token, False), (editor_token, True)):
        with _service_client(token) as client:
            response = client.post(url, json={"device_label": "isolated-qgis-test"})
            assert response.status_code == 201, response.text
            data = response.json()
            assert data["capabilities"]["can_read"] is True
            assert data["capabilities"]["can_edit"] is want_edit
            assert data["state"] == "active"
            assert client.get(f"{url}/{data['id']}").status_code == 200


def test_managed_qgis_session_rejects_other_project_actor(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    base = f"{_root(world)}/integration-hub/managed"
    with _client(world.owner_user_id) as owner:
        tokens = []
        for _ in range(2):
            principal = _principal(owner, world)
            _grant(owner, world, "service_principal", principal, "editor")
            _, token = _credential(
                owner, world, principal, ["project.read", "project.edit"]
            )
            tokens.append(token)

    with _service_client(tokens[0]) as first:
        created = first.post(f"{base}/sessions", json={"device_label": "test-owner"})
        assert created.status_code == 201, created.text
        sid = created.json()["id"]
        own_delta = first.get(f"{base}/delta", params={"session_id": sid, "cursor": 0})
        assert own_delta.status_code == 200, own_delta.text

    with _service_client(tokens[1]) as second:
        response = second.get(f"{base}/delta", params={"session_id": sid, "cursor": 0})
        assert response.status_code == 404, response.text
        commands = second.get(f"{base}/desktop-sessions/{sid}/commands")
        assert commands.status_code == 404, commands.text
        forged = second.post(
            f"{base}/commits",
            json={
                "session_id": sid,
                "source_id": str(uuid4()),
                "base_content_revision": "a" * 64,
                "operation_id": str(uuid4()),
                "operations": [{"op_type": "delete", "feature_id": "fixture"}],
            },
        )
        assert forged.status_code == 404, forged.text
        command = second.post(
            f"{base}/desktop-sessions/{sid}/commands",
            json={"tool_name": "inspect_project", "arguments": {}},
        )
        assert command.status_code in (403, 404), command.text



def test_manager_can_enqueue_but_only_session_actor_can_respond(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    base = f"{_root(world)}/integration-hub/managed"
    with _client(world.owner_user_id) as owner:
        tokens = []
        for _ in range(2):
            principal = _principal(owner, world)
            _grant(owner, world, "service_principal", principal, "editor")
            _, token = _credential(
                owner, world, principal, ["project.read", "project.edit"]
            )
            tokens.append(token)
        with _service_client(tokens[0]) as desktop:
            created = desktop.post(f"{base}/sessions", json={"device_label": "desktop"})
            assert created.status_code == 201, created.text
            sid = created.json()["id"]
        command = owner.post(
            f"{base}/desktop-sessions/{sid}/commands",
            json={"tool_name": "inspect_project", "arguments": {}},
        )
        assert command.status_code == 201, command.text
        command_id = command.json()["id"]

    url = f"{base}/desktop-sessions/{sid}/commands/{command_id}/respond"
    with _service_client(tokens[1]) as other:
        response = other.post(url, json={"status": "rejected"})
        assert response.status_code == 404, response.text
    with _service_client(tokens[0]) as desktop:
        response = desktop.post(url, json={"status": "rejected"})
        assert response.status_code == 200, response.text