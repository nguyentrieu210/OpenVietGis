"""Exercise the route import API against isolated test PostGIS."""

import json
from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import (
    _activate_house,
)
from modules.house_numbering.tests.integration.test_house_numbering_runtime import (
    _seed_source,
)


def test_route_hierarchy_preview_apply_and_reimport(migrated_database):
    migrate_house_numbering()
    world = seed_access_world()
    layers = []
    for code, name, parent, prefix, wkt in [
        ("A", "Main Street", "", "", "LINESTRING(0 0,100 0)"),
        ("B", "Alley 155", "A", "155", "LINESTRING(50 0,50 80)"),
        ("C", "Branch 155/3", "B", "3", "LINESTRING(50 40,100 40)"),
    ]:
        sid = _seed_source(
            organization_id=world.organization_id,
            project_id=world.project_id,
            reference_key=f"tree_{uuid4().hex[:8]}",
            family="LineString",
            feature_id=code,
            wkt=wkt,
            extent={
                "min_x": 0,
                "min_y": 0,
                "max_x": 100,
                "max_y": 100,
                "crs_code": 3857,
            },
        )
        with get_admin_engine().begin() as conn:
            conn.execute(
                text(
                    "UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"
                ),
                {
                    "sid": sid,
                    "attrs": json.dumps(
                        {"code": code, "name": name, "parent": parent, "prefix": prefix}
                    ),
                },
            )
        layers.append(
            {
                "source_id": str(sid),
                "default_type": "alley" if parent else "street",
                "mapping": {
                    "code": "code",
                    "name": "name",
                    "parent_code": "parent",
                    "prefix": "prefix",
                },
            }
        )
    with _client(world.owner_user_id) as client:
        _, _, module, _ = _activate_house(client, world)
        body = {"layers": list(reversed(layers))}
        missing_parent = {
            **body,
            "overrides": {
                f"{layers[1]['source_id']}:B": {"parent_code": "", "prefix": "155"}
            },
        }
        suggested = client.post(module + "/route-import/preview", json=missing_parent)
        assert suggested.status_code == 200, suggested.text
        incomplete = next(r for r in suggested.json()["items"] if r["code"] == "B")
        assert incomplete["state"] == "error"
        assert any(c["code"] == "A" for c in incomplete["parent_suggestions"]), (
            incomplete
        )
        preview = client.post(module + "/route-import/preview", json=body)
        assert preview.status_code == 200, preview.text
        plan = preview.json()
        assert plan["ready"] == 3 and plan["errors"] == 0, plan
        assert plan["items"][-1]["resolved_prefix"] == "155/3"
        assert client.get(module + "/routes").json()["items"] == []
        stale = client.post(
            module + "/route-import/apply", json={**body, "preview_token": "0" * 64}
        )
        assert stale.status_code == 409, stale.text
        applied = client.post(
            module + "/route-import/apply",
            json={**body, "preview_token": plan["preview_token"]},
        )
        assert applied.status_code == 201, applied.text
        assert applied.json()["created"] == 3
        routes = {r["name"]: r for r in client.get(module + "/routes").json()["items"]}
        assert routes["Branch 155/3"]["parent_route_id"] == routes["Alley 155"]["id"]
        assert routes["Branch 155/3"]["prefix"] == "155/3"
        again = client.post(module + "/route-import/preview", json=body)
        assert again.status_code == 200, again.text
        assert again.json()["existing"] == 3 and again.json()["errors"] == 0, again.text
