import base64
from io import BytesIO

from fastapi.testclient import TestClient
from PIL import Image

from openvietgis_api.main import create_app
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_account_avatar_private_and_validated(migrated_database):
    world = seed_access_world()
    path = "/api/v1/me/avatar"
    b = BytesIO()
    Image.new("RGB", (192, 192), "teal").save(b, format="JPEG")
    value = "data:image/jpeg;base64," + base64.b64encode(b.getvalue()).decode()
    with (
        client_for_user(world.owner_user_id) as owner,
        client_for_user(world.member_user_id) as member,
    ):
        assert owner.get(path).json() == {"data_url": None}
        r = owner.put(path, json={"data_url": value})
        assert r.status_code == 200, r.text
        assert owner.get(path).json() == r.json()
        assert member.get(path).json() == {"data_url": None}
        assert member.put(path, json={"data_url": "data:image/jpeg;base64,aaaa"}).status_code == 422
        assert owner.delete(path).status_code == 204
        assert owner.get(path).json() == {"data_url": None}
        members = owner.get(f"/api/v1/organizations/{world.organization_id}/members")
        assert members.status_code == 200, members.text
        assert {m["display_name"] for m in members.json()} >= {"Owner", "Member"}
    with TestClient(create_app()) as anonymous:
        assert anonymous.get(path).status_code == 401
