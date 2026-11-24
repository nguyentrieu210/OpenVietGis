"""Acceptance of official module runtime identity and manifest pins."""

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_official_runtime_contract_is_pinned(migrated_database: None) -> None:
    import json

    from openvietgis_api.adapters.http.official_modules import official_manifest

    from modules.house_numbering.migrate import upgrade

    upgrade()

    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"
    shipped = official_manifest("house_numbering")
    assert shipped is not None
    with _client(world.owner_user_id) as owner:
        rejected = owner.post(f"{org}/extensions/manifests", json=shipped)
        assert rejected.status_code == 409, rejected.text
        official = owner.post(f"{org}/extensions/official/house_numbering/register")
        assert official.status_code == 201, official.text
        mid = official.json()["id"]
        repeat = owner.post(f"{org}/extensions/official/house_numbering/register")
        assert repeat.json()["id"] == mid
        activated = owner.post(
            f"{project}/extensions/activations", json={"manifest_id": mid}
        )
        assert activated.status_code == 201, activated.text
        listed = owner.get(f"{project}/extensions/activations").json()["items"]
        assert listed[0]["manifest_version"] == shipped["version"]
        assert listed[0]["runtime_compatible"] is True
        runtime = f"{project}/modules/house-numbering/routes"
        assert owner.get(runtime).status_code == 200
        # Simulate a legacy, same-version metadata collision in the isolated test database.
        with get_admin_engine().begin() as conn:
            conn.execute(
                text(
                    "UPDATE extensions.manifests SET capabilities=CAST(:caps AS jsonb) WHERE id=:id"
                ),
                {"caps": json.dumps(["different_capability"]), "id": mid},
            )
        assert (
            owner.post(
                f"{org}/extensions/official/house_numbering/register"
            ).status_code
            == 409
        )
        listed = owner.get(f"{project}/extensions/activations").json()["items"]
        assert listed[0]["runtime_compatible"] is False
        assert owner.get(runtime).status_code == 404
        assert (
            owner.post(
                f"{project}/extensions/activations", json={"manifest_id": mid}
            ).status_code
            == 409
        )
        disabled = owner.post(f"{project}/extensions/house_numbering/disable")
        assert disabled.status_code == 200, disabled.text
