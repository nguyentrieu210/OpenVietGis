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
        _, editor_token = _credential(owner, world, editor_id, ["project.read", "project.edit"])

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
        created = owner.post(
            project + "/layers/manual",
            json={
                "operation_id": str(uuid4()),
                "name": "QGIS auth layer",
                "geometry_family": "Point",
                "fields": [
                    {
                        "field_id": "fld_name",
                        "name": "Display name",
                        "type": "text",
                        "nullable": False,
                    }
                ],
            },
        )
        assert created.status_code == 201, created.text
        layer = created.json()["layer_id"]
        source = owner.get(project + "/layers/" + layer).json()["source"]
        seed_url = project + "/sources/" + created.json()["source_id"] + "/changesets"
        seed = owner.post(
            seed_url,
            json={
                "operation_id": str(uuid4()),
                "base_content_revision": source["content_revision"],
                "base_schema_revision": source["schema_revision"],
                "operations": [
                    {
                        "op_type": "create",
                        "feature_id": "initial",
                        "geometry": {"type": "Point", "coordinates": [106.6, 10.7]},
                        "properties": {"fld_name": "Initial"},
                    }
                ],
            },
        )
        assert seed.status_code == 201, seed.text
        candidates = owner.get(bindings + "/candidates")
        assert candidates.status_code == 200, candidates.text
        candidate = next(row for row in candidates.json()["items"] if row["layer_id"] == layer)

        def pair(name):
            response = owner.post(
                bindings,
                json={
                    "layer_id": layer,
                    "adapter_key": "qgis",
                    "external_dataset_ref": name,
                    "direction": "bidirectional_manual",
                    "source_crs_code": 4326,
                    "target_crs_code": 4326,
                    "external_primary_key": "ovg_feature_id",
                    "allowed_field_ids": candidate["allowed_field_ids"],
                },
            )
            assert response.status_code == 201, response.text
            binding = response.json()
            bid = binding["id"]
            active = owner.post(
                bindings + "/" + bid + "/activate", json={"expected_revision": binding["revision"]}
            )
            assert active.status_code == 200, active.text
            issued = owner.post(
                bindings + "/" + bid + "/pairing-tokens",
                json={"label": "QGIS isolated fixture", "expires_in_days": 1},
            )
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
        managed = root + "/managed"
        page_response = device.get(
            managed + "/bootstrap",
            params={
                "layer_id": layer,
                "session_id": sid,
                "limit": 1,
            },
        )
        assert page_response.status_code == 200, page_response.text
        page = page_response.json()
        assert page["crs_code"] == 4326
        assert page["fields"] and page["features"]
        feature = page["features"][0]
        assert set(feature["properties"]) == {field["name"] for field in page["fields"]}
        assert feature["geometry"]["type"] == "Point"
        second = device.post(sessions, json={"binding_id": bid})
        assert second.status_code == 201, second.text
        second_id = second.json()["id"]
        commit = device.post(
            managed + "/commits",
            json={
                "session_id": sid,
                "source_id": page["source_id"],
                "base_content_revision": page["content_revision"],
                "base_schema_revision": page["schema_revision"],
                "operation_id": "qgis-fixture-create",
                "operations": [
                    {
                        "op_type": "create",
                        "feature_id": "qgis-fixture-new",
                        "geometry": feature["geometry"],
                        "properties": feature["properties"],
                    }
                ],
            },
        )
        assert commit.status_code == 201, commit.text
        receipt = commit.json()
        assert receipt["event_cursor"] > page["cursor"]
        delta = device.get(
            managed + "/delta",
            params={
                "session_id": second_id,
                "cursor": page["cursor"],
            },
        )
        assert delta.status_code == 200, delta.text
        assert any(
            item["feature_id"] == "qgis-fixture-new"
            for change in delta.json()["changes"]
            for item in change["upserts"]
        )
        recovered = device.get(managed + "/commits/qgis-fixture-create")
        assert recovered.status_code == 200, recovered.text
        assert recovered.json()["changeset_id"] == receipt["changeset_id"]
        assert device.post(sessions, json={"binding_id": other_bid}).status_code == 403
        assert device.post(
            _root(foreign) + "/integration-hub/managed/sessions", json={"binding_id": bid}
        ).status_code in (401, 403, 404)
    with _service_client(generic_token) as other:
        assert other.post(sessions, json={"binding_id": bid}).status_code == 403
    with _client(world.owner_user_id) as owner:
        revoke = owner.post(
            bindings + "/" + bid + "/pairing-tokens/" + issued["token"]["id"] + "/revoke"
        )
        assert revoke.status_code == 200, revoke.text
    with _service_client(issued["bearer_token"]) as device:
        assert device.post(sessions, json={"binding_id": bid}).status_code == 401


def test_managed_session_actor_isolation_and_epoch_authority(migrated_database: None) -> None:
    from uuid import uuid4

    world = seed_access_world()
    root = _root(world) + "/integration-hub/managed"
    url = root + "/sessions"
    with _client(world.owner_user_id) as owner:
        tokens = []
        for _ in range(2):
            principal = _principal(owner, world)
            _grant(owner, world, "service_principal", principal, "editor")
            _, token = _credential(owner, world, principal, ["project.read", "project.edit"])
            tokens.append(token)
    with _service_client(tokens[0]) as a, _service_client(tokens[1]) as b:
        created = a.post(url, json={"device_label": "A"})
        assert created.status_code == 201, created.text
        sid = created.json()["id"]
        second = b.post(url, json={"device_label": "B"})
        assert second.status_code == 201, second.text
        bid = second.json()["id"]
        commands = root + "/desktop-sessions/" + sid + "/commands"
        assert {item["id"] for item in b.get(url).json()["items"]} == {bid}
        assert b.get(url + "/" + sid).status_code == 404
        assert b.get(root + "/delta", params={"session_id": sid}).status_code == 404
        assert (
            b.get(
                root + "/bootstrap", params={"session_id": sid, "layer_id": str(uuid4())}
            ).status_code
            == 404
        )
        assert b.get(commands).status_code == 404
        assert b.post(commands, json={"tool_name": "inspect_project"}).status_code == 404
        assert (
            b.post(
                commands + "/" + str(uuid4()) + "/respond", json={"status": "completed"}
            ).status_code
            == 404
        )
        assert (
            b.post(
                root + "/commits",
                json={
                    "session_id": sid,
                    "source_id": str(uuid4()),
                    "base_content_revision": "a" * 64,
                    "operation_id": "foreign",
                    "operations": [{"op_type": "delete", "feature_id": "p1"}],
                },
            ).status_code
            == 404
        )
        assert b.delete(url + "/" + sid).status_code == 403
        assert a.delete(url + "/" + sid + "?bump_epoch=true").status_code == 403
        assert a.get(url + "/" + sid).json()["state"] == "active"
        assert a.get(root + "/delta", params={"session_id": sid}).status_code == 200
        with _client(world.owner_user_id) as owner:
            assert {sid, bid} <= {item["id"] for item in owner.get(url).json()["items"]}
            cmd = owner.post(commands, json={"tool_name": "inspect_project"})
            assert cmd.status_code == 201, cmd.text
            assert (
                owner.post(
                    commands + "/" + cmd.json()["id"] + "/respond", json={"status": "completed"}
                ).status_code
                == 404
            )
            assert (
                a.post(
                    commands + "/" + cmd.json()["id"] + "/respond", json={"status": "rejected"}
                ).status_code
                == 200
            )
            revoked = owner.delete(url + "/" + bid + "?bump_epoch=true")
            assert revoked.status_code == 200, revoked.text
        assert a.get(root + "/delta", params={"session_id": sid}).json()["reset_required"] is True
        assert a.get(commands).status_code == 403
        assert (
            a.get(
                root + "/bootstrap", params={"session_id": sid, "layer_id": str(uuid4())}
            ).status_code
            == 403
        )
        assert b.get(root + "/delta", params={"session_id": bid}).status_code == 403
        assert a.delete(url + "/" + sid).status_code == 200


def test_scoped_delta_does_not_reveal_hidden_feature_ids(migrated_database: None) -> None:
    from uuid import uuid4

    world = seed_access_world()
    root = _root(world)
    managed = root + "/integration-hub/managed"
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        _grant(owner, world, "membership", str(world.member_membership_id), "editor")
        created = owner.post(
            root + "/layers/manual",
            json={
                "operation_id": str(uuid4()),
                "name": "Scope fixture",
                "geometry_family": "Point",
                "fields": [
                    {"field_id": "label", "name": "Label", "type": "text", "nullable": False}
                ],
            },
        )
        assert created.status_code == 201, created.text
        layer, source = created.json()["layer_id"], created.json()["source_id"]

        def commit(ops):
            pin = owner.get(root + "/layers/" + layer).json()["source"]
            response = owner.post(
                root + "/sources/" + source + "/changesets",
                json={
                    "operation_id": str(uuid4()),
                    "base_content_revision": pin["content_revision"],
                    "base_schema_revision": pin["schema_revision"],
                    "operations": ops,
                },
            )
            assert response.status_code == 201, response.text

        commit(
            [
                {
                    "op_type": "create",
                    "feature_id": fid,
                    "geometry": {"type": "Point", "coordinates": [106.6, 10.7]},
                    "properties": {"label": fid},
                }
                for fid in ("visible", "hidden")
            ]
        )
        rule = owner.post(
            root + "/access/rules",
            json={
                "user_id": str(world.member_user_id),
                "layer_id": layer,
                "actions": ["read"],
                "feature_ids": ["visible"],
            },
        )
        assert rule.status_code == 201, rule.text
        session = member.post(managed + "/sessions", json={})
        assert session.status_code == 201, session.text
        sid = session.json()["id"]
        page = member.get(managed + "/bootstrap", params={"session_id": sid, "layer_id": layer})
        assert page.status_code == 200, page.text
        assert [f["feature_id"] for f in page.json()["features"]] == ["visible"]
        commit([{"op_type": "update", "feature_id": "hidden", "properties": {"label": "changed"}}])
        delta = member.get(
            managed + "/delta", params={"session_id": sid, "cursor": page.json()["cursor"]}
        )
        assert delta.status_code == 200, delta.text
        assert delta.json()["reset_required"] is True
        assert delta.json()["changes"] == []
        assert "hidden" not in delta.text
