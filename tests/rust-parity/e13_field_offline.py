#!/usr/bin/env python3
"""E13 Field & Offline differential parity: package create, download, snapshot, revoke, and outbox sync."""

from __future__ import annotations

import argparse
import hashlib
import os
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


def require(cond: bool, msg: str) -> None:
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


def norm_pkg(body):
    if isinstance(body, dict) and "items" in body:
        return {"items": [norm_pkg(item) for item in body["items"]]}
    out = dict(body)
    for key in ("id", "sha256", "byte_size", "expires_at", "revoked_at", "created_at"):
        if key in out and out[key] is not None:
            out[key] = f"<{key}>"
    if isinstance(out.get("layer_pins"), list):
        pins = []
        for pin in out["layer_pins"]:
            p = dict(pin)
            for k in ("layer_id", "source_id", "content_revision", "schema_revision", "snapshot_sha256"):
                if k in p and p[k] is not None:
                    p[k] = f"<{k}>"
            pins.append(p)
        out["layer_pins"] = pins
    return out


def norm_sync(body):
    out = dict(body)
    for key in ("id", "package_id", "created_at"):
        if key in out and out[key] is not None:
            out[key] = f"<{key}>"
    if isinstance(out.get("applied_changeset_ids"), list):
        out["applied_changeset_ids"] = ["<changeset_id>" for _ in out["applied_changeset_ids"]]
    if isinstance(out.get("operation_receipts"), list):
        receipts = []
        for r in out["operation_receipts"]:
            item = dict(r)
            if item.get("core_feature_id") is not None:
                item["core_feature_id"] = "<core_feature_id>"
            if item.get("changeset_id") is not None:
                item["changeset_id"] = "<changeset_id>"
            receipts.append(item)
        out["operation_receipts"] = receipts
    if isinstance(out.get("conflicts"), list):
        confs = []
        for c in out["conflicts"]:
            item = dict(c)
            if isinstance(item.get("details"), dict):
                det = dict(item["details"])
                for k in ("expected_content_revision", "current_content_revision"):
                    if k in det and det[k] is not None:
                        det[k] = f"<{k}>"
                item["details"] = det
            confs.append(item)
        out["conflicts"] = confs
    return out


@parity.case
def compare(label: str, exp, act, *, norm=lambda x: x, expect_status: int | tuple[int, ...] | None = None):
    if expect_status is not None:
        statuses = (expect_status,) if isinstance(expect_status, int) else expect_status
        require(exp.status_code in statuses, f"{label}: unexpected oracle status {exp.status_code} ({exp.text})")
    require(
        exp.status_code == act.status_code,
        f"{label}: status {exp.status_code} != {act.status_code} (oracle={exp.text}, rust={act.text})",
    )
    eb = norm(exp.json())
    ab = norm(act.json())
    diff = parity.difference(eb, ab)
    require(diff is None, f"{label}: {diff}\noracle={eb}\nrust={ab}")
    print(f"PASS {label}", flush=True)
    return act


def seed_manual_point_layer(client, base: str, csrf: dict, name: str):
    res = client.post(
        f"{base}/layers/manual",
        headers=csrf,
        json={
            "operation_id": str(uuid4()),
            "name": name,
            "geometry_family": "Point",
            "fields": [{"field_id": "fld_name", "name": "Name", "type": "text", "nullable": False}],
        },
    )
    require(res.status_code == 201, f"create manual layer failed: {res.status_code} {res.text}")
    body = res.json()
    lid, sid = body["layer_id"], body["source_id"]
    src = client.get(f"{base}/layers/{lid}", headers=csrf).json()["source"]
    cs = client.post(
        f"{base}/sources/{sid}/changesets",
        headers=csrf,
        json={
            "operation_id": str(uuid4()),
            "base_content_revision": src["content_revision"],
            "base_schema_revision": src["schema_revision"],
            "summary": "seed point",
            "operations": [
                {
                    "op_type": "create",
                    "feature_id": "pt_1",
                    "geometry": {"type": "Point", "coordinates": [106.65, 10.75]},
                    "properties": {"fld_name": "Alpha"},
                }
            ],
        },
    )
    require(cs.status_code in (200, 201), f"seed changeset failed: {cs.status_code} {cs.text}")
    src = client.get(f"{base}/layers/{lid}", headers=csrf).json()["source"]
    return lid, sid, src


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--candidate", type=Path, required=True)
    args = ap.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file(), f"candidate binary not found: {binary}")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e13-parity-") as tmp:
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
            proj_o = clone_project(owner_url, org_id, template_proj, "oracle")
            proj_r = clone_project(owner_url, org_id, template_proj, "rust")

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
                    lid_o, sid_o, src_o = seed_manual_point_layer(oracle, base_o, csrf_a, "Field Pts")
                    lid_r, sid_r, src_r = seed_manual_point_layer(rust, base_r, csrf_a, "Field Pts")
                    bbox = {"min_x": 106.5, "min_y": 10.6, "max_x": 106.8, "max_y": 10.9}

                    # Negative create checks
                    compare(
                        "E13 create package unconfirmed 403",
                        oracle.post(
                            f"{base_o}/field/packages",
                            headers=csrf_a,
                            json={"name": "Bad", "layer_id": lid_o, "aoi_bbox": bbox, "local_storage_confirmed": False},
                        ),
                        rust.post(
                            f"{base_r}/field/packages",
                            headers=csrf_a,
                            json={"name": "Bad", "layer_id": lid_r, "aoi_bbox": bbox, "local_storage_confirmed": False},
                        ),
                        expect_status=403,
                    )
                    compare(
                        "E13 create package invalid bbox 422",
                        oracle.post(
                            f"{base_o}/field/packages",
                            headers=csrf_a,
                            json={"name": "BadBbox", "layer_id": lid_o, "aoi_bbox": {"min_x": 10.0, "max_x": 5.0, "min_y": 1.0, "max_y": 2.0}, "local_storage_confirmed": True},
                        ),
                        rust.post(
                            f"{base_r}/field/packages",
                            headers=csrf_a,
                            json={"name": "BadBbox", "layer_id": lid_r, "aoi_bbox": {"min_x": 10.0, "max_x": 5.0, "min_y": 1.0, "max_y": 2.0}, "local_storage_confirmed": True},
                        ),
                        expect_status=422,
                    )
                    compare(
                        "E13 create package bad ttl 422",
                        oracle.post(
                            f"{base_o}/field/packages",
                            headers=csrf_a,
                            json={"name": "BadTtl", "layer_id": lid_o, "aoi_bbox": bbox, "ttl_hours": 0, "local_storage_confirmed": True},
                        ),
                        rust.post(
                            f"{base_r}/field/packages",
                            headers=csrf_a,
                            json={"name": "BadTtl", "layer_id": lid_r, "aoi_bbox": bbox, "ttl_hours": 0, "local_storage_confirmed": True},
                        ),
                        expect_status=422,
                    )

                    # Create package (201)
                    pkg_o = oracle.post(
                        f"{base_o}/field/packages",
                        headers=csrf_a,
                        json={"name": "Field Pack 1", "layer_id": lid_o, "aoi_bbox": bbox, "ttl_hours": 48, "local_storage_confirmed": True},
                    )
                    pkg_r = rust.post(
                        f"{base_r}/field/packages",
                        headers=csrf_a,
                        json={"name": "Field Pack 1", "layer_id": lid_r, "aoi_bbox": bbox, "ttl_hours": 48, "local_storage_confirmed": True},
                    )
                    compare("E13 create package 201", pkg_o, pkg_r, norm=norm_pkg, expect_status=201)
                    pid_o = pkg_o.json()["id"]
                    pid_r = pkg_r.json()["id"]
                    require(pkg_r.json()["byte_size"] > 0 and len(pkg_r.json()["sha256"]) == 64, "invalid pkg metadata")

                    compare(
                        "E13 list packages 200",
                        oracle.get(f"{base_o}/field/packages", headers=h_a),
                        rust.get(f"{base_r}/field/packages", headers=h_a),
                        norm=norm_pkg,
                        expect_status=200,
                    )

                    dl_o = oracle.get(f"{base_o}/field/packages/{pid_o}/download", headers=h_a)
                    dl_r = rust.get(f"{base_r}/field/packages/{pid_r}/download", headers=h_a)
                    require(dl_o.status_code == dl_r.status_code == 200, "download status mismatch")
                    require(dl_r.content.startswith(b"SQLite format 3\x00"), "rust gpkg invalid sqlite header")
                    require(hashlib.sha256(dl_r.content).hexdigest() == pkg_r.json()["sha256"], "gpkg sha mismatch")
                    print("PASS E13 download gpkg verified", flush=True)

                    snap_o = oracle.get(f"{base_o}/field/packages/{pid_o}/snapshot", headers=h_a)
                    snap_r = rust.get(f"{base_r}/field/packages/{pid_r}/snapshot", headers=h_a)
                    require(snap_o.status_code == snap_r.status_code == 200, "snapshot status mismatch")
                    require(snap_r.json()["features"][0]["feature_id"] == "pt_1", "snapshot missing pt_1")
                    print("PASS E13 snapshot verified", flush=True)

                    # Sync: create point with client_operation_id + replay + operation_id conflict
                    client_op_id = str(uuid4())
                    op_id_1 = f"sync-{uuid4().hex[:8]}"
                    sync_payload_create = {
                        "operation_id": op_id_1,
                        "operations": [
                            {
                                "op_type": "create",
                                "feature_id": "local_pt_1",
                                "client_operation_id": client_op_id,
                                "geometry": {"type": "Point", "coordinates": [106.7, 10.8]},
                                "properties": {"fld_name": "FieldBeta"},
                            }
                        ],
                    }
                    s1_o = oracle.post(f"{base_o}/field/packages/{pid_o}/sync", headers=csrf_a, json=sync_payload_create)
                    s1_r = rust.post(f"{base_r}/field/packages/{pid_r}/sync", headers=csrf_a, json=sync_payload_create)
                    compare("E13 sync create point 201", s1_o, s1_r, norm=norm_sync, expect_status=201)
                    expected_core_r = "field-core-" + hashlib.sha256(
                        f"ovg-field-feature-v1:{f['user_a_id']}:{org_id}:{proj_r}:{pid_r}:{sid_r}:{client_op_id}".encode("ascii")
                    ).hexdigest()[:48]
                    require(
                        s1_r.json()["operation_receipts"][0]["core_feature_id"] == expected_core_r,
                        "rust core_feature_id digest mismatch",
                    )

                    # Replay
                    rep_o = oracle.post(f"{base_o}/field/packages/{pid_o}/sync", headers=csrf_a, json=sync_payload_create)
                    rep_r = rust.post(f"{base_r}/field/packages/{pid_r}/sync", headers=csrf_a, json=sync_payload_create)
                    compare("E13 sync idempotent replay 201", rep_o, rep_r, norm=norm_sync, expect_status=201)
                    require(rep_r.json()["id"] == s1_r.json()["id"], "replay batch id changed")

                    # Reuse operation_id with different content -> 409
                    conflict_payload = {
                        "operation_id": op_id_1,
                        "operations": [
                            {
                                "op_type": "update",
                                "feature_id": "pt_1",
                                "properties": {"fld_name": "Tampered"},
                            }
                        ],
                    }
                    compare(
                        "E13 sync op_id reuse conflict 409",
                        oracle.post(f"{base_o}/field/packages/{pid_o}/sync", headers=csrf_a, json=conflict_payload),
                        rust.post(f"{base_r}/field/packages/{pid_r}/sync", headers=csrf_a, json=conflict_payload),
                        expect_status=409,
                    )

                    # Out-of-AOI create -> 422
                    oob_payload = {
                        "operation_id": f"sync-oob-{uuid4().hex[:8]}",
                        "operations": [
                            {
                                "op_type": "create",
                                "feature_id": "local_oob",
                                "geometry": {"type": "Point", "coordinates": [108.0, 10.8]},
                                "properties": {"fld_name": "Outside"},
                            }
                        ],
                    }
                    compare(
                        "E13 sync out-of-AOI 422",
                        oracle.post(f"{base_o}/field/packages/{pid_o}/sync", headers=csrf_a, json=oob_payload),
                        rust.post(f"{base_r}/field/packages/{pid_r}/sync", headers=csrf_a, json=oob_payload),
                        expect_status=422,
                    )

                    # Stale content revision conflict -> 201 status='conflict'
                    upd_payload = {
                        "operation_id": f"sync-upd-{uuid4().hex[:8]}",
                        "operations": [
                            {
                                "op_type": "update",
                                "feature_id": "pt_1",
                                "properties": {"fld_name": "OfflineEdited"},
                            }
                        ],
                    }
                    st_o = oracle.post(f"{base_o}/field/packages/{pid_o}/sync", headers=csrf_a, json=upd_payload)
                    st_r = rust.post(f"{base_r}/field/packages/{pid_r}/sync", headers=csrf_a, json=upd_payload)
                    compare("E13 sync stale conflict 201", st_o, st_r, norm=norm_sync, expect_status=201)
                    require(st_r.json()["status"] == "conflict", f"expected conflict status, got {st_r.json()}")

                    # Revoke package -> 200, then sync -> 403 PACKAGE_INVALID
                    rv_o = oracle.post(f"{base_o}/field/packages/{pid_o}/revoke", headers=csrf_a)
                    rv_r = rust.post(f"{base_r}/field/packages/{pid_r}/revoke", headers=csrf_a)
                    compare("E13 revoke package 200", rv_o, rv_r, norm=norm_pkg, expect_status=200)

                    compare(
                        "E13 sync after revoke 403",
                        oracle.post(
                            f"{base_o}/field/packages/{pid_o}/sync",
                            headers=csrf_a,
                            json={"operation_id": f"after-{uuid4().hex[:8]}", "operations": [{"op_type": "update", "feature_id": "pt_1", "properties": {"fld_name": "Late"}}]},
                        ),
                        rust.post(
                            f"{base_r}/field/packages/{pid_r}/sync",
                            headers=csrf_a,
                            json={"operation_id": f"after-{uuid4().hex[:8]}", "operations": [{"op_type": "update", "feature_id": "pt_1", "properties": {"fld_name": "Late"}}]},
                        ),
                        expect_status=403,
                    )
            finally:
                get_engine().dispose()

    print("E13 Field & Offline parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
