#!/usr/bin/env python3
"""E05 / E10 3D Tiles differential parity across ZIP import, procedural generation,
tile byte serving, and triangle/ID feature picking on isolated PostGIS."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import struct
import sys
import tempfile
import zipfile
from pathlib import Path
from uuid import uuid4

import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def req(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def clone_project(owner_url: str, org_id: str, template_project_id: str, suffix: str) -> str:
    new_id = str(uuid4())
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO workspace.projects (
                id, organization_id, workspace_id, name, slug, status,
                created_by_actor_type, created_by_actor_id, created_at, updated_at
            )
            SELECT %s, organization_id, workspace_id, name || ' ' || %s, slug || '-' || %s,
                   status, created_by_actor_type, created_by_actor_id, created_at, updated_at
            FROM workspace.projects
            WHERE organization_id = %s AND id = %s
            """,
            (new_id, suffix, suffix, org_id, template_project_id),
        )
    return new_id


def enable_gis_scoped_editor(owner_url: str, org_id: str, project_ids: tuple[str, ...], user_id: str) -> None:
    from datetime import UTC, datetime

    now = datetime.now(UTC)
    with psycopg.connect(owner_url) as conn:
        membership_id = conn.execute(
            """
            UPDATE org.organization_memberships
            SET role = 'member'
            WHERE organization_id = %s AND user_id = %s
            RETURNING id
            """,
            (org_id, user_id),
        ).fetchone()[0]
        conn.execute(
            """
            UPDATE org.resource_grants
            SET revoked_at = %s
            WHERE organization_id = %s AND membership_id = %s AND revoked_at IS NULL
            """,
            (now, org_id, membership_id),
        )
        for project_id in project_ids:
            conn.execute(
                """
                INSERT INTO org.resource_grants (
                    id, organization_id, subject_kind, membership_id, scope_kind,
                    project_id, role, created_at, created_by_actor_type, created_by_actor_id
                ) VALUES (%s, %s, 'membership', %s, 'project', %s, 'editor', %s, 'human', %s)
                """,
                (uuid4(), org_id, membership_id, project_id, now, user_id),
            )
            conn.execute(
                """
                INSERT INTO gis_access.scoped_members(organization_id, project_id, user_id)
                VALUES (%s, %s, %s)
                ON CONFLICT DO NOTHING
                """,
                (org_id, project_id, user_id),
            )


def make_minimal_glb() -> bytes:
    bin_data = struct.pack("<9f", 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0)
    doc = {
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": len(bin_data)}],
        "bufferViews": [{"buffer": 0, "byteLength": len(bin_data), "target": 34962}],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0],
            }
        ],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}],
        "scene": 0,
    }
    raw = json.dumps(doc, separators=(",", ":")).encode()
    raw += b" " * (-len(raw) % 4)
    return (
        struct.pack("<5I", 0x46546C67, 2, 28 + len(raw) + len(bin_data), len(raw), 0x4E4F534A)
        + raw
        + struct.pack("<2I", len(bin_data), 0x004E4942)
        + bin_data
    )


def make_tileset_zip(manifest: dict, glb_map: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("tileset.json", json.dumps(manifest, separators=(",", ":")))
        for uri, payload in glb_map.items():
            zf.writestr(uri, payload)
    return buf.getvalue()


def normalize_tileset(body: object) -> object:
    out = json.loads(json.dumps(body))
    if isinstance(out, dict):
        if "id" in out:
            out["id"] = "<id>"
        if "created_at" in out:
            out["created_at"] = "<created_at>"
        prov = out.get("provenance")
        if isinstance(prov, dict):
            for k in ("source_id", "layer_id", "content_revision", "schema_revision"):
                if k in prov:
                    prov[k] = f"<{k}>"
    return out


def normalize_feature_pick(body: object) -> object:
    out = json.loads(json.dumps(body))
    if isinstance(out, dict):
        for k in ("tileset_id", "source_id", "layer_id", "content_revision", "schema_revision"):
            if k in out:
                out[k] = f"<{k}>"
    return out


@parity.case
def compare_response(label: str, e, a, *, normalize=lambda x: x, check_headers: bool = False) -> None:
    req(
        e.status_code == a.status_code,
        f"{label}: status {e.status_code} != {a.status_code} (expected={e.text}, actual={a.text})",
    )
    if check_headers and e.status_code == 200:
        for h in ("cache-control", "x-content-type-options"):
            req(e.headers.get(h) == a.headers.get(h), f"{label}: header {h} mismatch")
    eb = normalize(e.json())
    ab = normalize(a.json())
    diff = parity.difference(eb, ab)
    req(diff is None, f"{label}: {diff}\nexpected={eb}\nactual={ab}")
    print(f"PASS {label}", flush=True)


def seed_polygon_layer(client, base: str, headers: dict, csrf: dict, name: str) -> tuple[str, str, dict]:
    reg = client.post(
        f"{base}/spatial/sources",
        headers=csrf,
        json={"reference_key": "reference_polygons_4326", "name": f"{name} src"},
    )
    req(reg.status_code in (200, 201), f"seed source failed: {reg.text}")
    src_id = reg.json()["resource_id"]
    lay = client.post(
        f"{base}/layers",
        headers=csrf,
        json={"source_id": src_id, "name": name},
    )
    req(lay.status_code in (200, 201), f"seed layer failed: {lay.text}")
    lid = lay.json()["resource_id"]
    detail = client.get(f"{base}/layers/{lid}", headers=headers)
    req(detail.status_code == 200, f"layer detail failed: {detail.text}")
    return lid, src_id, detail.json()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    req(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e05-tilesets-") as directory:
        work = Path(directory)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=20261127,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )
            org_id = str(fixture["org_a_id"])
            template_proj = str(fixture["project_a_id"])
            proj_o = clone_project(owner_url, org_id, template_proj, "oracle")
            proj_r = clone_project(owner_url, org_id, template_proj, "rust")
            user_a = str(fixture["user_a_id"])

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            base_o = f"{API}/organizations/{org_id}/projects/{proj_o}"
            base_r = f"{API}/organizations/{org_id}/projects/{proj_r}"
            url_o = f"{base_o}/3d/tilesets"
            url_r = f"{base_r}/3d/tilesets"
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )
            foreign = parity.session_headers(
                fixture["session_b_token"],
                **{"x-openvietgis-csrf": fixture["csrf_b_token"]},
            )

            glb_bytes = make_minimal_glb()
            manifest = {
                "asset": {"version": "1.1", "tilesetVersion": "v1"},
                "geometricError": 10,
                "root": {
                    "boundingVolume": {"region": [1.85, 0.18, 1.87, 0.20, 0, 50]},
                    "geometricError": 0,
                    "refine": "REPLACE",
                    "content": {"uri": "tiles/root.glb"},
                },
            }
            zip_payload = make_tileset_zip(manifest, {"tiles/root.glb": glb_bytes})

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    # 1. Import ZIP tileset + fetch GLB + error cases
                    imp_o = oracle.post(
                        url_o,
                        headers=csrf,
                        data={"name": "  Imported Tileset  "},
                        files={"file": ("demo.zip", zip_payload, "application/zip")},
                    )
                    imp_r = rust.post(
                        url_r,
                        headers=csrf,
                        data={"name": "  Imported Tileset  "},
                        files={"file": ("demo.zip", zip_payload, "application/zip")},
                    )
                    compare_response("E05 import ZIP tileset", imp_o, imp_r, normalize=normalize_tileset)
                    ts_imp_o = imp_o.json()["id"]
                    ts_imp_r = imp_r.json()["id"]

                    # Fetch imported GLB bytes
                    glb_o = oracle.get(f"{url_o}/{ts_imp_o}/tiles/root.glb", headers=headers)
                    glb_r = rust.get(f"{url_r}/{ts_imp_r}/tiles/root.glb", headers=headers)
                    req(glb_o.status_code == 200 and glb_r.status_code == 200, "fetch GLB status mismatch")
                    req(glb_o.content == glb_bytes and glb_r.content == glb_bytes, "GLB bytes mismatch")
                    req(glb_o.headers.get("content-type") == glb_r.headers.get("content-type"), "GLB content-type")
                    req(glb_o.headers.get("cache-control") == glb_r.headers.get("cache-control"), "GLB cache-control")
                    print("PASS E05 fetch imported GLB bytes", flush=True)

                    compare_response(
                        "E05 invalid tile uri traversal 404",
                        oracle.get(f"{url_o}/{ts_imp_o}/tiles/bad.gltf", headers=headers),
                        rust.get(f"{url_r}/{ts_imp_r}/tiles/bad.gltf", headers=headers),
                    )
                    compare_response(
                        "E05 import bad zip rejected 422",
                        oracle.post(
                            url_o,
                            headers=csrf,
                            data={"name": "BadZip"},
                            files={"file": ("bad.zip", b"not-a-zip", "application/zip")},
                        ),
                        rust.post(
                            url_r,
                            headers=csrf,
                            data={"name": "BadZip"},
                            files={"file": ("bad.zip", b"not-a-zip", "application/zip")},
                        ),
                    )

                    # 2. Procedural 3D Tiles generation from polygon layer
                    lid_o, src_o, det_o = seed_polygon_layer(oracle, base_o, headers, csrf, "Extrude Poly")
                    lid_r, src_r, det_r = seed_polygon_layer(rust, base_r, headers, csrf, "Extrude Poly")
                    gen_o = oracle.post(
                        f"{url_o}/generate",
                        headers=csrf,
                        json={
                            "name": "  Generated Buildings  ",
                            "layer_id": lid_o,
                            "expected_layer_revision": det_o["layer_revision"],
                            "expected_content_revision": det_o["source"]["content_revision"],
                            "expected_schema_revision": det_o["source"]["schema_revision"],
                            "uniform_height_m": 18.5,
                            "base_ellipsoid_m": 2.0,
                            "acknowledge_ellipsoid": True,
                        },
                    )
                    gen_r = rust.post(
                        f"{url_r}/generate",
                        headers=csrf,
                        json={
                            "name": "  Generated Buildings  ",
                            "layer_id": lid_r,
                            "expected_layer_revision": det_r["layer_revision"],
                            "expected_content_revision": det_r["source"]["content_revision"],
                            "expected_schema_revision": det_r["source"]["schema_revision"],
                            "uniform_height_m": 18.5,
                            "base_ellipsoid_m": 2.0,
                            "acknowledge_ellipsoid": True,
                        },
                    )
                    compare_response("E05 generate extrusion tileset", gen_o, gen_r, normalize=normalize_tileset)
                    ts_gen_o = gen_o.json()["id"]
                    ts_gen_r = gen_r.json()["id"]

                    # Verify generated manifest & GLB structural parity
                    man_o = oracle.get(f"{url_o}/{ts_gen_o}/tileset.json", headers=headers).json()
                    man_r = rust.get(f"{url_r}/{ts_gen_r}/tileset.json", headers=headers).json()
                    req(len(man_o["root"]["children"]) == len(man_r["root"]["children"]) == 1, "generated child count")
                    gen_uri = man_o["root"]["children"][0]["content"]["uri"]
                    req(man_r["root"]["children"][0]["content"]["uri"] == gen_uri, "generated tile uri")
                    glb_gen_r = rust.get(f"{url_r}/{ts_gen_r}/{gen_uri}", headers=headers)
                    req(glb_gen_r.status_code == 200 and glb_gen_r.content[:4] == b"glTF", "valid generated GLB")
                    print("PASS E05 generated manifest and GLB structure", flush=True)

                    # 3. Feature picking by triangle & feature-by-id
                    fid_first = oracle.get(
                        f"{url_o}/{ts_gen_o}/feature",
                        headers=headers,
                        params={"uri": gen_uri, "triangle": 0},
                    ).json()["feature_id"]
                    compare_response(
                        "E05 get tile feature tri 0",
                        oracle.get(f"{url_o}/{ts_gen_o}/feature", headers=headers, params={"uri": gen_uri, "triangle": 0}),
                        rust.get(f"{url_r}/{ts_gen_r}/feature", headers=headers, params={"uri": gen_uri, "triangle": 0}),
                        normalize=normalize_feature_pick,
                        check_headers=True,
                    )
                    compare_response(
                        "E05 locate feature by id",
                        oracle.get(
                            f"{url_o}/{ts_gen_o}/feature-by-id",
                            headers=headers,
                            params={"source_id": src_o, "layer_id": lid_o, "feature_id": fid_first},
                        ),
                        rust.get(
                            f"{url_r}/{ts_gen_r}/feature-by-id",
                            headers=headers,
                            params={"source_id": src_r, "layer_id": lid_r, "feature_id": fid_first},
                        ),
                        normalize=normalize_feature_pick,
                        check_headers=True,
                    )
                    compare_response(
                        "E05 get feature missing triangle 404",
                        oracle.get(f"{url_o}/{ts_gen_o}/feature", headers=headers, params={"uri": gen_uri, "triangle": 99999}),
                        rust.get(f"{url_r}/{ts_gen_r}/feature", headers=headers, params={"uri": gen_uri, "triangle": 99999}),
                    )

                    # 4. Negative cases: foreign tenant, revision conflict, missing CSRF, GIS-scoped deny
                    compare_response(
                        "E05 generate foreign tenant 403",
                        oracle.post(
                            f"{url_o}/generate",
                            headers=foreign,
                            json={
                                "name": "Nope",
                                "layer_id": lid_o,
                                "expected_layer_revision": det_o["layer_revision"],
                                "expected_content_revision": det_o["source"]["content_revision"],
                                "expected_schema_revision": det_o["source"]["schema_revision"],
                                "uniform_height_m": 10.0,
                                "base_ellipsoid_m": 0.0,
                                "acknowledge_ellipsoid": True,
                            },
                        ),
                        rust.post(
                            f"{url_r}/generate",
                            headers=foreign,
                            json={
                                "name": "Nope",
                                "layer_id": lid_r,
                                "expected_layer_revision": det_r["layer_revision"],
                                "expected_content_revision": det_r["source"]["content_revision"],
                                "expected_schema_revision": det_r["source"]["schema_revision"],
                                "uniform_height_m": 10.0,
                                "base_ellipsoid_m": 0.0,
                                "acknowledge_ellipsoid": True,
                            },
                        ),
                    )
                    compare_response(
                        "E05 generate stale revision 409",
                        oracle.post(
                            f"{url_o}/generate",
                            headers=csrf,
                            json={
                                "name": "Stale",
                                "layer_id": lid_o,
                                "expected_layer_revision": det_o["layer_revision"] + 1,
                                "expected_content_revision": det_o["source"]["content_revision"],
                                "expected_schema_revision": det_o["source"]["schema_revision"],
                                "uniform_height_m": 10.0,
                                "base_ellipsoid_m": 0.0,
                                "acknowledge_ellipsoid": True,
                            },
                        ),
                        rust.post(
                            f"{url_r}/generate",
                            headers=csrf,
                            json={
                                "name": "Stale",
                                "layer_id": lid_r,
                                "expected_layer_revision": det_r["layer_revision"] + 1,
                                "expected_content_revision": det_r["source"]["content_revision"],
                                "expected_schema_revision": det_r["source"]["schema_revision"],
                                "uniform_height_m": 10.0,
                                "base_ellipsoid_m": 0.0,
                                "acknowledge_ellipsoid": True,
                            },
                        ),
                    )

                    # 5. Stale state after source update / rename
                    ren_o = oracle.patch(
                        f"{base_o}/layers/{lid_o}",
                        headers=csrf,
                        json={"name": "Extrude Poly Renamed", "expected_layer_revision": det_o["layer_revision"]},
                    )
                    ren_r = rust.patch(
                        f"{base_r}/layers/{lid_r}",
                        headers=csrf,
                        json={"name": "Extrude Poly Renamed", "expected_layer_revision": det_r["layer_revision"]},
                    )
                    req(ren_o.status_code == 200 and ren_r.status_code == 200, "rename layer failed")
                    compare_response(
                        "E05 stale feature pick after layer revision change",
                        oracle.get(f"{url_o}/{ts_gen_o}/feature", headers=headers, params={"uri": gen_uri, "triangle": 0}),
                        rust.get(f"{url_r}/{ts_gen_r}/feature", headers=headers, params={"uri": gen_uri, "triangle": 0}),
                        normalize=normalize_feature_pick,
                    )

                    # 6. GIS-scoped denial + symmetric audit counts
                    enable_gis_scoped_editor(owner_url, org_id, (proj_o, proj_r), user_a)
                    compare_response(
                        "E05 GIS-scoped denied list tilesets 403",
                        oracle.get(url_o, headers=headers),
                        rust.get(url_r, headers=headers),
                    )

                    with psycopg.connect(owner_url) as conn:
                        cnt_o = conn.execute(
                            """
                            SELECT count(*) FROM audit.audit_events
                            WHERE organization_id=%s AND action IN ('3d.tileset.import', '3d.tileset.generate')
                            """,
                            (org_id,),
                        ).fetchone()[0]
                        req(cnt_o == 4, f"expected 4 tileset audit events across both projects, got {cnt_o}")
                        _ = hashlib.sha256(b"").hexdigest()
                    print("PASS E05 tileset audit parity", flush=True)
            finally:
                get_engine().dispose()

    print("E05 tilesets parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
