"""A path recovery preview respects actual parent IDs and never mutates the registry."""

import json
from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade
from modules.house_numbering.tests.integration.test_house_numbering_edge_acceptance import (
    _activate_house,
    _create_route,
)
from modules.house_numbering.tests.integration.test_house_numbering_runtime import (
    _seed_source,
)


def test_prefix_recovery(migrated_database):
    upgrade()
    world = seed_access_world()

    def seed(fid, wkt, family, attrs):
        sid = _seed_source(
            organization_id=world.organization_id,
            project_id=world.project_id,
            reference_key=f"prefix_{uuid4().hex[:8]}",
            family=family,
            feature_id=fid,
            wkt=wkt,
            extent={
                "min_x": 0,
                "min_y": 0,
                "max_x": 100,
                "max_y": 300,
                "crs_code": 3857,
            },
        )
        with get_admin_engine().begin() as conn:
            conn.execute(
                text(
                    "UPDATE spatial.reference_features SET attributes=CAST(:attrs AS jsonb) WHERE source_id=:sid"
                ),
                {"sid": sid, "attrs": json.dumps(attrs)},
            )
        return sid

    with _client(world.owner_user_id) as c:
        _, _, module, _ = _activate_house(c, world)
        root = _create_route(
            c,
            module,
            name="Đường A",
            source_id=seed(
                "root", "LINESTRING(0 0,100 0)", "LineString", {"ten": "Đường A"}
            ),
            feature_id="root",
        )
        _create_route(
            c,
            module,
            name="Đường A",
            source_id=seed(
                "other", "LINESTRING(0 200,100 200)", "LineString", {"ten": "Đường A"}
            ),
            feature_id="other",
        )
        parent = _create_route(
            c,
            module,
            name="Đường A hẻm 12",
            parent_route_id=root["id"],
            prefix="12",
            source_id=seed(
                "parent",
                "LINESTRING(20 0,20 50)",
                "LineString",
                {"ten": "Đường A hẻm 12"},
            ),
            feature_id="parent",
        )
        for fid, geom, branch in [
            ("a", "POINT(22 35)", "12/3"),
            ("b", "POINT(95 150)", "12/3"),
            ("c", "POINT(50 50)", "12/8/9"),
            ("d", "POINT(20 2)", "???"),
            ("e", "POINT(35 38)", "12/4"),
            ("f", "POINT(20 3)", "12/99"),
        ]:
            sid = seed(
                fid,
                geom,
                "Point",
                {"ten_duong": "Đường A", "hem": branch, "tuyen_id": root["id"]},
            )
            r = c.post(
                module + "/buildings",
                json={
                    "source_id": str(sid),
                    "feature_id": fid,
                    "eligibility": "eligible",
                },
            )
            assert r.status_code == 201, r.text
        for fid, geom, branch in [
            ("seg1", "LINESTRING(20 35,30 35)", "12/3"),
            ("seg2", "LINESTRING(30 35,40 35)", "12/3"),
            ("gap", "LINESTRING(50 50,70 50)", "12/8/9"),
            ("branch_a", "LINESTRING(20 40,30 40)", "12/4"),
            ("branch_b", "LINESTRING(50 45,60 45)", "12/4"),
        ]:
            seed(
                fid,
                geom,
                "LineString",
                {
                    "ten": "Đường A hẻm " + branch,
                    "prefix": branch,
                    "route_type": "alley",
                    "route_code": fid,
                    "review_reason": "parent_required",
                },
            )
        ep = module + "/building-assignment/prefix-recovery/preview"
        s = c.post(ep, json={})
        assert s.status_code == 200, s.text
        r = s.json()
        assert r["counts"]["total"] == 5, r
        by_path = {x["alley"]: x for x in r["items"]}
        assert (
            by_path["12/3"]["status"] == "connected_review"
            and by_path["12/3"]["near"] == 1
        )
        assert by_path["12/3"]["parent_candidates"][0]["route_id"] == parent["id"]
        assert by_path["12/3"]["chain"][0]["existing_count"] == 2
        assert by_path["12/8/9"]["status"] == "parent_missing"
        assert by_path["12/4"]["status"] == "ambiguous_geometry"
        assert by_path["12/99"]["status"] == "missing_geometry"
        result = c.post(ep, json={"search": "12/3", "limit": 1}).json()
        assert result["filtered_groups"] == 1 and result["items"][0]["alley"] == "12/3"
        with get_admin_engine().connect() as conn:
            assert (
                conn.execute(
                    text(
                        "SELECT count(*) FROM house_numbering.routes WHERE organization_id=:org"
                    ),
                    {"org": world.organization_id},
                ).scalar_one()
                == 3
            )
            assert (
                conn.execute(
                    text(
                        "SELECT count(*) FROM house_numbering.buildings WHERE organization_id=:org AND route_id IS NOT NULL"
                    ),
                    {"org": world.organization_id},
                ).scalar_one()
                == 0
            )
