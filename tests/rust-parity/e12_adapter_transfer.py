"""E12 Adapter Transfer differential parity: layer export and QGIS GeoPackage sync preview/push."""

from __future__ import annotations

import argparse
import hashlib
import io
import os
import sqlite3
import struct
import sys
import tempfile
from pathlib import Path
from uuid import uuid4

import ezdxf

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import e13_field_offline as offline_helpers
import run as parity

API = "/api/v1"


def require(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def _norm_preview(body):
    out = dict(body)
    for k in (
        "layer_id",
        "source_id",
        "base_content_revision",
        "current_content_revision",
        "base_schema_revision",
        "current_schema_revision",
        "diff_sha256",
    ):
        if k in out:
            out[k] = f"<{k}>"
    return out


def _norm_push(body):
    out = dict(body)
    for k in (
        "changeset_id",
        "source_id",
        "layer_id",
        "resulting_content_revision",
        "resulting_catalog_revision",
        "diff_sha256",
        "audit_event_id",
    ):
        if k in out:
            out[k] = f"<{k}>"
    return out


def _seed_projected_layer(client, base_url: str, csrf: dict[str, str], name: str) -> tuple[str, str, dict]:
    src = client.post(
        f"{base_url}/sources",
        headers=csrf,
        json={
            "name": f"{name} Source",
            "geometry_family": "Point",
            "dimensions": "XY",
            "source_crs_code": 3857,
            "stored_crs_code": 3857,
        },
    )
    require(src.status_code in (200, 201), f"create source failed: {src.status_code} {src.text}")
    sid = src.json()["id"]
    fld = client.post(
        f"{base_url}/sources/{sid}/fields",
        headers=csrf,
        json={"field_id": "fld_name", "name": "Name", "type": "text"},
    )
    require(fld.status_code in (200, 201), f"create field failed: {fld.status_code} {fld.text}")
    layer = client.post(
        f"{base_url}/layers",
        headers=csrf,
        json={"name": name, "source_id": sid},
    )
    require(layer.status_code in (200, 201), f"create layer failed: {layer.status_code} {layer.text}")
    lid = layer.json()["id"]
    cs = client.post(
        f"{base_url}/sources/{sid}/changesets",
        headers=csrf,
        json={
            "operation_id": f"seed-{uuid4().hex[:8]}",
            "base_content_revision": layer.json()["source"]["content_revision"],
            "summary": "Seed 2 points",
            "operations": [
                {
                    "op_type": "create",
                    "feature_id": "pt_1",
                    "geometry": {"type": "Point", "coordinates": [11872000.0, 1204000.0]},
                    "properties": {"fld_name": "Alpha"},
                },
                {
                    "op_type": "create",
                    "feature_id": "pt_2",
                    "geometry": {"type": "Point", "coordinates": [11872100.0, 1204100.0]},
                    "properties": {"fld_name": "Beta"},
                },
            ],
        },
    )
    require(cs.status_code in (200, 201), f"seed changeset failed: {cs.status_code} {cs.text}")
    detail = client.get(f"{base_url}/layers/{lid}", headers=csrf).json()["source"]
    return lid, sid, detail


def _mutate_gpkg_for_sync(raw_gpkg: bytes) -> bytes:
    """Modify pt_1 fld_name to AlphaEdited and insert pt_3 Create Point using GeoPackage GPB."""
    with tempfile.NamedTemporaryFile(suffix=".gpkg", delete=False) as tf:
        tf.write(raw_gpkg)
        path = Path(tf.name)
    try:
        conn = sqlite3.connect(str(path))
        try:
            table_name = conn.execute(
                "SELECT table_name FROM gpkg_contents WHERE data_type='features' LIMIT 1"
            ).fetchone()[0]
            quoted = '"' + table_name.replace('"', '""') + '"'
            conn.execute(
                f"UPDATE {quoted} SET fld_name = 'AlphaEdited' WHERE ovg_feature_id = 'pt_1'"
            )
            # Delete pt_2 locally -> omitted on server -> deletions_blocked should protect it
            conn.execute(f"DELETE FROM {quoted} WHERE ovg_feature_id = 'pt_2'")
            # Build GPB Point header (GP, version=0, flags=0x01 little-endian no envelope, srs_id=3857) + WKB Point
            gpb = (
                b"GP\x00\x01"
                + struct.pack("<i", 3857)
                + b"\x01"
                + struct.pack("<I", 1)
                + struct.pack("<dd", 11872250.0, 1204250.0)
            )
            conn.execute(
                f"INSERT INTO {quoted} (geom, ovg_feature_id, fld_name) VALUES (?, ?, ?)",
                (gpb, "pt_3", "Gamma"),
            )
            conn.commit()
        finally:
            conn.close()
        return path.read_bytes()
    finally:
        path.unlink(missing_ok=True)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--candidate", type=Path, required=True)
    args = ap.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file(), f"candidate binary not found: {binary}")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e12-transfer-") as tmp:
        work = Path(tmp)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            f = baseline.seed_benchmark_database(
                owner_url,
                seed=20261127,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )

            org_id = str(f["org_a_id"])
            template_proj = str(f["project_a_id"])
            proj_o = offline_helpers.clone_project(owner_url, org_id, template_proj, "oracle")
            proj_r = offline_helpers.clone_project(owner_url, org_id, template_proj, "rust")
            base_o = f"{API}/organizations/{org_id}/projects/{proj_o}"
            base_r = f"{API}/organizations/{org_id}/projects/{proj_r}"
            h_a = parity.session_headers(f["session_a_token"])
            csrf_a = parity.session_headers(
                f["session_a_token"], **{"x-openvietgis-csrf": f["csrf_a_token"]}
            )

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    lid_o, _sid_o, src_o = _seed_projected_layer(oracle, base_o, csrf_a, "Điểm Khảo Sát 1")
                    lid_r, _sid_r, src_r = _seed_projected_layer(rust, base_r, csrf_a, "Điểm Khảo Sát 1")

                    # 1. Error checks on export
                    offline_helpers.compare(
                        "E12 export unknown adapter 404",
                        oracle.get(f"{base_o}/integration-hub/adapters/missing/layers/{lid_o}/export", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/adapters/missing/layers/{lid_r}/export", headers=h_a),
                        expect_status=404,
                    )
                    offline_helpers.compare(
                        "E12 export disabled adapter 409",
                        oracle.get(f"{base_o}/integration-hub/adapters/ogc_services/layers/{lid_o}/export", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/adapters/ogc_services/layers/{lid_r}/export", headers=h_a),
                        expect_status=409,
                    )
                    offline_helpers.compare(
                        "E12 autocad non-dxf 422",
                        oracle.get(f"{base_o}/integration-hub/adapters/autocad/layers/{lid_o}/export?format=geojson", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/adapters/autocad/layers/{lid_r}/export?format=geojson", headers=h_a),
                        expect_status=422,
                    )

                    # 2. GeoPackage export + headers
                    gpkg_o = oracle.get(f"{base_o}/integration-hub/adapters/qgis/layers/{lid_o}/export?format=geopackage", headers=h_a)
                    gpkg_r = rust.get(f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/export?format=geopackage", headers=h_a)
                    require(gpkg_o.status_code == 200 and gpkg_r.status_code == 200, "gpkg export failed")
                    require(gpkg_r.content.startswith(b"SQLite format 3\x00"), "bad rust gpkg magic")
                    require(
                        gpkg_r.headers["x-content-sha256"] == hashlib.sha256(gpkg_r.content).hexdigest(),
                        "rust x-content-sha256 mismatch",
                    )
                    require(gpkg_r.headers["x-openvietgis-crs"] == "EPSG:3857", "bad crs header")
                    require(gpkg_r.headers["x-openvietgis-feature-count"] == "2", "bad feature count")
                    require(
                        "diem-khao-sat-1-qgis.gpkg" in gpkg_r.headers["content-disposition"],
                        f"unexpected content-disposition: {gpkg_r.headers['content-disposition']}",
                    )

                    # 3. GeoJSON export (3857 -> 4326)
                    gj_o = oracle.get(f"{base_o}/integration-hub/adapters/qgis/layers/{lid_o}/export?format=geojson", headers=h_a)
                    gj_r = rust.get(f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/export?format=geojson", headers=h_a)
                    offline_helpers.compare("E12 geojson export 200", gj_o, gj_r, expect_status=200)
                    require(gj_r.headers["x-openvietgis-crs"] == "EPSG:4326", "bad geojson crs header")

                    # 4. PostGIS SQL export
                    sql_o = oracle.get(f"{base_o}/integration-hub/adapters/postgis/layers/{lid_o}/export?format=postgis_sql", headers=h_a)
                    sql_r = rust.get(f"{base_r}/integration-hub/adapters/postgis/layers/{lid_r}/export?format=postgis_sql", headers=h_a)
                    require(sql_o.status_code == 200 and sql_r.status_code == 200, "postgis_sql export failed")
                    require(b"CREATE TABLE" in sql_r.content and b"ST_GeomFromText" in sql_r.content, "invalid postgis SQL")

                    # 5. AutoCAD DXF export + ezdxf roundtrip check
                    dxf_o = oracle.get(f"{base_o}/integration-hub/adapters/autocad/layers/{lid_o}/export?format=dxf", headers=h_a)
                    dxf_r = rust.get(f"{base_r}/integration-hub/adapters/autocad/layers/{lid_r}/export?format=dxf", headers=h_a)
                    require(dxf_o.status_code == 200 and dxf_r.status_code == 200, f"dxf export failed: {dxf_r.status_code} {dxf_r.text}")
                    require(dxf_r.headers.get("x-openvietgis-attribute-loss") == "true", "missing attribute loss header")
                    doc_r = ezdxf.read(io.StringIO(dxf_r.content.decode("utf-8")))
                    pts_r = [e for e in doc_r.modelspace() if e.dxftype() == "POINT"]
                    require(len(pts_r) == 2, f"expected 2 DXF POINT entities, got {len(pts_r)}")

                    # 6. sync-preview (create pt_3 + update pt_1 + omit pt_2 with deletions_blocked=True)
                    edited_o = _mutate_gpkg_for_sync(gpkg_o.content)
                    edited_r = _mutate_gpkg_for_sync(gpkg_r.content)
                    prev_o = oracle.post(
                        f"{base_o}/integration-hub/adapters/qgis/layers/{lid_o}/sync-preview",
                        headers=csrf_a,
                        files={"file": ("sync.gpkg", edited_o, "application/geopackage+sqlite3")},
                        data={
                            "base_content_revision": src_o["content_revision"],
                            "base_schema_revision": src_o["schema_revision"],
                        },
                    )
                    prev_r = rust.post(
                        f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/sync-preview",
                        headers=csrf_a,
                        files={"file": ("sync.gpkg", edited_r, "application/geopackage+sqlite3")},
                        data={
                            "base_content_revision": src_r["content_revision"],
                            "base_schema_revision": src_r["schema_revision"],
                        },
                    )
                    offline_helpers.compare("E12 sync-preview 200", prev_o, prev_r, norm=_norm_preview, expect_status=200)
                    require(prev_r.json()["create_count"] == 1 and prev_r.json()["update_count"] == 1, "bad preview counts")
                    require(prev_r.json()["omitted_server_feature_ids"] == ["pt_2"], "expected pt_2 omitted")

                    # 7. sync-push error cases: field-sync- prefix 422 & wrong diff sha 409
                    offline_helpers.compare(
                        "E12 sync-push reserved prefix 422",
                        oracle.post(
                            f"{base_o}/integration-hub/adapters/qgis/layers/{lid_o}/sync-push",
                            headers=csrf_a,
                            files={"file": ("sync.gpkg", edited_o, "application/geopackage+sqlite3")},
                            data={
                                "base_content_revision": src_o["content_revision"],
                                "base_schema_revision": src_o["schema_revision"],
                                "operation_id": "field-sync-forbidden",
                                "expected_diff_sha256": prev_o.json()["diff_sha256"],
                            },
                        ),
                        rust.post(
                            f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/sync-push",
                            headers=csrf_a,
                            files={"file": ("sync.gpkg", edited_r, "application/geopackage+sqlite3")},
                            data={
                                "base_content_revision": src_r["content_revision"],
                                "base_schema_revision": src_r["schema_revision"],
                                "operation_id": "field-sync-forbidden",
                                "expected_diff_sha256": prev_r.json()["diff_sha256"],
                            },
                        ),
                        expect_status=422,
                    )
                    offline_helpers.compare(
                        "E12 sync-push diff mismatch 409",
                        oracle.post(
                            f"{base_o}/integration-hub/adapters/qgis/layers/{lid_o}/sync-push",
                            headers=csrf_a,
                            files={"file": ("sync.gpkg", edited_o, "application/geopackage+sqlite3")},
                            data={
                                "base_content_revision": src_o["content_revision"],
                                "base_schema_revision": src_o["schema_revision"],
                                "operation_id": "qgis-bad-diff",
                                "expected_diff_sha256": "0" * 64,
                            },
                        ),
                        rust.post(
                            f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/sync-push",
                            headers=csrf_a,
                            files={"file": ("sync.gpkg", edited_r, "application/geopackage+sqlite3")},
                            data={
                                "base_content_revision": src_r["content_revision"],
                                "base_schema_revision": src_r["schema_revision"],
                                "operation_id": "qgis-bad-diff",
                                "expected_diff_sha256": "0" * 64,
                            },
                        ),
                        expect_status=409,
                    )

                    # 8. Valid sync-push 201
                    push_o = oracle.post(
                        f"{base_o}/integration-hub/adapters/qgis/layers/{lid_o}/sync-push",
                        headers=csrf_a,
                        files={"file": ("sync.gpkg", edited_o, "application/geopackage+sqlite3")},
                        data={
                            "base_content_revision": src_o["content_revision"],
                            "base_schema_revision": src_o["schema_revision"],
                            "operation_id": f"qgis-sync-{uuid4().hex[:8]}",
                            "expected_diff_sha256": prev_o.json()["diff_sha256"],
                        },
                    )
                    push_r = rust.post(
                        f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/sync-push",
                        headers=csrf_a,
                        files={"file": ("sync.gpkg", edited_r, "application/geopackage+sqlite3")},
                        data={
                            "base_content_revision": src_r["content_revision"],
                            "base_schema_revision": src_r["schema_revision"],
                            "operation_id": f"qgis-sync-{uuid4().hex[:8]}",
                            "expected_diff_sha256": prev_r.json()["diff_sha256"],
                        },
                    )
                    offline_helpers.compare("E12 sync-push 201", push_o, push_r, norm=_norm_push, expect_status=201)

                    # Verify server now exports 3 features (pt_1 updated, pt_2 preserved, pt_3 created)
                    after_gj = rust.get(f"{base_r}/integration-hub/adapters/qgis/layers/{lid_r}/export?format=geojson", headers=h_a)
                    require(after_gj.status_code == 200 and after_gj.headers["x-openvietgis-feature-count"] == "3", "expected 3 features after sync")
            finally:
                get_engine().dispose()

    print("E12 Adapter Transfer parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
