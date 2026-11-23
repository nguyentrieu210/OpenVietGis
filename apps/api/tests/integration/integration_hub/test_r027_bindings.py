"""R027 draft connector registry: real tenant/RLS/CSRF acceptance on isolated PostGIS.

No outbound connections or writes to a GIS dataset occur.
"""

from __future__ import annotations

from uuid import uuid4

from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.main import create_app
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _create
from tests.integration.spatial.test_sources import _client, _register
from tests.integration.spatial.test_sources import _path as _source_path


def test_r027_draft_binding_is_scoped_and_non_executable(migrated_database: None) -> None:
    own = seed_access_world()
    foreign = seed_access_world()
    root = (
        f"/api/v1/organizations/{own.organization_id}/projects/{own.project_id}"
        "/integration-hub/bindings"
    )
    with _client(own.owner_user_id) as owner:
        source_id = _register(
            owner, _source_path(own), "reference_points_5899", "R027 synthetic"
        )["resource_id"]
        layer_id = _create(owner, own, source_id, "QGIS roundtrip draft")["resource_id"]
        with get_admin_engine().connect() as con:
            fields = con.execute(
                text("""SELECT field_id FROM spatial.source_fields
                        WHERE organization_id=:org AND project_id=:proj
                          AND source_id=:src ORDER BY field_id LIMIT 3"""),
                {"org": own.organization_id, "proj": own.project_id, "src": source_id},
            ).scalars().all()
        assert fields
        payload = {
            "layer_id": layer_id,
            "adapter_key": "qgis",
            "external_dataset_ref": f"fixture-{uuid4().hex[:8]}",
            "direction": "bidirectional_manual",
            "source_crs_code": 5899,
            "target_crs_code": 5899,
            "external_primary_key": "ovg_feature_id",
            "allowed_field_ids": fields,
        }
        created = owner.post(root, json=payload)
        assert created.status_code == 201, created.text
        data = created.json()
        assert data["state"] == "draft"
        assert data["connected"] is False
        assert data["writes_allowed"] is False
        assert data["deletes_allowed"] is False
        assert data["base_content_revision"]
        assert data["base_schema_revision"]
        assert "token" not in data and "password" not in data
        binding_id = data["id"]
        duplicate = owner.post(root, json=payload)
        assert duplicate.status_code == 409

        listed = owner.get(root)
        assert listed.status_code == 200
        assert binding_id in {row["id"] for row in listed.json()["items"]}

        wrong_crs = owner.post(root, json={**payload, "target_crs_code": 4326})
        assert wrong_crs.status_code == 422
        bad_field = owner.post(root, json={**payload, "allowed_field_ids": ["secret"]})
        assert bad_field.status_code == 422
        bad_url = owner.post(
            root, json={**payload, "external_dataset_ref": "https://evil.invalid/tok"}
        )
        assert bad_url.status_code == 422

        activate = owner.post(
            f"{root}/{binding_id}/lifecycle",
            json={"action": "activate", "expected_revision": 1},
        )
        assert activate.status_code == 422

        paused = owner.post(
            f"{root}/{binding_id}/lifecycle",
            json={"action": "pause", "expected_revision": 1},
        )
        assert paused.status_code == 200, paused.text
        assert paused.json()["state"] == "paused"
        assert paused.json()["revision"] == 2

        stale = owner.post(
            f"{root}/{binding_id}/lifecycle",
            json={"action": "revoke", "expected_revision": 1},
        )
        assert stale.status_code == 409

        revoked = owner.post(
            f"{root}/{binding_id}/lifecycle",
            json={"action": "revoke", "expected_revision": 2},
        )
        assert revoked.status_code == 200
        assert revoked.json()["state"] == "revoked"
        assert revoked.json()["writes_allowed"] is False

    with _client(foreign.owner_user_id) as outsider:
        result = outsider.get(root)
        assert result.status_code in (403, 404)
        result = outsider.post(root, json=payload)
        assert result.status_code in (403, 404)

    with TestClient(create_app()) as anonymous:
        assert anonymous.get(root).status_code == 401
        assert anonymous.post(root, json=payload).status_code == 401