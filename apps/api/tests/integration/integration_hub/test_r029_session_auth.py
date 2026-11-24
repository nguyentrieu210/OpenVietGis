"""Managed QGIS session scopes against isolated real PostGIS and service credentials."""
from __future__ import annotations

from uuid import UUID

from sqlalchemy import text

from openvietgis_api.adapters.http.managed_sync import _session_still_valid
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from tests.fixtures.db_admin import get_admin_engine
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
    from uuid import uuid4

    world = seed_access_world()
    foreign = seed_access_world()
    root = _root(world) + "/integration-hub"
    bindings = root + "/bindings"
    sessions = root + "/managed/sessions"
    with _client(world.owner_user_id) as owner:
        project = _root(world)
        created = owner.post(project + "/layers/manual", json={
            "operation_id": str(uuid4()), "name": "QGIS auth layer",
            "geometry_family": "Point",
            "fields": [{"field_id": "fld_name", "name": "Display name",
                        "type": "text", "nullable": False}],
        })
        assert created.status_code == 201, created.text
        layer = created.json()["layer_id"]
        source = owner.get(project + "/layers/" + layer).json()["source"]
        seed_url = project + "/sources/" + created.json()["source_id"] + "/changesets"
        seed = owner.post(seed_url, json={
            "operation_id": str(uuid4()), "base_content_revision": source["content_revision"],
            "base_schema_revision": source["schema_revision"],
            "operations": [{"op_type": "create", "feature_id": "initial",
                            "geometry": {"type": "Point", "coordinates": [106.6, 10.7]},
                            "properties": {"fld_name": "Initial"}}],
        })
        assert seed.status_code == 201, seed.text
        candidates = owner.get(bindings + "/candidates")
        assert candidates.status_code == 200, candidates.text
        candidate = next(row for row in candidates.json()["items"] if row["layer_id"] == layer)

        def pair(name):
            response = owner.post(bindings, json={
                "layer_id": layer, "adapter_key": "qgis", "external_dataset_ref": name,
                "direction": "bidirectional_manual", "source_crs_code": 4326,
                "target_crs_code": 4326, "external_primary_key": "ovg_feature_id",
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
        sid = response.json()["id"]
        with get_admin_engine().connect() as db:
            credential_id = db.execute(text(
                "SELECT service_credential_id FROM integration_hub.binding_pairing_tokens "
                "WHERE id=:id"
            ), {"id": UUID(issued["token"]["id"])}).scalar_one()
        ctx = AuthorizationContext(
            actor=Actor("service_principal", UUID(response.json()["actor_id"]), "QGIS"),
            organization_id=world.organization_id,
            project_id=world.project_id,
            credential_id=credential_id,
        )
        assert _session_still_valid(ctx, UUID(sid))
        managed = root + "/managed"
        page_response = device.get(managed + "/bootstrap", params={
            "layer_id": layer, "session_id": sid, "limit": 1,
        })
        assert page_response.status_code == 200, page_response.text
        page = page_response.json()
        assert page["crs_code"] == 4326
        assert page["cacheable"] is True and page["feature_count"] == 1
        assert page["fields"] and page["features"]
        feature = page["features"][0]
        assert set(feature["properties"]) == {field["name"] for field in page["fields"]}
        assert feature["geometry"]["type"] == "Point"
        second = device.post(sessions, json={"binding_id": bid})
        assert second.status_code == 201, second.text
        second_id = second.json()["id"]
        commit = device.post(managed + "/commits", json={
            "session_id": sid, "source_id": page["source_id"],
            "base_content_revision": page["content_revision"],
            "base_schema_revision": page["schema_revision"],
            "operation_id": "qgis-fixture-create",
            "operations": [{
                "op_type": "create", "feature_id": "qgis-fixture-new",
                "geometry": feature["geometry"], "properties": feature["properties"],
            }],
        })
        assert commit.status_code == 201, commit.text
        receipt = commit.json()
        assert receipt["event_cursor"] > page["cursor"]
        delta = device.get(managed + "/delta", params={
            "session_id": second_id, "cursor": page["cursor"],
        })
        assert delta.status_code == 200, delta.text
        assert any(
            item["feature_id"] == "qgis-fixture-new"
            for change in delta.json()["changes"] for item in change["upserts"]
        )
        recovered = device.get(managed + "/commits/qgis-fixture-create")
        assert recovered.status_code == 200, recovered.text
        assert recovered.json()["changeset_id"] == receipt["changeset_id"]
        # Cross-engine acceptance: 100 new features, alternating the public
        # Web ChangeSet path and the managed desktop path, ten commits total.
        # Both use the real restricted database and authenticated API.
        cursor = receipt["event_cursor"]
        revision = receipt["resulting_content_revision"]
        expected_ids = set()
        last_payload = None
        last_receipt = None
        for batch in range(10):
            operations = [
                {
                    "op_type": "create",
                    "feature_id": f"r031-{batch}-{index}",
                    "geometry": {"type": "Point", "coordinates": [106.6, 10.7]},
                    "properties": feature["properties"],
                }
                for index in range(10)
            ]
            expected_ids.update(op["feature_id"] for op in operations)
            payload = {
                "source_id": page["source_id"],
                "session_id": sid,
                "base_content_revision": revision,
                "base_schema_revision": page["schema_revision"],
                "operation_id": str(uuid4()),
                "operations": operations,
            }
            if batch % 2 == 0:
                with _client(world.owner_user_id) as owner:
                    reply = owner.post(
                        seed_url,
                        json={
                            key: value for key, value in payload.items()
                            if key not in {"source_id", "session_id"}
                        },
                    )
            else:
                reply = device.post(managed + "/commits", json=payload)
            assert reply.status_code == 201, reply.text
            revision = reply.json()["resulting_content_revision"]
            if batch % 2:
                last_payload, last_receipt = payload, reply.json()

        assert last_payload is not None and last_receipt is not None
        replay = device.post(managed + "/commits", json=last_payload)
        assert replay.status_code == 201, replay.text
        assert replay.json()["changeset_id"] == last_receipt["changeset_id"]
        assert replay.json()["event_cursor"] == last_receipt["event_cursor"]

        # Simulated disconnected consumer catches up without a full bootstrap.
        observed_ids = []
        observed_cursors = []
        while True:
            reply = device.get(managed + "/delta", params={
                "session_id": second_id, "cursor": cursor, "limit": 3,
            })
            assert reply.status_code == 200, reply.text
            body = reply.json()
            assert not body["reset_required"], body
            for change in body["changes"]:
                observed_cursors.append(change["cursor"])
                observed_ids.extend(item["feature_id"] for item in change["upserts"])
                assert not change["removals"]
            assert body["next_cursor"] >= cursor
            if body["has_more"]:
                assert body["next_cursor"] > cursor
            cursor = body["next_cursor"]
            if not body["has_more"]:
                break
        assert len(observed_ids) == 100
        assert set(observed_ids) == expected_ids
        assert len(observed_cursors) == 10
        assert observed_cursors == sorted(set(observed_cursors))
        empty_delta = device.get(managed + "/delta", params={
            "session_id": second_id, "cursor": cursor,
        })
        assert empty_delta.status_code == 200
        assert empty_delta.json()["changes"] == []
        print("R031: 100 alternating Web/QGIS mutations converged over 10 ordered commits")

        assert device.post(sessions, json={"binding_id": other_bid}).status_code == 403
        assert device.post(_root(foreign) + "/integration-hub/managed/sessions",
                           json={"binding_id": bid}).status_code in (401, 403, 404)
    with _service_client(generic_token) as other:
        assert other.post(sessions, json={"binding_id": bid}).status_code == 403
        assert other.get(managed + "/delta", params={"session_id": sid}).status_code == 403
        assert other.get(managed + "/bootstrap", params={
            "session_id": sid, "layer_id": layer,
        }).status_code == 403
    with _client(world.owner_user_id) as owner:
        revoke = owner.post(
            bindings + "/" + bid + "/pairing-tokens/" + issued["token"]["id"] + "/revoke"
        )
        assert revoke.status_code == 200, revoke.text
    assert not _session_still_valid(ctx, UUID(sid))
    with _service_client(issued["bearer_token"]) as device:
        assert device.post(sessions, json={"binding_id": bid}).status_code == 401
