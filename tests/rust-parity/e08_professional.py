"""E08 professional GIS tools differential parity (/pro/geometry, /pro/topology, /pro/schema, /pro/tiles)."""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path
from uuid import uuid4

import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def approx_equal(a, b, tol=1e-7) -> bool:
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(approx_equal(a[k], b[k], tol) for k in a)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(approx_equal(x, y, tol) for x, y in zip(a, b))
    if isinstance(a, (int, float)) and isinstance(b, (int, float)) and not isinstance(a, bool) and not isinstance(b, bool):
        return abs(float(a) - float(b)) <= tol
    return type(a) is type(b) and a == b


def compare_json(label: str, expected, actual, *, tol=None):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text[:300]} rust={actual.text[:300]}",
    )
    # a case not named with an error status must succeed, or both runtimes rejecting it would "pass"
    if not re.search(r"\b[45]\d\d\b", label):
        require(expected.status_code < 400, f"{label}: oracle {expected.status_code}: {expected.text[:300]}")
    ebody = expected.json()
    abody = actual.json()
    if tol is not None:
        require(approx_equal(ebody, abody, tol), f"{label}: mismatch beyond tol={tol}; oracle={ebody}; rust={abody}")
    else:
        mismatch = parity.difference(ebody, abody)
        require(mismatch is None, f"{label}: {mismatch}; oracle={ebody}; rust={abody}")
    print(f"PASS {label}", flush=True)


# layer_id -> source_id; kept out of the selection because FastAPI Selection forbids extra fields
SOURCES: dict[str, str] = {}


def make_layer(client, project_root: str, csrf: dict, family: str, geoms: list, name: str):
    rec = client.post(
        f"{project_root}/layers/manual",
        headers=csrf,
        json={
            "operation_id": str(uuid4()),
            "name": name,
            "geometry_family": family,
            "fields": [{"field_id": "nm", "name": "Tên", "type": "text", "nullable": True}],
        },
    )
    require(rec.status_code == 201, rec.text)
    lid, sid = rec.json()["layer_id"], rec.json()["source_id"]
    s = client.get(f"{project_root}/layers/{lid}", headers=csrf).json()["source"]
    ids = [f"feat_{i}_{uuid4().hex[:8]}" for i in range(len(geoms))]
    if ids:
        cs = client.post(
            f"{project_root}/sources/{sid}/changesets",
            headers=csrf,
            json={
                "operation_id": str(uuid4()),
                "base_content_revision": s["content_revision"],
                "base_schema_revision": s["schema_revision"],
                "summary": f"seed {name}",
                "operations": [
                    {
                        "op_type": "create",
                        "feature_id": fid,
                        "geometry": {"type": family, "coordinates": g},
                        "properties": {"nm": "A"},
                    }
                    for fid, g in zip(ids, geoms, strict=True)
                ],
            },
        )
        require(cs.status_code in (200, 201), cs.text)
        s = client.get(f"{project_root}/layers/{lid}", headers=csrf).json()["source"]
    SOURCES[lid] = sid
    return {
        "layer_id": lid,
        "content_revision": s["content_revision"],
        "schema_revision": s["schema_revision"],
        "feature_ids": ids,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e08-pro-") as directory:
        work = Path(directory)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=20261126,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )

            user_id, mid, surveyor = parity.scoped_member(owner_url, fixture)
            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app
            from openvietgis_api.security.csrf import make_csrf_token

            project_root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}"
            )
            pro = f"{project_root}/pro"
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )
            foreign_csrf = parity.session_headers(
                fixture["session_b_token"],
                **{"x-openvietgis-csrf": fixture["csrf_b_token"]},
            )
            surveyor_csrf = parity.session_headers(
                surveyor,
                **{"x-openvietgis-csrf": make_csrf_token(surveyor, baseline.DEFAULT_CSRF_SECRET)},
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    line_a = make_layer(
                        oracle,
                        project_root,
                        csrf,
                        "LineString",
                        [[[106.5, 10.7], [106.51, 10.7], [106.51, 10.71]]],
                        "Line A",
                    )
                    line_b = make_layer(
                        oracle,
                        project_root,
                        csrf,
                        "LineString",
                        [[[106.505, 10.69], [106.505, 10.71]]],
                        "Line B",
                    )
                    poly = make_layer(
                        oracle,
                        project_root,
                        csrf,
                        "Polygon",
                        [[[[106.49, 10.69], [106.52, 10.69], [106.52, 10.72], [106.49, 10.72], [106.49, 10.69]]]],
                        "Poly 1",
                    )
                    poly2 = make_layer(
                        oracle,
                        project_root,
                        csrf,
                        "Polygon",
                        [[[[106.50, 10.70], [106.51, 10.70], [106.51, 10.71], [106.50, 10.71], [106.50, 10.70]]]],
                        "Poly 2",
                    )
                    overlapping = make_layer(
                        oracle,
                        project_root,
                        csrf,
                        "Polygon",
                        [
                            [[[106.49, 10.69], [106.52, 10.69], [106.52, 10.72], [106.49, 10.72], [106.49, 10.69]]],
                            [[[106.50, 10.70], [106.51, 10.70], [106.51, 10.71], [106.50, 10.71], [106.50, 10.70]]],
                        ],
                        "Overlapping",
                    )
                    pts = make_layer(
                        oracle,
                        project_root,
                        csrf,
                        "Point",
                        [[106.505, 10.705], [106.505, 10.705], [107.0, 10.7]],
                        "Points",
                    )

                    # Grant scoped surveyor edit access to line_a & read on pts
                    oracle.post(
                        f"{API}/organizations/{fixture['org_a_id']}/grants",
                        headers=csrf,
                        json={
                            "subject_kind": "membership",
                            "subject_id": str(mid),
                            "scope_kind": "project",
                            "project_id": fixture["project_a_id"],
                            "role": "editor",
                        },
                    )
                    oracle.post(
                        f"{project_root}/access/rules",
                        headers=csrf,
                        json={
                            "user_id": user_id,
                            "layer_id": line_a["layer_id"],
                            "actions": ["read", "geometry"],
                        },
                    )
                    oracle.post(
                        f"{project_root}/access/rules",
                        headers=csrf,
                        json={
                            "user_id": user_id,
                            "layer_id": pts["layer_id"],
                            "actions": ["read"],
                        },
                    )

                    # 1. All 17 /pro/geometry operations!
                    geom_cases = [
                        ("move_batch", {"operation": "move_batch", "primary": pts, "dx": 10, "dy": 20}),
                        ("copy", {"operation": "copy", "primary": line_a, "dx": 10, "dy": 4}),
                        ("offset", {"operation": "offset", "primary": line_a, "distance": 5}),
                        ("rotate", {"operation": "rotate", "primary": line_a, "angle": 30}),
                        ("scale", {"operation": "scale", "primary": line_a, "factor": 1.2}),
                        ("stretch", {"operation": "stretch", "primary": line_a, "dx": 5, "vertex": 1}),
                        ("extend", {"operation": "extend", "primary": line_a, "distance": 10}),
                        ("trim", {"operation": "trim", "primary": line_a, "secondary": line_b}),
                        ("fillet", {"operation": "fillet", "primary": line_a, "distance": 5, "vertex": 1}),
                        ("simplify", {"operation": "simplify", "primary": line_a, "distance": 10}),
                        ("centroid", {"operation": "centroid", "primary": line_a}),
                        ("hull", {"operation": "hull", "primary": line_a}),
                        ("nearest", {"operation": "nearest", "primary": pts, "secondary": poly}),
                        ("spatial_join", {"operation": "spatial_join", "primary": pts, "secondary": poly}),
                        ("intersection", {"operation": "intersection", "primary": line_a, "secondary": poly}),
                        ("difference", {"operation": "difference", "primary": poly, "secondary": poly2}),
                        ("union", {"operation": "union", "primary": poly, "secondary": poly2}),
                    ]
                    for label, payload in geom_cases:
                        compare_json(
                            f"E08 pro/geometry {label}",
                            oracle.post(f"{pro}/geometry", headers=csrf, json=payload),
                            rust.post(f"{pro}/geometry", headers=csrf, json=payload),
                            tol=1e-9,
                        )

                    # Geometry negative & auth parity
                    ref_sel = {
                        "layer_id": fixture["layers"]["epsg4326_1k"]["layer_id"],
                        "content_revision": fixture["layers"]["epsg4326_1k"]["content_revision"],
                        "schema_revision": fixture["layers"]["epsg4326_1k"]["schema_revision"],
                        "feature_ids": ["poly-000001"],
                    }
                    neg_geom_cases = [
                        ("move_batch zero vector 422", csrf, {"operation": "move_batch", "primary": pts, "dx": 0, "dy": 0}),
                        ("stale content_revision 409", csrf, {"operation": "move_batch", "primary": pts | {"content_revision": "0" * 64}, "dx": 10}),
                        ("read-only reference fixture 403", csrf, {"operation": "copy", "primary": ref_sel, "dx": 10}),
                        ("scoped allowed single CAD 200", surveyor_csrf, {"operation": "copy", "primary": line_a, "dx": 10}),
                        ("scoped denied move_batch 403", surveyor_csrf, {"operation": "move_batch", "primary": line_a, "dx": 10}),
                        ("missing feature 404", csrf, {"operation": "copy", "primary": line_a | {"feature_ids": ["missing_id"]}, "dx": 10}),
                        ("offset non-LineString 422", csrf, {"operation": "offset", "primary": poly, "distance": 5}),
                        ("fillet non-interior vertex 422", csrf, {"operation": "fillet", "primary": line_a, "distance": 5, "vertex": 0}),
                        ("foreign tenant 404", foreign_csrf, {"operation": "copy", "primary": line_a, "dx": 10}),
                    ]
                    for label, hdrs, payload in neg_geom_cases:
                        compare_json(
                            f"E08 pro/geometry {label}",
                            oracle.post(f"{pro}/geometry", headers=hdrs, json=payload),
                            rust.post(f"{pro}/geometry", headers=hdrs, json=payload),
                            tol=1e-9,
                        )

                    # 2. All 6 /pro/topology checks!
                    # Also insert one invalid geometry row (geometry_state 'invalid': the table CHECK rejects an invalid 'present' geometry) directly via owner_url into a temporary manual layer to test "invalid"
                    inv_layer = make_layer(oracle, project_root, csrf, "Polygon", [], "Invalid Poly")
                    with psycopg.connect(owner_url) as conn:
                        conn.execute(
                            """
                            INSERT INTO spatial.reference_features (
                                organization_id, project_id, source_id, feature_id,
                                attributes, geometry, geometry_state, content_revision, schema_revision
                            ) VALUES (
                                %s, %s, %s, 'bowtie_1', '{"nm": "bad"}'::jsonb,
                                ST_GeomFromText('POLYGON((0 0, 1 1, 1 0, 0 1, 0 0))', 4326),
                                'invalid', %s, %s
                            )
                            """,
                            (
                                fixture["org_a_id"],
                                fixture["project_a_id"],
                                SOURCES[inv_layer["layer_id"]],
                                inv_layer["content_revision"],
                                inv_layer["schema_revision"],
                            ),
                        )

                    topo_cases = [
                        ("duplicate", {"check": "duplicate", "primary": pts | {"feature_ids": []}}),
                        ("invalid", {"check": "invalid", "primary": inv_layer | {"feature_ids": []}}),
                        ("overlap", {"check": "overlap", "primary": overlapping}),
                        ("dangling", {"check": "dangling", "primary": line_a}),
                        ("outside", {"check": "outside", "primary": pts | {"feature_ids": []}, "secondary": poly}),
                        ("gap", {"check": "gap", "primary": poly2, "secondary": poly}),
                    ]
                    for label, payload in topo_cases:
                        compare_json(
                            f"E08 pro/topology {label}",
                            oracle.post(f"{pro}/topology", headers=csrf, json=payload),
                            rust.post(f"{pro}/topology", headers=csrf, json=payload),
                        )

                    for label, hdrs, payload in [
                        ("scoped denied 403", surveyor_csrf, {"check": "duplicate", "primary": pts}),
                        ("overlap non-Polygon 422", csrf, {"check": "overlap", "primary": line_a}),
                        ("dangling non-LineString 422", csrf, {"check": "dangling", "primary": poly}),
                        ("outside missing secondary 422", csrf, {"check": "outside", "primary": pts}),
                    ]:
                        compare_json(
                            f"E08 pro/topology {label}",
                            oracle.post(f"{pro}/topology", headers=hdrs, json=payload),
                            rust.post(f"{pro}/topology", headers=hdrs, json=payload),
                        )

                    # 3. /pro/schema add & rename parity (test both runtimes on distinct layers, compare exact digest evolution!)
                    pts_o = make_layer(oracle, project_root, csrf, "Point", [[106.5, 10.7]], "Schema Oracle")
                    pts_r = make_layer(oracle, project_root, csrf, "Point", [[106.5, 10.7]], "Schema Rust")

                    add_o = oracle.post(
                        f"{pro}/schema",
                        headers=csrf,
                        json={"primary": pts_o, "action": "add", "field_id": "new_field", "name": "Mới", "type": "integer"},
                    )
                    add_r = rust.post(
                        f"{pro}/schema",
                        headers=csrf,
                        json={"primary": pts_r, "action": "add", "field_id": "new_field", "name": "Mới", "type": "integer"},
                    )
                    require(add_o.status_code == 200 and add_r.status_code == 200, f"add failed: {add_o.text} / {add_r.text}")
                    # Verify both produced the exact same schema_revision!
                    require(add_o.json()["schema_revision"] == add_r.json()["schema_revision"], "schema_revision digest mismatch")
                    print("PASS E08 pro/schema add + schema_revision digest parity", flush=True)

                    pts_o.update(content_revision=add_o.json()["content_revision"], schema_revision=add_o.json()["schema_revision"])
                    pts_r.update(content_revision=add_r.json()["content_revision"], schema_revision=add_r.json()["schema_revision"])

                    # Read back features on Rust to verify NULL attribute and revision pins
                    f_r = rust.get(
                        f"{project_root}/layers/{pts_r['layer_id']}/features",
                        headers=headers,
                        params={"content_revision": pts_r["content_revision"], "schema_revision": pts_r["schema_revision"]},
                    )
                    require(f_r.status_code == 200 and f_r.json()["items"][0]["attributes"]["new_field"] is None, f_r.text)

                    ren_o = oracle.post(
                        f"{pro}/schema",
                        headers=csrf,
                        json={"primary": pts_o, "action": "rename", "field_id": "new_field", "name": "Đổi tên"},
                    )
                    ren_r = rust.post(
                        f"{pro}/schema",
                        headers=csrf,
                        json={"primary": pts_r, "action": "rename", "field_id": "new_field", "name": "Đổi tên"},
                    )
                    require(ren_o.status_code == 200 and ren_r.status_code == 200, f"rename failed: {ren_o.text} / {ren_r.text}")
                    require(ren_o.json()["schema_revision"] == ren_r.json()["schema_revision"], "rename schema_revision mismatch")
                    print("PASS E08 pro/schema rename + schema_revision digest parity", flush=True)

                    pts_o.update(content_revision=ren_o.json()["content_revision"], schema_revision=ren_o.json()["schema_revision"])
                    pts_r.update(content_revision=ren_r.json()["content_revision"], schema_revision=ren_r.json()["schema_revision"])

                    # Negative /pro/schema cases
                    for label, hdrs, payload in [
                        ("duplicate name 422", csrf, {"primary": pts_o, "action": "add", "field_id": "f2", "name": "Đổi tên"}),
                        ("existing field_id 422", csrf, {"primary": pts_o, "action": "add", "field_id": "new_field", "name": "Other"}),
                        ("unchanged rename 422", csrf, {"primary": pts_o, "action": "rename", "field_id": "new_field", "name": "Đổi tên"}),
                        ("scoped denied 403", surveyor_csrf, {"primary": pts_o, "action": "add", "field_id": "f3", "name": "Scoped"}),
                        ("reference fixture 403", csrf, {"primary": ref_sel, "action": "add", "field_id": "f3", "name": "Ref"}),
                        ("stale revision 409", csrf, {"primary": pts_o | {"schema_revision": "0" * 64}, "action": "add", "field_id": "f3", "name": "Stale"}),
                    ]:
                        compare_json(
                            f"E08 pro/schema {label}",
                            oracle.post(f"{pro}/schema", headers=hdrs, json=payload),
                            rust.post(f"{pro}/schema", headers=hdrs, json=payload),
                        )

                    # 4. /pro/tiles/{layer_id}/{z}/{x}/{y}.mvt parity (4326 + 3857 + ETag/304 + scoped)
                    tile_url = f"{pro}/tiles/{pts['layer_id']}/0/0/0.mvt"
                    tile_pins = {"content_revision": pts["content_revision"], "schema_revision": pts["schema_revision"]}
                    to = oracle.get(tile_url, headers=headers, params=tile_pins)
                    tr = rust.get(tile_url, headers=headers, params=tile_pins)
                    require(to.status_code == 200 and tr.status_code == 200, f"tile 4326 status {to.status_code}/{tr.status_code}")
                    require(to.headers["content-type"] == tr.headers["content-type"], "tile content-type mismatch")
                    require(to.headers["cache-control"] == tr.headers["cache-control"] == "private, no-cache", "tile cache-control mismatch")
                    require(to.headers["etag"] == tr.headers["etag"], f"tile etag mismatch: {to.headers['etag']} != {tr.headers['etag']}")
                    require(b"features" in to.content and b"features" in tr.content, "tile missing features layer")
                    print("PASS E08 pro/tiles 4326 + exact ETag parity", flush=True)

                    # Conditional 304
                    etag = tr.headers["etag"]
                    c304_o = oracle.get(tile_url, headers=headers | {"if-none-match": etag}, params=tile_pins)
                    c304_r = rust.get(tile_url, headers=headers | {"if-none-match": etag}, params=tile_pins)
                    require(c304_o.status_code == 304 and c304_r.status_code == 304 and c304_r.content == b"", "tile 304 mismatch")
                    print("PASS E08 pro/tiles 304 Not Modified", flush=True)

                    # Scoped tile: allowed, private, no-store, no etag
                    sc_o = oracle.get(tile_url, headers=surveyor_csrf, params=tile_pins)
                    sc_r = rust.get(tile_url, headers=surveyor_csrf, params=tile_pins)
                    require(sc_o.status_code == 200 and sc_r.status_code == 200, f"scoped tile {sc_o.status_code}/{sc_r.status_code}")
                    require(sc_o.headers["cache-control"] == sc_r.headers["cache-control"] == "private, no-store", "scoped tile cache-control mismatch")
                    require("etag" not in sc_r.headers, "scoped tile must not emit etag")
                    print("PASS E08 pro/tiles scoped actor allowed with no-store", flush=True)

                    # Register native 3857 fixture via oracle and test tile on both
                    reg = oracle.post(
                        f"{project_root}/spatial/sources",
                        headers=csrf,
                        json={"reference_key": "reference_points_3857", "name": "Ref 3857"},
                    )
                    require(reg.status_code == 201, reg.text)
                    src_3857_id = reg.json()["resource_id"]
                    src_3857 = oracle.get(f"{project_root}/spatial/sources/{src_3857_id}", headers=headers).json()
                    lyr_3857 = oracle.post(
                        f"{project_root}/layers",
                        headers=csrf,
                        json={"source_id": src_3857_id, "name": "Layer 3857"},
                    )
                    require(lyr_3857.status_code == 201, lyr_3857.text)
                    lid_3857 = lyr_3857.json()["resource_id"]
                    pins_3857 = {"content_revision": src_3857["content_revision"], "schema_revision": src_3857["schema_revision"]}
                    t3857_o = oracle.get(f"{pro}/tiles/{lid_3857}/0/0/0.mvt", headers=headers, params=pins_3857)
                    t3857_r = rust.get(f"{pro}/tiles/{lid_3857}/0/0/0.mvt", headers=headers, params=pins_3857)
                    require(t3857_o.status_code == 200 and t3857_r.status_code == 200 and b"features" in t3857_r.content, "3857 tile failed")
                    print("PASS E08 pro/tiles native 3857 parity", flush=True)

                    # Tile negative cases
                    compare_json(
                        "E08 pro/tiles stale pin 409",
                        oracle.get(tile_url, headers=headers, params=tile_pins | {"content_revision": "0" * 64}),
                        rust.get(tile_url, headers=headers, params=tile_pins | {"content_revision": "0" * 64}),
                    )
                    compare_json(
                        "E08 pro/tiles invalid zoom 422",
                        oracle.get(f"{pro}/tiles/{pts['layer_id']}/23/0/0.mvt", headers=headers, params=tile_pins),
                        rust.get(f"{pro}/tiles/{pts['layer_id']}/23/0/0.mvt", headers=headers, params=tile_pins),
                    )
                    compare_json(
                        "E08 pro/tiles foreign tenant 404",
                        oracle.get(tile_url, headers=foreign_csrf, params=tile_pins),
                        rust.get(tile_url, headers=foreign_csrf, params=tile_pins),
                    )
            finally:
                get_engine().dispose()

    print("E08 professional GIS parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
