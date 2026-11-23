"""Validate actual HTTP payloads against independently generated R004 schemas."""

from __future__ import annotations

from pathlib import Path
from uuid import uuid4

import yaml
from jsonschema import Draft202012Validator
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_authorization import _root
from tests.integration.spatial.test_features import _setup
from tests.integration.spatial.test_sources import _client

ROOT = Path(__file__).resolve().parents[5]
CONTRACT = ROOT / "specs/004-map-workspace/contracts/map-workspace-api.yaml"


def _validate(schema: str, response) -> dict:
    assert response.status_code == 200, response.text
    assert response.headers.get("cache-control") == "no-store"
    resources = [
        (
            path.as_uri(),
            Resource.from_contents(
                yaml.safe_load(path.read_text(encoding="utf-8")),
                default_specification=DRAFT202012,
            ),
        )
        for path in (ROOT / "specs").glob("*/contracts/*-api.yaml")
    ]
    registry = Registry().with_resources(resources)
    payload = response.json()
    Draft202012Validator(
        {"$ref": CONTRACT.as_uri() + "#/components/schemas/" + schema},
        registry=registry,
    ).validate(payload)
    return payload


def test_real_map_receipts_history_and_pinned_render_payloads_conform(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = _root(world)
    with _client(world.owner_user_id) as client:
        virtual = _validate("ProjectMap", client.get(root + "/map"))
        assert virtual["configuration_revision"] == 0
        _, source, layer, pins = _setup(client, world, "reference_points_3857")
        operation = str(uuid4())
        intent = {
            "operation_id": operation,
            "expected_revision": 0,
            "layers": [
                {
                    "layer_id": layer,
                    "visible": True,
                    "style": {"color": "#102030", "opacity": 0.8, "width": 2, "radius": 5},
                }
            ],
            "basemap": "grid",
            "default_viewpoint": {
                "mode": "set",
                "value": {
                    "longitude": 107,
                    "latitude": 20,
                    "zoom": 8,
                    "bearing": 0,
                    "pitch": 0,
                    "crs": 4326,
                },
            },
        }
        receipt = _validate("MapSaveReceipt", client.put(root + "/map", json=intent))
        saved = _validate("ProjectMap", client.get(root + "/map"))
        assert saved["configuration_revision"] == receipt["configuration_revision"] == 1
        assert saved["map_id"] == virtual["map_id"] == receipt["map_id"]
        recovered = _validate(
            "MapSaveReceipt",
            client.get(root + "/map/operations/" + operation),
        )
        assert recovered == receipt
        history = _validate("MapHistoryPage", client.get(root + "/map/history"))
        assert len(history["items"]) == 1
        assert history["items"][0]["after_configuration"] == saved["configuration"]
        assert history["items"][0]["before_configuration"] is None
        viewport = _validate(
            "MapViewport",
            client.get(
                root + f"/layers/{layer}/viewport",
                params={**pins, "west": 100, "south": 5, "east": 115, "north": 25, "limit": 1},
            ),
        )
        assert viewport["context"]["source_id"] == source["id"]
        assert viewport["context"]["response_crs"]["code"] == 4326
        assert viewport["complete"] is False
        assert viewport["limitation"] == "feature_limit"
        assert len(viewport["items"]) == 1
        assert "attributes" not in viewport["items"][0]
        extent = _validate(
            "MapDisplayExtent",
            client.get(
                root + f"/layers/{layer}/display-extent",
                params=pins,
            ),
        )
        assert extent["context"] == viewport["context"]
        assert extent["state"] == "available" and len(extent["bounds"]) == 4
