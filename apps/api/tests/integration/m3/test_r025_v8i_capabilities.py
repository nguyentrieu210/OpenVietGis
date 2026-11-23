"""R025 capability endpoint must enforce actual project authorization."""

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_v8i_capabilities_project_isolation(migrated_database):
    world = seed_access_world()
    path = (
        f"/api/v1/organizations/{world.organization_id}"
        f"/projects/{world.project_id}/integrations/v8i/capabilities"
    )
    with _client(world.owner_user_id) as owner:
        response = owner.get(path)
        assert response.status_code == 200, response.text
        assert response.json()["live_connection"] is False
    with _client(world.wrong_user_id) as outsider:
        response = outsider.get(path)
        assert response.status_code in (403, 404), response.text
