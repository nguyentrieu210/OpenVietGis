"""T006 real PostGIS integration suite for integration hub RLS, service principals, and GIS ACL.

Uses the restricted runtime database role via FastAPI HTTP endpoints, real R002 sessions
and service principal bearer credentials, and real gis_access scoped layer rules.
"""

from __future__ import annotations

from uuid import uuid4

from fastapi.testclient import TestClient

from openvietgis_api.main import create_app
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.service_identity.test_service_authorization import _service_client
from tests.integration.spatial.test_layers import _create
from tests.integration.spatial.test_sources import _client, _register
from tests.integration.spatial.test_sources import _path as _source_path


def _org(world: AccessWorld) -> str:
    return f"/api/v1/organizations/{world.organization_id}"


def _root(world: AccessWorld) -> str:
    return f"{_org(world)}/projects/{world.project_id}"


def _grant(owner: TestClient, world: AccessWorld, kind: str, subject_id: str, role: str) -> str:
    response = owner.post(
        f"{_org(world)}/grants",
        json={
            "subject_kind": kind,
            "subject_id": subject_id,
            "scope_kind": "project",
            "project_id": str(world.project_id),
            "role": role,
        },
    )
    assert response.status_code == 201, response.text
    return str(response.json()["id"])


def _principal(owner: TestClient, world: AccessWorld) -> str:
    response = owner.post(
        f"{_org(world)}/service-principals",
        json={"name": f"integration-hub-{uuid4().hex[:8]}"},
    )
    assert response.status_code == 201, response.text
    return str(response.json()["id"])


def _credential(
    owner: TestClient, world: AccessWorld, principal_id: str, permissions: list[str]
) -> tuple[str, str]:
    response = owner.post(
        f"{_org(world)}/service-principals/{principal_id}/credentials",
        json={"name": f"ceiling-{uuid4().hex[:8]}", "permission_ceiling": permissions},
    )
    assert response.status_code == 201, response.text
    body = response.json()
    return str(body["id"]), str(body["secret"])


def test_integration_hub_rejects_anonymous_and_cross_tenant_rls(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    other = seed_access_world()
    root = _root(world)
    catalog_url = f"{root}/integration-hub/catalog"
    qgis_url = f"{root}/integration-hub/adapters/qgis"
    unknown_url = f"{root}/integration-hub/adapters/nonexistent_adapter"

    with TestClient(create_app()) as anon:
        assert anon.get(catalog_url).status_code == 401
        assert anon.get(qgis_url).status_code == 401
        assert anon.get(unknown_url).status_code == 401

    with _client(other.owner_user_id) as outsider:
        r_catalog = outsider.get(catalog_url)
        r_qgis = outsider.get(qgis_url)
        r_unknown = outsider.get(unknown_url)
        assert r_catalog.status_code in (403, 404), r_catalog.text
        assert r_qgis.status_code in (403, 404), r_qgis.text
        assert r_unknown.status_code in (403, 404), r_unknown.text
        # Cross-tenant RLS must conceal known vs unknown adapter keys identically.
        assert r_qgis.status_code == r_unknown.status_code
        assert r_qgis.json() == r_unknown.json()

    with _client(world.member_user_id) as member:
        r_member_catalog = member.get(catalog_url)
        r_member_qgis = member.get(qgis_url)
        r_member_unknown = member.get(unknown_url)
        assert r_member_catalog.status_code in (403, 404), r_member_catalog.text
        assert r_member_qgis.status_code in (403, 404), r_member_qgis.text
        assert r_member_unknown.status_code in (403, 404), r_member_unknown.text
        # Member without project grant must be denied before adapter lookup.
        assert r_member_qgis.status_code == r_member_unknown.status_code
        assert r_member_qgis.json() == r_member_unknown.json()

    with _client(world.owner_user_id) as owner:
        for method in ("POST", "PUT", "PATCH", "DELETE"):
            assert owner.request(method, catalog_url).status_code == 405
            assert owner.request(method, qgis_url).status_code == 405


def test_integration_hub_service_principal_ceiling_and_revocation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    other = seed_access_world()
    root = _root(world)
    other_root = _root(other)
    catalog_url = f"{root}/integration-hub/catalog"
    qgis_url = f"{root}/integration-hub/adapters/qgis"

    def _assert_read_ceiling_reflected(actions: list[dict[str, object]]) -> None:
        for action in actions:
            if action["required_permission"] in ("project.edit", "project.manage"):
                assert action["eligible_by_role"] is False, action
                assert action["allowed"] is False, action
                assert action["reason"] == "permission_denied", action
            if action["action"] != "catalog.discover":
                assert action["allowed"] is False, action
                assert action["executable_in_hub"] is False, action

    with _client(world.owner_user_id) as owner:
        principal_id = _principal(owner, world)
        viewer_grant_id = _grant(owner, world, "service_principal", principal_id, "viewer")
        credential_id, secret = _credential(owner, world, principal_id, ["project.read"])

        with _service_client(secret) as svc:
            r_cat = svc.get(catalog_url)
            assert r_cat.status_code == 200, r_cat.text
            cat_body = r_cat.json()
            assert cat_body["default_write_policy"] == "deny"
            assert cat_body["scoped_access"] is False
            for adapter in cat_body["adapters"]:
                _assert_read_ceiling_reflected(adapter["actions"])

            r_qgis = svc.get(qgis_url)
            assert r_qgis.status_code == 200, r_qgis.text
            qgis_body = r_qgis.json()
            assert qgis_body["execution_status"] == "read_only_profile_ready"
            assert qgis_body["default_write_policy"] == "deny"
            assert qgis_body["connection_profile"]["supports_direct_db_credentials"] is False
            _assert_read_ceiling_reflected(qgis_body["adapter"]["actions"])

            # Upgrade principal to manager; the read-only credential ceiling must still apply.
            revoked_grant = owner.post(f"{_org(world)}/grants/{viewer_grant_id}/revoke")
            assert revoked_grant.status_code == 204, revoked_grant.text
            _grant(owner, world, "service_principal", principal_id, "manager")

            r_cat_mgr = svc.get(catalog_url)
            assert r_cat_mgr.status_code == 200, r_cat_mgr.text
            for adapter in r_cat_mgr.json()["adapters"]:
                _assert_read_ceiling_reflected(adapter["actions"])

            r_qgis_mgr = svc.get(qgis_url)
            assert r_qgis_mgr.status_code == 200, r_qgis_mgr.text
            assert r_qgis_mgr.json()["default_write_policy"] == "deny"
            _assert_read_ceiling_reflected(r_qgis_mgr.json()["adapter"]["actions"])

            # Cross-org bearer request must fail.
            r_other_cat = svc.get(f"{other_root}/integration-hub/catalog")
            r_other_qgis = svc.get(f"{other_root}/integration-hub/adapters/qgis")
            assert r_other_cat.status_code in (401, 403, 404), r_other_cat.text
            assert r_other_qgis.status_code in (401, 403, 404), r_other_qgis.text

            # After credential revocation, bearer requests return 401.
            revoked = owner.post(
                f"{_org(world)}/service-principals/{principal_id}/credentials/{credential_id}/revoke"
            )
            assert revoked.status_code == 204, revoked.text
            assert svc.get(catalog_url).status_code == 401
            assert svc.get(qgis_url).status_code == 401


def test_integration_hub_gis_scoped_layer_acl_and_crs_preservation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = _root(world)
    catalog_url = f"{root}/integration-hub/catalog"
    qgis_url = f"{root}/integration-hub/adapters/qgis"
    postgis_url = f"{root}/integration-hub/adapters/postgis"

    with _client(world.owner_user_id) as admin, _client(world.member_user_id) as member:
        allowed_source_id = _register(
            admin, _source_path(world), "reference_points_5899", "Allowed VN-2000"
        )["resource_id"]
        allowed_layer_id = _create(admin, world, allowed_source_id, "Allowed")["resource_id"]

        hidden_source_id = _register(
            admin, _source_path(world), "reference_points_3857", "Hidden WebMercator"
        )["resource_id"]
        hidden_layer_id = _create(admin, world, hidden_source_id, "Hidden")["resource_id"]

        _grant(
            owner=admin,
            world=world,
            kind="membership",
            subject_id=str(world.member_membership_id),
            role="editor",
        )

        rule_resp = admin.post(
            f"{root}/access/rules",
            json={
                "user_id": str(world.member_user_id),
                "layer_id": allowed_layer_id,
                "actions": ["read", "geometry"],
            },
        )
        assert rule_resp.status_code == 201, rule_resp.text
        read_rule_id = rule_resp.json()["id"]

        r_member_cat = member.get(catalog_url)
        assert r_member_cat.status_code == 200, r_member_cat.text
        assert r_member_cat.json()["scoped_access"] is True

        for adapter_url in (qgis_url, postgis_url):
            r_profile = member.get(adapter_url)
            assert r_profile.status_code == 200, r_profile.text
            body = r_profile.json()
            assert body["scoped_access"] is True
            assert body["execution_status"] == "read_only_profile_ready"
            assert body["connection_profile"]["supports_direct_db_credentials"] is False
            layers = body["layers"]
            assert [layer["layer_id"] for layer in layers] == [allowed_layer_id]
            assert layers[0]["stored_crs_code"] == 5899
            assert layers[0]["source_crs_code"] == 5899
            assert layers[0]["crs_explicit"] is True
            assert layers[0]["export_permitted"] is False

        # Replace rule with one including "export" while keeping Hidden inaccessible.
        assert admin.delete(f"{root}/access/rules/{read_rule_id}").status_code == 204
        export_rule_resp = admin.post(
            f"{root}/access/rules",
            json={
                "user_id": str(world.member_user_id),
                "layer_id": allowed_layer_id,
                "actions": ["read", "geometry", "export"],
            },
        )
        assert export_rule_resp.status_code == 201, export_rule_resp.text
        export_rule_id = export_rule_resp.json()["id"]

        for adapter_url in (qgis_url, postgis_url):
            r_profile = member.get(adapter_url)
            assert r_profile.status_code == 200, r_profile.text
            layers = r_profile.json()["layers"]
            assert [layer["layer_id"] for layer in layers] == [allowed_layer_id]
            assert layers[0]["stored_crs_code"] == 5899
            assert layers[0]["crs_explicit"] is True
            assert layers[0]["export_permitted"] is True

        # Revoke the rule, leaving member scoped with 0 rules.
        assert admin.delete(f"{root}/access/rules/{export_rule_id}").status_code == 204
        for adapter_url in (qgis_url, postgis_url):
            r_empty = member.get(adapter_url)
            assert r_empty.status_code == 200, r_empty.text
            assert r_empty.json()["scoped_access"] is True
            assert r_empty.json()["layers"] == []

        r_admin = admin.get(qgis_url)
        assert r_admin.status_code == 200, r_admin.text
        admin_body = r_admin.json()
        assert admin_body["scoped_access"] is False
        admin_layers = {layer["layer_id"]: layer for layer in admin_body["layers"]}
        assert set(admin_layers) == {allowed_layer_id, hidden_layer_id}
        assert admin_layers[allowed_layer_id]["stored_crs_code"] == 5899
        assert admin_layers[hidden_layer_id]["stored_crs_code"] == 3857
        assert admin_layers[allowed_layer_id]["export_permitted"] is True
        assert admin_layers[hidden_layer_id]["export_permitted"] is True
