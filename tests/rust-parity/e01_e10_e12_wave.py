#!/usr/bin/env python3
"""Consolidated differential parity for Mega-Wave:
- E01: POST /bootstrap/initialize + 10 gis_access routes
- E10: 2 Model Vault write routes + 10 CAD doc/version/download/links/conversions/COGO routes
- E12: export-scopes + 12 integration-hub binding/token/conflict/SSE routes
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import struct
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import httpx
import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def require(cond: bool, detail: str) -> None:
    if not cond:
        raise AssertionError(detail)


UUID_RE = re.compile(
    r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
    re.IGNORECASE,
)


def norm_value(val, *, scrub_ids: bool = True):
    if isinstance(val, dict):
        out = {}
        for k, v in val.items():
            if k in (
                "id",
                "binding_id",
                "layer_id",
                "source_id",
                "document_id",
                "group_id",
                "rule_id",
                "model_id",
                "token_id",
                "changeset_id",
                "created_at",
                "updated_at",
                "activated_at",
                "last_paired_at",
                "expires_at",
                "revoked_at",
                "resolved_at",
                "last_used_at",
                "content_revision",
                "schema_revision",
                "base_content_revision",
                "base_schema_revision",
                "token_prefix",
                "bearer_token",
                "pairing_code",
            ):
                if v is None:
                    out[k] = None
                else:
                    out[k] = f"<{k}>"
            else:
                out[k] = norm_value(v, scrub_ids=scrub_ids)
        return out
    if isinstance(val, list):
        return [norm_value(x, scrub_ids=scrub_ids) for x in val]
    if scrub_ids and isinstance(val, str) and UUID_RE.fullmatch(val):
        return "<uuid>"
    return val


@parity.case
def compare(label: str, exp, act, *, norm=None, expect_status: int | tuple[int, ...] | None = None):
    if expect_status is not None:
        statuses = (expect_status,) if isinstance(expect_status, int) else expect_status
        require(
            exp.status_code in statuses,
            f"{label}: unexpected oracle status {exp.status_code} ({exp.text})",
        )
    require(
        exp.status_code == act.status_code,
        f"{label}: status {exp.status_code} != {act.status_code} (oracle={exp.text}, rust={act.text})",
    )
    eb = exp.json()
    ab = act.json()
    if norm is not None:
        eb = norm(eb)
        ab = norm(ab)
    diff = parity.difference(eb, ab)
    require(diff is None, f"{label}: {diff}\noracle={eb}\nrust={ab}")
    print(f"PASS {label}", flush=True)
    return act


def make_glb() -> bytes:
    gltf = json.dumps({"asset": {"version": "2.0"}}, separators=(",", ":")).encode("utf-8")
    pad = (4 - (len(gltf) % 4)) % 4
    gltf_padded = gltf + b" " * pad
    total_len = 12 + 8 + len(gltf_padded)
    return (
        b"glTF"
        + struct.pack("<II", 2, total_len)
        + struct.pack("<I4s", len(gltf_padded), b"JSON")
        + gltf_padded
    )


def make_dxf() -> bytes:
    return (
        b"0\nSECTION\n2\nHEADER\n0\nENDSEC\n"
        b"0\nSECTION\n2\nENTITIES\n"
        b"0\nPOINT\n8\n0\n10\n100.0\n20\n200.0\n30\n0.0\n"
        b"0\nENDSEC\n0\nEOF\n"
    )


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


def add_org_member(owner_url: str, org_id: str, user_id: str, role: str = "member") -> str:
    mid = str(uuid4())
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO org.organization_memberships
              (id, organization_id, user_id, role, status, created_at, activated_at)
            VALUES (%s, %s, %s, %s, 'active', now(), now())
            ON CONFLICT (organization_id, user_id) DO NOTHING
            """,
            (mid, org_id, user_id, role),
        )
        conn.execute(
            """
            INSERT INTO org.resource_grants
              (id, organization_id, subject_kind, membership_id, scope_kind, role,
               created_at, created_by_actor_type, created_by_actor_id)
            SELECT %s, %s, 'membership', m.id, 'organization', 'editor', now(), 'human', m.user_id
            FROM org.organization_memberships m
            WHERE m.organization_id = %s AND m.user_id = %s
            """,
            (str(uuid4()), org_id, org_id, user_id),
        )
    return mid


def create_manual_layer_with_point(client, base: str, csrf: dict, name: str):
    payload = {
        "operation_id": f"op-{uuid4().hex[:8]}",
        "name": name,
        "geometry_family": "Point",
        "crs_code": 4326,
        "fields": [
            {"field_id": "fld_name", "display_name": "Name", "field_type": "text", "required": True},
        ],
    }
    resp = client.post(f"{base}/layers/manual", headers=csrf, json=payload)
    require(resp.status_code == 201, f"create manual layer failed: {resp.status_code} {resp.text}")
    body = resp.json()
    lid, sid = body["layer_id"], body["source_id"]
    cs = client.post(
        f"{base}/sources/{sid}/changesets",
        headers=csrf,
        json={
            "operation_id": f"seed-{uuid4().hex[:8]}",
            "base_content_revision": body["content_revision"],
            "base_schema_revision": body["schema_revision"],
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
    return lid, sid, cs.json()["id"]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--candidate", type=Path, required=True)
    args = ap.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file(), f"candidate binary not found: {binary}")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-wave-parity-") as tmp:
        work = Path(tmp)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            f = baseline.seed_benchmark_database(
                owner_url,
                seed=20261126,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )

            org_id = str(f["org_a_id"])
            proj_o = str(f["project_a_id"])
            proj_r = clone_project(owner_url, org_id, proj_o, "rust")
            # Add user_b as active member in org_a so we can assign GIS groups/rules cleanly.
            add_org_member(owner_url, org_id, str(f["user_b_id"]), role="member")

            base_o = f"{API}/organizations/{org_id}/projects/{proj_o}"
            base_r = f"{API}/organizations/{org_id}/projects/{proj_r}"
            h_a = parity.session_headers(f["session_a_token"])
            csrf_a = parity.session_headers(
                f["session_a_token"], **{"x-openvietgis-csrf": f["csrf_a_token"]}
            )
            h_b = parity.session_headers(f["session_b_token"])

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    # ------------------------------------------------------------------
                    # 1. E01: POST /bootstrap/initialize
                    # ------------------------------------------------------------------
                    bs_body = {"organization_name": "Extra Org", "organization_slug": "extra-org"}
                    compare(
                        "E01 bootstrap missing bearer 403",
                        oracle.post(f"{API}/bootstrap/initialize", headers=csrf_a, json=bs_body),
                        rust.post(f"{API}/bootstrap/initialize", headers=csrf_a, json=bs_body),
                        expect_status=403,
                    )
                    compare(
                        "E01 bootstrap wrong secret 403",
                        oracle.post(
                            f"{API}/bootstrap/initialize",
                            headers={**csrf_a, "authorization": "Bearer wrong-secret"},
                            json=bs_body,
                        ),
                        rust.post(
                            f"{API}/bootstrap/initialize",
                            headers={**csrf_a, "authorization": "Bearer wrong-secret"},
                            json=bs_body,
                        ),
                        expect_status=403,
                    )
                    compare(
                        "E01 bootstrap already initialized 409",
                        oracle.post(
                            f"{API}/bootstrap/initialize",
                            headers={
                                **csrf_a,
                                "authorization": f"Bearer {baseline.DEFAULT_BOOTSTRAP_SECRET}",
                            },
                            json=bs_body,
                        ),
                        rust.post(
                            f"{API}/bootstrap/initialize",
                            headers={
                                **csrf_a,
                                "authorization": f"Bearer {baseline.DEFAULT_BOOTSTRAP_SECRET}",
                            },
                            json=bs_body,
                        ),
                        expect_status=409,
                    )

                    # Seed identical manual editable layers on proj_o and proj_r
                    lid_o, sid_o, cs_o = create_manual_layer_with_point(oracle, base_o, csrf_a, "Parcels")
                    lid_r, sid_r, cs_r = create_manual_layer_with_point(rust, base_r, csrf_a, "Parcels")

                    # ------------------------------------------------------------------
                    # 2. E10: Model Vault writes (2 routes)
                    # ------------------------------------------------------------------
                    glb = make_glb()
                    placement = {
                        "source_id": sid_o,
                        "feature_id": "pt_1",
                        "scale": 1.5,
                        "rotation_deg": 45.0,
                        "offset_m": 2.0,
                        "dem_id": None,
                        "vertical_reference": "local",
                        "reference_label": "Bench",
                    }
                    placement_r = {**placement, "source_id": sid_r}
                    m_o = oracle.post(
                        f"{base_o}/3d/models",
                        headers=csrf_a,
                        files={"file": ("tower.glb", glb, "model/gltf-binary")},
                        data={"placement": json.dumps(placement)},
                    )
                    m_r = rust.post(
                        f"{base_r}/3d/models",
                        headers=csrf_a,
                        files={"file": ("tower.glb", glb, "model/gltf-binary")},
                        data={"placement": json.dumps(placement_r)},
                    )
                    compare("E10 create model 201", m_o, m_r, norm=norm_value, expect_status=201)
                    mid_o = m_o.json()["id"]
                    mid_r = m_r.json()["id"]

                    rev_o = {**placement, "expected_version": 1, "scale": 2.0}
                    rev_r = {**placement_r, "expected_version": 1, "scale": 2.0}
                    compare(
                        "E10 revise model placement 201",
                        oracle.post(f"{base_o}/3d/models/{mid_o}/placements", headers=csrf_a, json=rev_o),
                        rust.post(f"{base_r}/3d/models/{mid_r}/placements", headers=csrf_a, json=rev_r),
                        norm=norm_value,
                        expect_status=201,
                    )

                    # ------------------------------------------------------------------
                    # 3. E10: CAD documents CRUD / download / links / conversions / COGO (10 routes)
                    # ------------------------------------------------------------------
                    dxf = make_dxf()
                    doc_o = oracle.post(
                        f"{base_o}/cad/documents",
                        headers=csrf_a,
                        files={"file": ("plan.dxf", dxf, "application/dxf")},
                    )
                    doc_r = rust.post(
                        f"{base_r}/cad/documents",
                        headers=csrf_a,
                        files={"file": ("plan.dxf", dxf, "application/dxf")},
                    )
                    compare("E10 create cad document 201", doc_o, doc_r, norm=norm_value, expect_status=201)
                    did_o = doc_o.json()["document_id"]
                    did_r = doc_r.json()["document_id"]

                    v2_o = oracle.post(
                        f"{base_o}/cad/documents/{did_o}/versions",
                        headers=csrf_a,
                        files={"file": ("plan_v2.dxf", dxf, "application/dxf")},
                        data={"expected_version": "1"},
                    )
                    v2_r = rust.post(
                        f"{base_r}/cad/documents/{did_r}/versions",
                        headers=csrf_a,
                        files={"file": ("plan_v2.dxf", dxf, "application/dxf")},
                        data={"expected_version": "1"},
                    )
                    compare("E10 append cad version 201", v2_o, v2_r, norm=norm_value, expect_status=201)

                    dl_o = oracle.get(f"{base_o}/cad/documents/{did_o}/versions/1/download", headers=h_a)
                    dl_r = rust.get(f"{base_r}/cad/documents/{did_r}/versions/1/download", headers=h_a)
                    require(dl_o.status_code == dl_r.status_code == 200, "cad download status mismatch")
                    require(dl_o.content == dl_r.content == dxf, "cad download bytes mismatch")
                    print("PASS E10 download cad document version", flush=True)

                    compare(
                        "E10 cad links by feature",
                        oracle.get(f"{base_o}/cad/links/by-feature?source_id={sid_o}&feature_id=pt_1", headers=h_a),
                        rust.get(f"{base_r}/cad/links/by-feature?source_id={sid_r}&feature_id=pt_1", headers=h_a),
                        expect_status=200,
                    )
                    compare(
                        "E10 cad links by doc version",
                        oracle.get(f"{base_o}/cad/documents/{did_o}/versions/1/links?handle=1A", headers=h_a),
                        rust.get(f"{base_r}/cad/documents/{did_r}/versions/1/links?handle=1A", headers=h_a),
                        expect_status=200,
                    )
                    compare(
                        "E10 list cad conversions",
                        oracle.get(f"{base_o}/cad/conversions", headers=h_a),
                        rust.get(f"{base_r}/cad/conversions", headers=h_a),
                        expect_status=200,
                    )
                    compare(
                        "E10 cogo traverse",
                        oracle.post(
                            f"{base_o}/cad/cogo/traverse",
                            headers=csrf_a,
                            json={"origin": [0.0, 0.0], "distance": 100.0, "azimuth_degrees": 90.0},
                        ),
                        rust.post(
                            f"{base_r}/cad/cogo/traverse",
                            headers=csrf_a,
                            json={"origin": [0.0, 0.0], "distance": 100.0, "azimuth_degrees": 90.0},
                        ),
                        expect_status=200,
                    )
                    compare(
                        "E10 cogo intersection",
                        oracle.post(
                            f"{base_o}/cad/cogo/intersection",
                            headers=csrf_a,
                            json={"seg_a": [[0.0, 0.0], [10.0, 10.0]], "seg_b": [[0.0, 10.0], [10.0, 0.0]]},
                        ),
                        rust.post(
                            f"{base_r}/cad/cogo/intersection",
                            headers=csrf_a,
                            json={"seg_a": [[0.0, 0.0], [10.0, 10.0]], "seg_b": [[0.0, 10.0], [10.0, 0.0]]},
                        ),
                        expect_status=200,
                    )
                    compare(
                        "E10 cogo offset",
                        oracle.post(
                            f"{base_o}/cad/cogo/offset",
                            headers=csrf_a,
                            json={"segment": [[0.0, 0.0], [10.0, 0.0]], "offset_distance": 5.0},
                        ),
                        rust.post(
                            f"{base_r}/cad/cogo/offset",
                            headers=csrf_a,
                            json={"segment": [[0.0, 0.0], [10.0, 0.0]], "offset_distance": 5.0},
                        ),
                        expect_status=200,
                    )
                    compare(
                        "E10 cogo snap",
                        oracle.post(
                            f"{base_o}/cad/cogo/snap",
                            headers=csrf_a,
                            json={"target": [0.5, 0.0], "segments": [[[0.0, 0.0], [10.0, 0.0]]], "tolerance": 2.0},
                        ),
                        rust.post(
                            f"{base_r}/cad/cogo/snap",
                            headers=csrf_a,
                            json={"target": [0.5, 0.0], "segments": [[[0.0, 0.0], [10.0, 0.0]]], "tolerance": 2.0},
                        ),
                        expect_status=200,
                    )

                    # ------------------------------------------------------------------
                    # 4. E01: GIS Access (10 routes)
                    # ------------------------------------------------------------------
                    compare(
                        "E01 get_access initial",
                        oracle.get(f"{base_o}/access", headers=h_a),
                        rust.get(f"{base_r}/access", headers=h_a),
                        norm=norm_value,
                        expect_status=200,
                    )
                    grp_body = {"name": "Field Team", "user_ids": [str(f["user_b_id"])]}
                    grp_o = oracle.post(f"{base_o}/access/groups", headers=csrf_a, json=grp_body)
                    grp_r = rust.post(f"{base_r}/access/groups", headers=csrf_a, json=grp_body)
                    compare("E01 create access group 201", grp_o, grp_r, norm=norm_value, expect_status=201)
                    gid_o = grp_o.json()["id"]
                    gid_r = grp_r.json()["id"]

                    compare(
                        "E01 replace access group 200",
                        oracle.put(
                            f"{base_o}/access/groups/{gid_o}",
                            headers=csrf_a,
                            json={"name": "Field Team Updated", "user_ids": [str(f["user_b_id"])]},
                        ),
                        rust.put(
                            f"{base_r}/access/groups/{gid_r}",
                            headers=csrf_a,
                            json={"name": "Field Team Updated", "user_ids": [str(f["user_b_id"])]},
                        ),
                        norm=norm_value,
                        expect_status=200,
                    )

                    rule_o = oracle.post(
                        f"{base_o}/access/rules",
                        headers=csrf_a,
                        json={
                            "group_id": gid_o,
                            "layer_id": lid_o,
                            "actions": ["read", "export", "approve"],
                        },
                    )
                    rule_r = rust.post(
                        f"{base_r}/access/rules",
                        headers=csrf_a,
                        json={
                            "group_id": gid_r,
                            "layer_id": lid_r,
                            "actions": ["read", "export", "approve"],
                        },
                    )
                    compare("E01 create access rule 201", rule_o, rule_r, norm=norm_value, expect_status=201)
                    rid_o = rule_o.json()["id"]
                    rid_r = rule_r.json()["id"]

                    compare(
                        "E01 batch access rules 200",
                        oracle.post(
                            f"{base_o}/access/rules/batch",
                            headers=csrf_a,
                            json={
                                "rules": [
                                    {"group_id": gid_o, "layer_id": lid_o, "actions": ["read", "export", "approve"]}
                                ],
                                "revoke_ids": [],
                            },
                        ),
                        rust.post(
                            f"{base_r}/access/rules/batch",
                            headers=csrf_a,
                            json={
                                "rules": [
                                    {"group_id": gid_r, "layer_id": lid_r, "actions": ["read", "export", "approve"]}
                                ],
                                "revoke_ids": [],
                            },
                        ),
                        expect_status=200,
                    )

                    compare(
                        "E01 my access (scoped user)",
                        oracle.get(f"{base_o}/access/mine", headers=h_b),
                        rust.get(f"{base_r}/access/mine", headers=h_b),
                        norm=norm_value,
                        expect_status=200,
                    )
                    compare(
                        "E01 scoped geojson export",
                        oracle.get(f"{base_o}/access/export/{lid_o}?format=geojson", headers=h_b),
                        rust.get(f"{base_r}/access/export/{lid_r}?format=geojson", headers=h_b),
                        expect_status=200,
                    )
                    csv_o = oracle.get(f"{base_o}/access/export/{lid_o}?format=csv", headers=h_b)
                    csv_r = rust.get(f"{base_r}/access/export/{lid_r}?format=csv", headers=h_b)
                    require(csv_o.status_code == csv_r.status_code == 200, "csv export status")
                    require(csv_o.text == csv_r.text, f"csv mismatch: {csv_o.text!r} != {csv_r.text!r}")
                    print("PASS E01 scoped csv export", flush=True)

                    compare(
                        "E01 list reviews (scoped reviewer)",
                        oracle.get(f"{base_o}/access/reviews", headers=h_b),
                        rust.get(f"{base_r}/access/reviews", headers=h_b),
                        norm=norm_value,
                        expect_status=200,
                    )
                    # user_a (unscoped admin) cannot review own changeset (user_a committed cs_o/cs_r): 422
                    compare(
                        "E01 decide review own changeset 422",
                        oracle.post(
                            f"{base_o}/access/reviews/{cs_o}",
                            headers=csrf_a,
                            json={"decision": "approved", "note": "ok"},
                        ),
                        rust.post(
                            f"{base_r}/access/reviews/{cs_r}",
                            headers=csrf_a,
                            json={"decision": "approved", "note": "ok"},
                        ),
                        expect_status=422,
                    )

                    compare(
                        "E01 revoke access rule",
                        oracle.delete(f"{base_o}/access/rules/{rid_o}", headers=csrf_a),
                        rust.delete(f"{base_r}/access/rules/{rid_r}", headers=csrf_a),
                        expect_status=200,
                    )

                    # ------------------------------------------------------------------
                    # 5. E12: export-scopes + 12 integration binding routes
                    # ------------------------------------------------------------------
                    compare(
                        "E12 get export scopes",
                        oracle.get(f"{base_o}/integration-hub/export-scopes", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/export-scopes", headers=h_a),
                        expect_status=200,
                    )
                    compare(
                        "E12 list binding candidates",
                        oracle.get(f"{base_o}/integration-hub/bindings/candidates", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/bindings/candidates", headers=h_a),
                        norm=norm_value,
                        expect_status=200,
                    )
                    compare(
                        "E12 list bindings empty",
                        oracle.get(f"{base_o}/integration-hub/bindings", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/bindings", headers=h_a),
                        expect_status=200,
                    )

                    bind_req_o = {
                        "layer_id": lid_o,
                        "external_dataset_ref": "qgis_parcels_01",
                        "source_crs_code": 4326,
                        "target_crs_code": 4326,
                        "allowed_field_ids": ["fld_name"],
                    }
                    bind_req_r = {**bind_req_o, "layer_id": lid_r}
                    b_o = oracle.post(f"{base_o}/integration-hub/bindings", headers=csrf_a, json=bind_req_o)
                    b_r = rust.post(f"{base_r}/integration-hub/bindings", headers=csrf_a, json=bind_req_r)
                    compare("E12 create binding draft 201", b_o, b_r, norm=norm_value, expect_status=201)
                    bid_o = b_o.json()["id"]
                    bid_r = b_r.json()["id"]

                    compare(
                        "E12 duplicate binding 409",
                        oracle.post(f"{base_o}/integration-hub/bindings", headers=csrf_a, json=bind_req_o),
                        rust.post(f"{base_r}/integration-hub/bindings", headers=csrf_a, json=bind_req_r),
                        expect_status=409,
                    )
                    compare(
                        "E12 activate binding",
                        oracle.post(
                            f"{base_o}/integration-hub/bindings/{bid_o}/activate",
                            headers=csrf_a,
                            json={"expected_revision": 1},
                        ),
                        rust.post(
                            f"{base_r}/integration-hub/bindings/{bid_r}/activate",
                            headers=csrf_a,
                            json={"expected_revision": 1},
                        ),
                        norm=norm_value,
                        expect_status=200,
                    )
                    compare(
                        "E12 pause binding lifecycle",
                        oracle.post(
                            f"{base_o}/integration-hub/bindings/{bid_o}/lifecycle",
                            headers=csrf_a,
                            json={"action": "pause", "expected_revision": 2},
                        ),
                        rust.post(
                            f"{base_r}/integration-hub/bindings/{bid_r}/lifecycle",
                            headers=csrf_a,
                            json={"action": "pause", "expected_revision": 2},
                        ),
                        norm=norm_value,
                        expect_status=200,
                    )

                    tok_o = oracle.post(
                        f"{base_o}/integration-hub/bindings/{bid_o}/pairing-tokens",
                        headers=csrf_a,
                        json={"label": "QGIS Field Laptop", "expires_in_days": 30},
                    )
                    tok_r = rust.post(
                        f"{base_r}/integration-hub/bindings/{bid_r}/pairing-tokens",
                        headers=csrf_a,
                        json={"label": "QGIS Field Laptop", "expires_in_days": 30},
                    )
                    compare("E12 issue pairing token 201", tok_o, tok_r, norm=norm_value, expect_status=201)
                    tid_o = tok_o.json()["token"]["id"]
                    tid_r = tok_r.json()["token"]["id"]
                    code_r = tok_r.json()["pairing_code"]
                    require(code_r.startswith("ovg-pair-v1:"), f"bad pairing code: {code_r}")
                    decoded = json.loads(
                        base64.urlsafe_b64decode(code_r.removeprefix("ovg-pair-v1:") + "===")
                    )
                    bearer_r = decoded["bearer_token"]
                    require(bearer_r == tok_r.json()["bearer_token"], "pairing code bearer mismatch")

                    compare(
                        "E12 list pairing tokens",
                        oracle.get(f"{base_o}/integration-hub/bindings/{bid_o}/pairing-tokens", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/bindings/{bid_r}/pairing-tokens", headers=h_a),
                        norm=norm_value,
                        expect_status=200,
                    )

                    c_req = {
                        "items": [
                            {
                                "feature_id": "pt_1",
                                "conflict_kind": "attribute",
                                "field_name": "fld_name",
                                "baseline_value": "Alpha",
                                "web_value": "Beta",
                                "qgis_value": "Gamma",
                            }
                        ]
                    }
                    # Use bearer token on Rust to verify SP credential issued by issue_pairing_token!
                    c_o = oracle.post(
                        f"{base_o}/integration-hub/bindings/{bid_o}/conflicts",
                        headers={"authorization": f"Bearer {tok_o.json()['bearer_token']}"},
                        json=c_req,
                    )
                    c_r = rust.post(
                        f"{base_r}/integration-hub/bindings/{bid_r}/conflicts",
                        headers={"authorization": f"Bearer {bearer_r}"},
                        json=c_req,
                    )
                    compare("E12 record conflicts with SP bearer 201", c_o, c_r, norm=norm_value, expect_status=201)
                    cid_o = c_o.json()["items"][0]["id"]
                    cid_r = c_r.json()["items"][0]["id"]

                    compare(
                        "E12 list conflicts",
                        oracle.get(f"{base_o}/integration-hub/bindings/{bid_o}/conflicts", headers=h_a),
                        rust.get(f"{base_r}/integration-hub/bindings/{bid_r}/conflicts", headers=h_a),
                        norm=norm_value,
                        expect_status=200,
                    )
                    compare(
                        "E12 resolve conflict keep_web",
                        oracle.post(
                            f"{base_o}/integration-hub/bindings/{bid_o}/conflicts/{cid_o}/resolve",
                            headers=csrf_a,
                            json={"decision": "keep_web", "note": "keep server value"},
                        ),
                        rust.post(
                            f"{base_r}/integration-hub/bindings/{bid_r}/conflicts/{cid_r}/resolve",
                            headers=csrf_a,
                            json={"decision": "keep_web", "note": "keep server value"},
                        ),
                        norm=norm_value,
                        expect_status=200,
                    )

                    compare(
                        "E12 revoke pairing token",
                        oracle.post(
                            f"{base_o}/integration-hub/bindings/{bid_o}/pairing-tokens/{tid_o}/revoke",
                            headers=csrf_a,
                        ),
                        rust.post(
                            f"{base_r}/integration-hub/bindings/{bid_r}/pairing-tokens/{tid_r}/revoke",
                            headers=csrf_a,
                        ),
                        norm=norm_value,
                        expect_status=200,
                    )

                    rev_bind = {"action": "revoke", "expected_revision": 4}
                    compare(
                        "E12 revoke binding lifecycle",
                        oracle.post(f"{base_o}/integration-hub/bindings/{bid_o}/lifecycle", headers=csrf_a, json=rev_bind),
                        rust.post(f"{base_r}/integration-hub/bindings/{bid_r}/lifecycle", headers=csrf_a, json=rev_bind),
                        norm=norm_value,
                        expect_status=200,
                    )

                    # Verify SSE event stream opens and emits `event: ready` + auth rejection
                    compare(
                        "E12 SSE unauthenticated 401",
                        oracle.get(f"{base_o}/integration-hub/events"),
                        rust.get(f"{base_r}/integration-hub/events"),
                        expect_status=401,
                    )
                    with rust.stream("GET", f"{base_r}/integration-hub/events", headers=h_a, timeout=5.0) as sse:
                        require(sse.status_code == 200, f"SSE status {sse.status_code}")
                        require(
                            sse.headers.get("content-type", "").startswith("text/event-stream"),
                            f"SSE content-type {sse.headers.get('content-type')}",
                        )
                        lines = []
                        for line in sse.iter_lines():
                            lines.append(line)
                            if "event: ready" in "\n".join(lines):
                                break
                        require("event: ready" in "\n".join(lines), f"SSE missing ready event: {lines}")
                    print("PASS E12 SSE integration-hub/events stream ready", flush=True)
            finally:
                get_engine().dispose()

    print("Consolidated E01/E10/E12 mega-wave parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
