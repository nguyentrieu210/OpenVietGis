from uuid import uuid4

from fastapi.testclient import TestClient

from openvietgis_api.adapters.persistence.db import get_engine, get_session_factory
from openvietgis_api.config import get_settings
from openvietgis_api.main import create_app
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def reset_runtime_state() -> None:
    if get_engine.cache_info().currsize:
        get_engine().dispose()
    get_session_factory.cache_clear()
    get_engine.cache_clear()
    get_settings.cache_clear()


def test_tenant_create_restart_read_path_ten_consecutive_runs(migrated_database: None) -> None:
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}/projects"
    with client_for_user(world.owner_user_id) as authenticated:
        cookie = authenticated.cookies.get("ovg_session")
        csrf = authenticated.headers["X-OpenVietGIS-CSRF"]
    assert cookie is not None
    for index in range(10):
        with TestClient(
            create_app(), cookies={"ovg_session": cookie}, headers={"X-OpenVietGIS-CSRF": csrf}
        ) as client:
            created = client.post(
                root,
                json={
                    "name": f"restart-check-{index}",
                    "slug": f"restart-{uuid4().hex}",
                },
            )
            assert created.status_code == 201
            record = created.json()

        reset_runtime_state()

        with TestClient(create_app(), cookies={"ovg_session": cookie}) as restarted_client:
            loaded = restarted_client.get(f"{root}/{record['id']}")
            assert loaded.status_code == 200
            assert loaded.json() == record
