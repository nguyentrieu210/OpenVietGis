"""T006 real PostGIS integration suite for integration hub RLS, service principals, and GIS ACL.

Uses the restricted runtime database role via FastAPI HTTP endpoints, real R002 sessions
and service principal bearer credentials, and real gis_access scoped layer/feature rules.
"""

from __future__ import annotations

import hashlib
from uuid import uuid4

from fastapi.testclient import TestClient

from openvietgis_api.adapters.formats.geopackage import (
    inspect_geopackage,
    parse_geopackage_member,
)
from openvietgis_api.main import create_app
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.service_identity.test_service_authorization import _service_client
from tests.integration.spatial.test_layers import _create
from tests.integration.spatial.test_sources import _client, _register
from tests.integration.spatial.test_sources import _path as _source_path

_EXPORT_ENABLED_ADAPTERS = frozenset({"qgis", "postgis"})


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

    with _client(world.owner_user_id) as owner:
        source_id = _register(
            owner, _source_path(world), "reference_points_5899", "Tenant VN-2000"
        )["resource_id"]
        layer_id = _create(owner, world, source_id, "Tenant Layer")["resource_id"]

    export_url = f"{root}/integration-hub/adapters/qgis/layers/{layer_id}/export"

    with TestClient(create_app()) as anon:
        assert anon.get(catalog_url).status_code == 401
        assert anon.get(qgis_url).status_code == 401
        assert anon.get(unknown_url).status_code == 401
        assert anon.get(export_url).status_code == 401

    with _client(other.owner_user_id) as outsider:
        r_catalog = outsider.get(catalog_url)
        r_qgis = outsider.get(qgis_url)
        r_unknown = outsider.get(unknown_url)
        r_export = outsider.get(export_url)
        assert r_catalog.status_code in (403, 404), r_catalog.text
        assert r_qgis.status_code in (403, 404), r_qgis.text
        assert r_unknown.status_code in (403, 404), r_unknown.text
        assert r_export.status_code in (403, 404), r_export.text
        # Cross-tenant RLS must conceal known vs unknown adapter keys identically.
        assert r_qgis.status_code == r_unknown.status_code
        assert r_qgis.json() == r_unknown.json()

    with _client(world.member_user_id) as member:
        r_member_catalog = member.get(catalog_url)
        r_member_qgis = member.get(qgis_url)
        r_member_unknown = member.get(unknown_url)
        r_member_export = member.get(export_url)
        assert r_member_catalog.status_code in (403, 404), r_member_catalog.text
        assert r_member_qgis.status_code in (403, 404), r_member_qgis.text
        assert r_member_unknown.status_code in (403, 404), r_member_unknown.text
        assert r_member_export.status_code in (403, 404), r_member_export.text
        # Member without project grant must be denied before adapter lookup.
        assert r_member_qgis.status_code == r_member_unknown.status_code
        assert r_member_qgis.json() == r_member_unknown.json()

    with _client(world.owner_user_id) as owner:
        for method in ("POST", "PUT", "PATCH", "DELETE"):
            assert owner.request(method, catalog_url).status_code == 405
            assert owner.request(method, qgis_url).status_code == 405
            assert owner.request(method, export_url).status_code == 405


def test_integration_hub_service_principal_ceiling_and_revocation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    other = seed_access_world()
    root = _root(world)
    other_root = _root(other)
    catalog_url = f"{root}/integration-hub/catalog"
    qgis_url = f"{root}/integration-hub/adapters/qgis"

    def _assert_read_ceiling_reflected(
        adapter_key: str, actions: list[dict[str, object]]
    ) -> None:
        for action in actions:
            if action["required_permission"] in ("project.edit", "project.manage"):
                assert action["eligible_by_role"] is False, (adapter_key, action)
                assert action["allowed"] is False, (adapter_key, action)
                assert action["reason"] == "permission_denied", (adapter_key, action)
            if action["action"] == "catalog.discover":
                assert action["allowed"] is True, (adapter_key, action)
            elif action["action"] == "data.export" and adapter_key in _EXPORT_ENABLED_ADAPTERS:
                assert action["allowed"] is True, (adapter_key, action)
                assert action["executable_in_hub"] is True, (adapter_key, action)
                assert action["reason"] == "read_only_export", (adapter_key, action)
            else:
                assert action["allowed"] is False, (adapter_key, action)
                assert action["executable_in_hub"] is False, (adapter_key, action)

    with _client(world.owner_user_id) as owner:
        source_id = _register(
            owner, _source_path(world), "reference_points_5899", "SP VN-2000"
        )["resource_id"]
        layer_id = _create(owner, world, source_id, "SP Export Layer")["resource_id"]
        qgis_export_url = (
            f"{root}/integration-hub/adapters/qgis/layers/{layer_id}/export?format=geopackage"
        )
        postgis_export_url = (
            f"{root}/integration-hub/adapters/postgis/layers/{layer_id}/export?format=postgis_sql"
        )

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
                _assert_read_ceiling_reflected(adapter["key"], adapter["actions"])

            r_qgis = svc.get(qgis_url)
            assert r_qgis.status_code == 200, r_qgis.text
            qgis_body = r_qgis.json()
            assert qgis_body["execution_status"] == "read_only_profile_ready"
            assert qgis_body["default_write_policy"] == "deny"
            assert qgis_body["connection_profile"]["supports_direct_db_credentials"] is False
            _assert_read_ceiling_reflected("qgis", qgis_body["adapter"]["actions"])

            # Service principal with project.read ceiling can export .gpkg and .sql.
            r_gpkg = svc.get(qgis_export_url)
            assert r_gpkg.status_code == 200, r_gpkg.text
            assert r_gpkg.headers["Cache-Control"] == "no-store"
            assert r_gpkg.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
            assert r_gpkg.headers["X-Content-SHA256"] == hashlib.sha256(r_gpkg.content).hexdigest()
            members = inspect_geopackage(r_gpkg.content)
            assert len(members) == 1
            assert members[0].detected_crs == 5899
            assert members[0].feature_count == 4

            r_sql = svc.get(postgis_export_url)
            assert r_sql.status_code == 200, r_sql.text
            assert r_sql.headers["Cache-Control"] == "no-store"
            assert r_sql.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
            assert r_sql.headers["X-Content-SHA256"] == hashlib.sha256(r_sql.content).hexdigest()
            sql_text = r_sql.content.decode("utf-8")
            assert "ST_GeomFromText(" in sql_text
            assert ", 5899)" in sql_text

            # Upgrade principal to manager; the read-only credential ceiling must still apply.
            revoked_grant = owner.post(f"{_org(world)}/grants/{viewer_grant_id}/revoke")
            assert revoked_grant.status_code == 204, revoked_grant.text
            _grant(owner, world, "service_principal", principal_id, "manager")

            r_cat_mgr = svc.get(catalog_url)
            assert r_cat_mgr.status_code == 200, r_cat_mgr.text
            for adapter in r_cat_mgr.json()["adapters"]:
                _assert_read_ceiling_reflected(adapter["key"], adapter["actions"])

            r_qgis_mgr = svc.get(qgis_url)
            assert r_qgis_mgr.status_code == 200, r_qgis_mgr.text
            assert r_qgis_mgr.json()["default_write_policy"] == "deny"
            _assert_read_ceiling_reflected("qgis", r_qgis_mgr.json()["adapter"]["actions"])

            # Cross-org bearer request must fail.
            r_other_cat = svc.get(f"{other_root}/integration-hub/catalog")
            r_other_qgis = svc.get(f"{other_root}/integration-hub/adapters/qgis")
            r_other_export = svc.get(
                f"{other_root}/integration-hub/adapters/qgis/layers/{layer_id}/export"
            )
            assert r_other_cat.status_code in (401, 403, 404), r_other_cat.text
            assert r_other_qgis.status_code in (401, 403, 404), r_other_qgis.text
            assert r_other_export.status_code in (401, 403, 404), r_other_export.text

            # After credential revocation, bearer requests return 401.
            revoked = owner.post(
                f"{_org(world)}/service-principals/{principal_id}/credentials/{credential_id}/revoke"
            )
            assert revoked.status_code == 204, revoked.text
            assert svc.get(catalog_url).status_code == 401
            assert svc.get(qgis_url).status_code == 401
            assert svc.get(qgis_export_url).status_code == 401
            assert svc.get(postgis_export_url).status_code == 401


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

        allowed_qgis_export = (
            f"{root}/integration-hub/adapters/qgis/layers/{allowed_layer_id}/export"
        )
        allowed_postgis_export = (
            f"{root}/integration-hub/adapters/postgis/layers/{allowed_layer_id}/export"
        )
        hidden_qgis_export = (
            f"{root}/integration-hub/adapters/qgis/layers/{hidden_layer_id}/export"
        )

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

        # Scoped member without "export" gets 403 on allowed layer and 404 on hidden layer.
        assert member.get(allowed_qgis_export).status_code == 403
        assert member.get(hidden_qgis_export).status_code == 404

        # Replace rule with "export" scoped to feature_ids + attribute_filter -> only HVS-QN1.
        assert admin.delete(f"{root}/access/rules/{read_rule_id}").status_code == 204
        export_rule_resp = admin.post(
            f"{root}/access/rules",
            json={
                "user_id": str(world.member_user_id),
                "layer_id": allowed_layer_id,
                "actions": ["read", "geometry", "export"],
                "feature_ids": ["HVS-QN1", "HVS-QN2"],
                "attribute_filter": {"fld_feature_code": "HVS-QN1"},
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

        # GeoPackage export contains ONLY HVS-QN1 with exact EPSG:5899 preserved.
        r_gpkg = member.get(f"{allowed_qgis_export}?format=geopackage")
        assert r_gpkg.status_code == 200, r_gpkg.text
        assert r_gpkg.headers["Cache-Control"] == "no-store"
        assert r_gpkg.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
        assert r_gpkg.headers["X-OpenVietGIS-Feature-Count"] == "1"
        assert r_gpkg.headers["X-Content-SHA256"] == hashlib.sha256(r_gpkg.content).hexdigest()
        gpkg_members = inspect_geopackage(r_gpkg.content)
        assert len(gpkg_members) == 1
        assert gpkg_members[0].detected_crs == 5899
        parsed = parse_geopackage_member(r_gpkg.content, member_name=gpkg_members[0].name)
        assert parsed.detected_crs == 5899
        assert [feat.feature_id for feat in parsed.features] == ["HVS-QN1"]
        assert parsed.features[0].geometry.srid == 5899

        # PostGIS SQL export contains ONLY HVS-QN1 with exact SRID 5899.
        r_sql = member.get(f"{allowed_postgis_export}?format=postgis_sql")
        assert r_sql.status_code == 200, r_sql.text
        assert r_sql.headers["Cache-Control"] == "no-store"
        assert r_sql.headers["X-OpenVietGIS-CRS"] == "EPSG:5899"
        assert r_sql.headers["X-OpenVietGIS-Feature-Count"] == "1"
        assert r_sql.headers["X-Content-SHA256"] == hashlib.sha256(r_sql.content).hexdigest()
        sql_text = r_sql.content.decode("utf-8")
        assert "'HVS-QN1'" in sql_text
        assert ", 5899)" in sql_text
        for excluded_id in ("HVS-QN2", "HVS-QN3", "HVS-QN4"):
            assert excluded_id not in sql_text

        # GeoJSON on EPSG:5899 fails closed with 422 (refuses silent reprojection).
        r_geojson = member.get(f"{allowed_qgis_export}?format=geojson")
        assert r_geojson.status_code == 422, r_geojson.text

        # Hidden layer remains 404.
        assert member.get(hidden_qgis_export).status_code == 404

        # Revoke the rule, leaving member scoped with 0 rules.
        assert admin.delete(f"{root}/access/rules/{export_rule_id}").status_code == 204
        for adapter_url in (qgis_url, postgis_url):
            r_empty = member.get(adapter_url)
            assert r_empty.status_code == 200, r_empty.text
            assert r_empty.json()["scoped_access"] is True
            assert r_empty.json()["layers"] == []
        assert member.get(allowed_qgis_export).status_code == 404

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
