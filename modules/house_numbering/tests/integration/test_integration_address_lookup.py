"""Authenticated third-party address lookup: real PostGIS and service credentials."""
from __future__ import annotations

from fastapi.testclient import TestClient
from openvietgis_api.main import create_app
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.service_identity.test_service_authorization import (
    _service_client,
)
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering
from modules.house_numbering.tests.integration.test_house_numbering_us6_us7 import (
    _seed_world,
)


def test_approved_addresses_only_and_credential_scope(migrated_database: None) -> None:
    migrate_house_numbering()
    world, other = seed_access_world(), seed_access_world()
    org, project, module, _, buildings = _seed_world(world)
    # A known synthetic draft must not appear in the integration projection.
    with get_admin_engine().begin() as conn:
        conn.execute(
            text("UPDATE house_numbering.address_numbers SET kind='proposed' WHERE building_id=:id"),
            {"id": buildings["f"]},
        )
    endpoint = f"{module}/integrations/v1/addresses"
    with TestClient(create_app()) as unauthenticated:
        assert unauthenticated.get(endpoint, params={"query": "10"}).status_code == 401
    with _client(world.owner_user_id) as owner:
        app = owner.post(f"{org}/service-principals", json={"name": "address-partner"})
        assert app.status_code == 201, app.text
        app_id = app.json()["id"]
        grant = owner.post(
            f"{org}/grants",
            json={"subject_kind": "service_principal", "subject_id": app_id,
                  "scope_kind": "project", "project_id": str(world.project_id), "role": "viewer"},
        )
        assert grant.status_code == 201, grant.text
        issued = owner.post(
            f"{org}/service-principals/{app_id}/credentials",
            json={"name": "readonly", "permission_ceiling": ["project.read"]},
        )
        assert issued.status_code == 201, issued.text
        secret = issued.json()["secret"]
        credential_id = issued.json()["id"]
        with _service_client(secret) as service:
            result = service.get(endpoint, params={"query": "10", "limit": 20})
            assert result.status_code == 200, result.text
            assert result.headers["cache-control"] == "no-store"
            data = result.json()
            assert data["crs"] == "EPSG:4326"
            assert {row["house_number"] for row in data["items"]} == {"100", "102", "104", "106", "108"}
            for row in data["items"]:
                assert set(row) == {"address_id", "house_number", "street", "area", "longitude", "latitude"}
                assert -180 <= row["longitude"] <= 180
                assert -90 <= row["latitude"] <= 90
            assert service.get(endpoint, params={"query": "%_"}).json()["items"] == []
            assert service.get(endpoint, params={"query": "1"}).status_code == 422
            denied = service.get(
                f"/api/v1/organizations/{other.organization_id}/projects/{other.project_id}"
                "/modules/house-numbering/integrations/v1/addresses",
                params={"query": "10"},
            )
            assert denied.status_code == 401
            assert service.get(
                f"{module}/buildings/{buildings['a']}/private-households", params={"purpose": "test"}
            ).status_code == 403
            assert service.get(f"{project}/layers").status_code == 200
        revoked = owner.post(f"{org}/service-principals/{app_id}/credentials/{credential_id}/revoke")
        assert revoked.status_code == 204
    with _service_client(secret) as service:
        assert service.get(endpoint, params={"query": "10"}).status_code == 401
