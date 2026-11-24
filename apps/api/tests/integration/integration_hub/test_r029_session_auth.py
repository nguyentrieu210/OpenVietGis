"""Managed QGIS session scopes against isolated real PostGIS and service credentials."""
from __future__ import annotations

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

def test_real_pairing_credential_can_open_only_its_binding(migrated_database: None) -> None:
    from tests.integration.spatial.test_layers import _create
    from tests.integration.spatial.test_sources import _path as source_path
    from tests.integration.spatial.test_sources import _register

    world = seed_access_world()
    foreign = seed_access_world()
    root = _root(world) + "/integration-hub"
    bindings = root + "/bindings"
    sessions = root + "/managed/sessions"
    with _client(world.owner_user_id) as owner:
        source = _register(owner, source_path(world), "reference_points_5899", "QGIS auth fixture")
        layer = _create(owner, world, source["resource_id"], "QGIS auth layer")["resource_id"]
        candidates = owner.get(bindings + "/candidates")
        assert candidates.status_code == 200, candidates.text
        candidate = next(row for row in candidates.json()["items"] if row["layer_id"] == layer)

        def pair(name):
            response = owner.post(bindings, json={
                "layer_id": layer, "adapter_key": "qgis", "external_dataset_ref": name,
                "direction": "bidirectional_manual", "source_crs_code": 5899,
                "target_crs_code": 5899, "external_primary_key": "ovg_feature_id",
                "allowed_field_ids": candidate["allowed_field_ids"],
            })
            assert response.status_code == 201, response.text
            binding = response.json()
            bid = binding["id"]
            active = owner.post(bindings + "/" + bid + "/activate",
                                json={"expected_revision": binding["revision"]})
            assert active.status_code == 200, active.text
            issued = owner.post(bindings + "/" + bid + "/pairing-tokens",
                                json={"label": "QGIS isolated fixture", "expires_in_days": 1})
            assert issued.status_code == 201, issued.text
            return bid, issued.json()

        bid, issued = pair("own-binding")
        other_bid, _ = pair("other-binding")
        generic_id = _principal(owner, world)
        _grant(owner, world, "service_principal", generic_id, "editor")
        _, generic_token = _credential(owner, world, generic_id, ["project.read", "project.edit"])

    with _service_client(issued["bearer_token"]) as device:
        assert device.get(root + "/adapters/qgis").status_code == 200
        response = device.post(sessions, json={"binding_id": bid})
        assert response.status_code == 201, response.text
        assert response.json()["binding_id"] == bid
        assert response.json()["capabilities"]["can_edit"] is True
        assert device.post(sessions, json={"binding_id": other_bid}).status_code == 403
        assert device.post(_root(foreign) + "/integration-hub/managed/sessions",
                           json={"binding_id": bid}).status_code in (401, 403, 404)
    with _service_client(generic_token) as other:
        assert other.post(sessions, json={"binding_id": bid}).status_code == 403
    with _client(world.owner_user_id) as owner:
        revoke = owner.post(
            bindings + "/" + bid + "/pairing-tokens/" + issued["token"]["id"] + "/revoke"
        )
        assert revoke.status_code == 200, revoke.text
    with _service_client(issued["bearer_token"]) as device:
        assert device.post(sessions, json={"binding_id": bid}).status_code == 401
