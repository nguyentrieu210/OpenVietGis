"""Partner extension approval, activation and isolation through real API/DB."""

from __future__ import annotations

from copy import deepcopy

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_authorization import _grant
from tests.integration.spatial.test_sources import _client


def partner_manifest() -> dict:
    return {
        "extension_key": "partner_example",
        "version": "1.0.0",
        "api_compatibility": "1.x",
        "display_name": "Example partner",
        "description": "Sandboxed UI",
        "permissions": [],
        "commands": [],
        "capabilities": [
            "host_context",
            "map_view",
            "map_navigate",
            "map_selection",
            "map_overlay",
            "tool_register",
        ],
        "entity_schemas": {},
        "ui_contributions": [
            {
                "id": "partner_panel",
                "slot": "project_panel",
                "label": "Example partner",
                "route_suffix": "/partner",
                "runtime": "sandboxed_iframe",
                "sdk_version": "1.0",
                "url": "https://plugins.example.com/v1/index.html",
            }
        ],
    }


def test_partner_manifest_security_and_activation(migrated_database: None) -> None:
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"
    manifests = f"{org}/extensions/manifests"
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        assert member.post(manifests, json=partner_manifest()).status_code == 403
        for url in (
            "javascript:alert(1)",
            "http://plugins.example.com/a",
            "https://localhost/a",
            "https://127.0.0.1/a",
            "https://openvietgis.cloud/rewrite/",
            "https://plugins.example.com/a?secret=x",
            "https://user@plugins.example.com/a",
            "https://plugins.example.com:8443/a",
        ):
            data = partner_manifest()
            data["ui_contributions"][0]["url"] = url
            result = owner.post(manifests, json=data)
            assert result.status_code in (400, 422), result.text
        for patch in (
            {"extension_key": "audit_notes"},
            {"permissions": ["ext.partner_example.read"]},
            {"capabilities": ["project_read"]},
            {"entity_schemas": {"note": {"fields": {"name": {"type": "text"}}}}},
        ):
            result = owner.post(manifests, json={**partner_manifest(), **patch})
            assert result.status_code in (400, 422), result.text
        data = partner_manifest()
        result = owner.post(manifests, json=data)
        assert result.status_code == 201, result.text
        manifest = result.json()
        assert manifest["ui_contributions"] == data["ui_contributions"]
        assert owner.post(manifests, json=data).status_code == 409
        _grant(owner, world, "membership", str(world.member_membership_id), "editor")
        payload = {"manifest_id": manifest["id"]}
        assert member.post(f"{project}/extensions/activations", json=payload).status_code == 403
        result = owner.post(f"{project}/extensions/activations", json=payload)
        assert result.status_code == 201, result.text
        records = f"{project}/extensions/partner_example/records"
        result = owner.post(records, json={"entity_name": "note", "payload": {"name": "x"}})
        assert result.status_code in (400, 403, 404, 422), result.text
        updated = deepcopy(data)
        updated["version"] = "2.0.0"
        result = owner.post(manifests, json=updated)
        assert result.status_code == 201, result.text
        replacement = owner.post(
            f"{project}/extensions/activations", json={"manifest_id": result.json()["id"]}
        )
        assert replacement.status_code == 409
        result = owner.post(f"{project}/extensions/partner_example/disable")
        assert result.status_code == 200, result.text
        assert result.json()["status"] == "disabled"
