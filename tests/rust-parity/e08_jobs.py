#!/usr/bin/env python3
"""E08 durable processing-job differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from datetime import UTC, datetime
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


def normalize_job(body: dict) -> dict:
    out = dict(body)
    for key in (
        "id",
        "operation_id",
        "output_name",
        "output_source_id",
        "output_layer_id",
        "created_at",
        "updated_at",
        "completed_at",
    ):
        if key in out and out[key] is not None:
            out[key] = f"<{key}>"
    return out


def normalize_job_keep_pins(body: dict) -> dict:
    out = normalize_job(body)
    # engine_version is identical PostGIS/<ver>, output_content_revision and feature_count
    # MUST match identically across FastAPI and Rust!
    return out


def seed_job(
    owner_url: str,
    fixture: dict,
    *,
    status: str,
    label: str,
    error_code: str | None = None,
    error_message: str | None = None,
):
    job_id = uuid4()
    created = datetime(2026, 11, 26, 1, 2, 3, 123456, tzinfo=UTC)
    completed = created if status in {"succeeded", "failed", "cancelled"} else None
    if status == "failed" and error_code is None:
        error_code = "SYNTHETIC_FAILURE"
        error_message = "synthetic failure"
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO processing.jobs (
                id, organization_id, project_id, algorithm_id, operation_id,
                status, actor_type, actor_id, input_refs, parameters,
                output_name, output_crs, engine_version, attempt_count,
                output_source_id, output_layer_id, output_content_revision,
                feature_count, error_code, error_message,
                created_at, updated_at, completed_at
            ) VALUES (
                %s, %s, %s, 'buffer', %s,
                %s, 'human', %s, %s::jsonb, %s::jsonb,
                %s, 4326, 'postgis-native-v1', 1,
                NULL, NULL, NULL,
                NULL, %s, %s,
                %s, %s, %s
            )
            """,
            (
                job_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                f"e08-{label}-{job_id.hex[:12]}",
                status,
                fixture["user_a_id"],
                json.dumps({"input_layer_id": fixture["layers"]["edit_4326"]["layer_id"]}),
                json.dumps({"distance_meters": 5.0, "quad_segs": 8}),
                f"E08 {label}",
                error_code,
                error_message,
                created,
                created,
                completed,
            ),
        )
    return job_id


def compare_response(label: str, expected, actual, *, normalize=None):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text[:300]} rust={actual.text[:300]}",
    )
    ebody = expected.json()
    abody = actual.json()
    if normalize is not None:
        ebody, abody = normalize(ebody), normalize(abody)
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}; oracle={ebody}; rust={abody}")
    print(f"PASS {label}", flush=True)


def make_manual_layer(client, project_root: str, csrf: dict, name: str, family: str, features: list[dict]):
    res = client.post(
        f"{project_root}/layers/manual",
        headers=csrf,
        json={
            "operation_id": str(uuid4()),
            "name": name,
            "geometry_family": family,
            "fields": [{"field_id": "zone", "name": "Zone", "type": "text", "nullable": True}],
        },
    )
    require(res.status_code == 201, f"create manual layer {name}: {res.text}")
    rec = res.json()
    lid, sid = rec["layer_id"], rec["source_id"]
    src = client.get(f"{project_root}/layers/{lid}", headers=csrf).json()["source"]
    cs = client.post(
        f"{project_root}/sources/{sid}/changesets",
        headers=csrf,
        json={
            "operation_id": str(uuid4()),
            "base_content_revision": src["content_revision"],
            "base_schema_revision": src["schema_revision"],
            "summary": f"seed {name}",
            "operations": features,
        },
    )
    require(cs.status_code in (200, 201), f"seed {name}: {cs.text}")
    src = client.get(f"{project_root}/layers/{lid}", headers=csrf).json()["source"]
    return lid, sid, src


def create_service_token(client, org_id: str, project_id: str, csrf: dict) -> str:
    sp = client.post(
        f"{API}/organizations/{org_id}/service-principals",
        headers=csrf,
        json={"name": f"e08-proc-{uuid4().hex[:8]}"},
    )
    require(sp.status_code == 201, sp.text)
    pid = sp.json()["id"]
    grant = client.post(
        f"{API}/organizations/{org_id}/grants",
        headers=csrf,
        json={
            "subject_kind": "service_principal",
            "subject_id": pid,
            "scope_kind": "project",
            "project_id": project_id,
            "role": "editor",
        },
    )
    require(grant.status_code == 201, grant.text)
    cred = client.post(
        f"{API}/organizations/{org_id}/service-principals/{pid}/credentials",
        headers=csrf,
        json={"name": "e08-runner", "permission_ceiling": ["project.read", "project.edit"]},
    )
    require(cred.status_code == 201, cred.text)
    return cred.json()["secret"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e08-parity-") as directory:
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

            queued = seed_job(owner_url, fixture, status="queued", label="queued")
            failed = seed_job(owner_url, fixture, status="failed", label="failed")
            oracle_cancel = seed_job(owner_url, fixture, status="running", label="cancel-oracle")
            rust_cancel = seed_job(owner_url, fixture, status="running", label="cancel-rust")
            terminal_oracle = seed_job(owner_url, fixture, status="succeeded", label="terminal-oracle")
            terminal_rust = seed_job(owner_url, fixture, status="succeeded", label="terminal-rust")
            quarantined = seed_job(
                owner_url,
                fixture,
                status="failed",
                label="quarantined",
                error_code="RECOVERY_QUARANTINED",
                error_message="quarantined",
            )

            user_id, mid, surveyor = parity.scoped_member(owner_url, fixture)
            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            project_root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}"
            )
            root = f"{project_root}/processing"
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )
            scoped_headers = parity.session_headers(surveyor)

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    # 1. Algorithms catalog
                    compare_response(
                        "E08 list algorithms",
                        oracle.get(f"{root}/algorithms", headers=headers),
                        rust.get(f"{root}/algorithms", headers=headers),
                    )

                    # 2. Existing list/get/cancel parity
                    compare_response(
                        "E08 list jobs",
                        oracle.get(f"{root}/jobs", headers=headers, params={"limit": 50}),
                        rust.get(f"{root}/jobs", headers=headers, params={"limit": 50}),
                    )
                    for job_id, label in ((queued, "queued"), (failed, "failed")):
                        compare_response(
                            f"E08 get {label} job",
                            oracle.get(f"{root}/jobs/{job_id}", headers=headers),
                            rust.get(f"{root}/jobs/{job_id}", headers=headers),
                        )

                    missing = uuid4()
                    compare_response(
                        "E08 missing job",
                        oracle.get(f"{root}/jobs/{missing}", headers=headers),
                        rust.get(f"{root}/jobs/{missing}", headers=headers),
                    )

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E08 foreign tenant denied",
                        oracle.get(f"{root}/jobs", headers=foreign_headers),
                        rust.get(f"{root}/jobs", headers=foreign_headers),
                    )

                    # Scoped user without project grant / or with grant must be denied on processing
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
                    # Add a GIS rule so surveyor becomes GIS-scoped
                    oracle.post(
                        f"{project_root}/access/rules",
                        headers=csrf,
                        json={
                            "user_id": user_id,
                            "layer_id": fixture["layers"]["edit_4326"]["layer_id"],
                            "actions": ["read"],
                        },
                    )
                    compare_response(
                        "E08 scoped actor denied algorithms",
                        oracle.get(f"{root}/algorithms", headers=scoped_headers),
                        rust.get(f"{root}/algorithms", headers=scoped_headers),
                    )

                    for limit in (0, 201):
                        compare_response(
                            f"E08 list limit {limit} rejected",
                            oracle.get(f"{root}/jobs", headers=headers, params={"limit": limit}),
                            rust.get(f"{root}/jobs", headers=headers, params={"limit": limit}),
                        )

                    expected = oracle.post(f"{root}/jobs/{oracle_cancel}/cancel", headers=csrf)
                    actual = rust.post(f"{root}/jobs/{rust_cancel}/cancel", headers=csrf)
                    compare_response("E08 cancel running job", expected, actual, normalize=normalize_job)
                    require(expected.json()["status"] == "cancelled", "oracle cancel did not cancel")
                    require(actual.json()["status"] == "cancelled", "rust cancel did not cancel")

                    expected = oracle.post(f"{root}/jobs/{terminal_oracle}/cancel", headers=csrf)
                    actual = rust.post(f"{root}/jobs/{terminal_rust}/cancel", headers=csrf)
                    compare_response("E08 terminal cancel conflict", expected, actual)
                    require(expected.status_code == 409, "terminal cancel must be 409")

                    no_csrf = rust.post(f"{root}/jobs/{queued}/cancel", headers=headers)
                    require(no_csrf.status_code == 403, "Rust cancel without CSRF must fail closed")
                    print("PASS E08 cancel CSRF fail-closed", flush=True)

                    # 3. Seed clean Polygon + Point layers for Buffer, Clip, Dissolve
                    poly_lid, _, _ = make_manual_layer(
                        oracle,
                        project_root,
                        csrf,
                        "E08 Polygons",
                        "Polygon",
                        [
                            {
                                "op_type": "create",
                                "feature_id": "p_null",
                                "geometry": {
                                    "type": "Polygon",
                                    "coordinates": [
                                        [[105.0, 21.0], [105.05, 21.0], [105.05, 21.05], [105.0, 21.05], [105.0, 21.0]]
                                    ],
                                },
                                "properties": {"zone": None},
                            },
                            {
                                "op_type": "create",
                                "feature_id": "p_lit",
                                "geometry": {
                                    "type": "Polygon",
                                    "coordinates": [
                                        [[105.04, 21.0], [105.09, 21.0], [105.09, 21.05], [105.04, 21.05], [105.04, 21.0]]
                                    ],
                                },
                                "properties": {"zone": "__null__"},
                            },
                        ],
                    )
                    mask_lid, _, _ = make_manual_layer(
                        oracle,
                        project_root,
                        csrf,
                        "E08 Mask",
                        "Polygon",
                        [
                            {
                                "op_type": "create",
                                "feature_id": "mask_1",
                                "geometry": {
                                    "type": "Polygon",
                                    "coordinates": [
                                        [[105.045, 21.01], [105.08, 21.01], [105.08, 21.04], [105.045, 21.04], [105.045, 21.01]]
                                    ],
                                },
                                "properties": {"zone": "mask"},
                            }
                        ],
                    )
                    pt_lid, pt_sid, pt_src = make_manual_layer(
                        oracle,
                        project_root,
                        csrf,
                        "E08 Points",
                        "Point",
                        [
                            {
                                "op_type": "create",
                                "feature_id": "pt_1",
                                "geometry": {"type": "Point", "coordinates": [105.8, 21.0]},
                                "properties": {"zone": "north"},
                            }
                        ],
                    )

                    # 4. Create & run Buffer on both runtimes and verify exact content_revision & output features!
                    buf_o = oracle.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "buffer",
                            "input_layer_id": pt_lid,
                            "parameters": {"distance_meters": 1000, "quad_segs": 8},
                            "output_name": "Buf Out",
                            "operation_id": f"buf-o-{uuid4().hex[:8]}",
                        },
                    )
                    buf_r = rust.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "buffer",
                            "input_layer_id": pt_lid,
                            "parameters": {"distance_meters": 1000, "quad_segs": 8},
                            "output_name": "Buf Out",
                            "operation_id": f"buf-r-{uuid4().hex[:8]}",
                        },
                    )
                    compare_response("E08 create buffer job", buf_o, buf_r, normalize=normalize_job_keep_pins)
                    require(buf_r.json()["status"] == "succeeded", buf_r.text)
                    require(buf_r.json()["feature_count"] == 1, buf_r.text)
                    # Verify derived layer is readable via /layers/{id}/features with exact output_content_revision
                    r_layer = rust.get(
                        f"{project_root}/layers/{buf_r.json()['output_layer_id']}",
                        headers=headers,
                    )
                    require(r_layer.status_code == 200, r_layer.text)
                    r_src = r_layer.json()["source"]
                    require(
                        r_src["content_revision"] == buf_r.json()["output_content_revision"],
                        "output_content_revision mismatch",
                    )
                    r_feats = rust.get(
                        f"{project_root}/layers/{buf_r.json()['output_layer_id']}/features",
                        headers=headers,
                        params={
                            "content_revision": r_src["content_revision"],
                            "schema_revision": r_src["schema_revision"],
                        },
                    )
                    require(r_feats.status_code == 200 and len(r_feats.json()["items"]) == 1, r_feats.text)
                    require(
                        r_feats.json()["items"][0]["attributes"]["buffer_meters"] == "1000",
                        r_feats.text,
                    )
                    print("PASS E08 derived buffer layer readable with R003 pins", flush=True)

                    # 5. Clip parity (including non-Polygon overlay rejection 422 INVALID_OVERLAY)
                    clip_o = oracle.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "clip",
                            "input_layer_id": poly_lid,
                            "overlay_layer_id": mask_lid,
                            "parameters": {},
                            "output_name": "Clip Out",
                            "operation_id": f"clip-o-{uuid4().hex[:8]}",
                        },
                    )
                    clip_r = rust.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "clip",
                            "input_layer_id": poly_lid,
                            "overlay_layer_id": mask_lid,
                            "parameters": {},
                            "output_name": "Clip Out",
                            "operation_id": f"clip-r-{uuid4().hex[:8]}",
                        },
                    )
                    compare_response("E08 create clip job", clip_o, clip_r, normalize=normalize_job_keep_pins)
                    require(clip_r.json()["status"] == "succeeded", clip_r.text)

                    bad_overlay = {
                        "algorithm_id": "clip",
                        "input_layer_id": poly_lid,
                        "overlay_layer_id": pt_lid,
                        "parameters": {},
                        "output_name": "Invalid overlay",
                        "operation_id": f"bad-ov-{uuid4().hex[:8]}",
                    }
                    compare_response(
                        "E08 clip non-polygon overlay rejected",
                        oracle.post(f"{root}/jobs", headers=csrf, json=bad_overlay),
                        rust.post(f"{root}/jobs", headers=csrf, json=bad_overlay),
                    )

                    # 6. Dissolve parity (distinct NULL and "__null__" groups + unknown field failure)
                    dis_o = oracle.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "dissolve",
                            "input_layer_id": poly_lid,
                            "parameters": {"group_by_field": "zone"},
                            "output_name": "Dissolve Out",
                            "operation_id": f"dis-o-{uuid4().hex[:8]}",
                        },
                    )
                    dis_r = rust.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "dissolve",
                            "input_layer_id": poly_lid,
                            "parameters": {"group_by_field": "zone"},
                            "output_name": "Dissolve Out",
                            "operation_id": f"dis-r-{uuid4().hex[:8]}",
                        },
                    )
                    compare_response("E08 create dissolve job", dis_o, dis_r, normalize=normalize_job_keep_pins)
                    require(dis_r.json()["status"] == "succeeded", dis_r.text)
                    require(dis_r.json()["feature_count"] == 2, dis_r.text)

                    # Dissolve unknown field -> durable failed job with UNKNOWN_FIELD
                    unk_o = oracle.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "dissolve",
                            "input_layer_id": poly_lid,
                            "parameters": {"group_by_field": "no_such_field"},
                            "output_name": "Unknown Field",
                            "operation_id": f"unk-o-{uuid4().hex[:8]}",
                        },
                    )
                    unk_r = rust.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "dissolve",
                            "input_layer_id": poly_lid,
                            "parameters": {"group_by_field": "no_such_field"},
                            "output_name": "Unknown Field",
                            "operation_id": f"unk-r-{uuid4().hex[:8]}",
                        },
                    )
                    compare_response("E08 dissolve unknown field fails durably", unk_o, unk_r, normalize=normalize_job)
                    require(unk_r.json()["status"] == "failed" and unk_r.json()["error_code"] == "UNKNOWN_FIELD", unk_r.text)

                    # 7. Idempotency replay + conflict across FastAPI and Rust
                    idem_op = f"idem-{uuid4().hex[:8]}"
                    idem_body = {
                        "algorithm_id": "buffer",
                        "input_layer_id": pt_lid,
                        "parameters": {"distance_meters": 50},
                        "output_name": "Idem Out",
                        "operation_id": idem_op,
                    }
                    first_r = rust.post(f"{root}/jobs", headers=csrf, json=idem_body)
                    require(first_r.status_code == 201, first_r.text)
                    replay_o = oracle.post(f"{root}/jobs", headers=csrf, json=idem_body)
                    require(replay_o.status_code == 201 and replay_o.json()["id"] == first_r.json()["id"], replay_o.text)
                    print("PASS E08 cross-runtime operation_id replay", flush=True)

                    conflict_body = dict(idem_body, parameters={"distance_meters": 75})
                    compare_response(
                        "E08 operation_id conflict",
                        oracle.post(f"{root}/jobs", headers=csrf, json=conflict_body),
                        rust.post(f"{root}/jobs", headers=csrf, json=conflict_body),
                    )

                    # 8. Invalid buffer parameters rejected without coercion
                    for bad_params, label in [
                        ({"distance_meters": True}, "bool distance"),
                        ({"distance_meters": "100"}, "string distance"),
                        ({"distance_meters": 0.05}, "too small distance"),
                        ({"distance_meters": 10, "quad_segs": 1}, "quad_segs too small"),
                        ({"distance_meters": 10, "extra": 1}, "unknown param"),
                    ]:
                        p = {
                            "algorithm_id": "buffer",
                            "input_layer_id": pt_lid,
                            "parameters": bad_params,
                            "output_name": "Bad",
                            "operation_id": f"bad-{uuid4().hex[:8]}",
                        }
                        compare_response(
                            f"E08 reject {label}",
                            oracle.post(f"{root}/jobs", headers=csrf, json=p),
                            rust.post(f"{root}/jobs", headers=csrf, json=p),
                        )

                    # 9. Retry succeeded idempotent + retry quarantined 409 + execute_now=false + stale pin retry!
                    compare_response(
                        "E08 retry succeeded job idempotent",
                        oracle.post(f"{root}/jobs/{buf_o.json()['id']}/retry", headers=csrf),
                        rust.post(f"{root}/jobs/{buf_r.json()['id']}/retry", headers=csrf),
                        normalize=normalize_job_keep_pins,
                    )
                    compare_response(
                        "E08 retry quarantined job 409",
                        oracle.post(f"{root}/jobs/{quarantined}/retry", headers=csrf),
                        rust.post(f"{root}/jobs/{quarantined}/retry", headers=csrf),
                    )

                    q_o = oracle.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "buffer",
                            "input_layer_id": pt_lid,
                            "parameters": {"distance_meters": 25},
                            "output_name": "Queued Stale",
                            "operation_id": f"q-o-{uuid4().hex[:8]}",
                            "execute_now": False,
                        },
                    )
                    q_r = rust.post(
                        f"{root}/jobs",
                        headers=csrf,
                        json={
                            "algorithm_id": "buffer",
                            "input_layer_id": pt_lid,
                            "parameters": {"distance_meters": 25},
                            "output_name": "Queued Stale",
                            "operation_id": f"q-r-{uuid4().hex[:8]}",
                            "execute_now": False,
                        },
                    )
                    compare_response("E08 create queued execute_now=false", q_o, q_r, normalize=normalize_job)
                    require(q_r.json()["status"] == "queued", q_r.text)

                    # Mutate pt_lid so both queued jobs become stale
                    cs = oracle.post(
                        f"{project_root}/sources/{pt_sid}/changesets",
                        headers=csrf,
                        json={
                            "base_content_revision": pt_src["content_revision"],
                            "operation_id": f"stale-edit-{uuid4().hex[:8]}",
                            "summary": "make stale",
                            "operations": [
                                {"op_type": "update", "feature_id": "pt_1", "properties": {"zone": "south"}}
                            ],
                        },
                    )
                    require(cs.status_code in (200, 201), cs.text)

                    stale_o = oracle.post(f"{root}/jobs/{q_o.json()['id']}/retry", headers=csrf)
                    stale_r = rust.post(f"{root}/jobs/{q_r.json()['id']}/retry", headers=csrf)
                    compare_response("E08 stale pin retry fails durably", stale_o, stale_r, normalize=normalize_job)
                    require(stale_r.json()["status"] == "failed", stale_r.text)
                    require(stale_r.json()["error_code"] == "STALE_INPUT_PIN", stale_r.text)
                    require(stale_r.json()["attempt_count"] == 2, stale_r.text)

                    # 10. Service principal parity
                    svc_secret = create_service_token(
                        oracle, str(fixture["org_a_id"]), str(fixture["project_a_id"]), csrf
                    )
                    svc_headers = {"authorization": f"Bearer {svc_secret}"}
                    compare_response(
                        "E08 service principal list algorithms",
                        oracle.get(f"{root}/algorithms", headers=svc_headers),
                        rust.get(f"{root}/algorithms", headers=svc_headers),
                    )
                    svc_job = rust.post(
                        f"{root}/jobs",
                        headers=svc_headers,
                        json={
                            "algorithm_id": "buffer",
                            "input_layer_id": pt_lid,
                            "parameters": {"distance_meters": 15},
                            "output_name": "Svc Job",
                            "operation_id": f"svc-{uuid4().hex[:8]}",
                            "execute_now": False,
                        },
                    )
                    require(svc_job.status_code == 201, svc_job.text)
                    svc_cancel = rust.post(
                        f"{root}/jobs/{svc_job.json()['id']}/cancel",
                        headers=svc_headers,
                    )
                    require(svc_cancel.status_code == 200 and svc_cancel.json()["status"] == "cancelled", svc_cancel.text)
                    print("PASS E08 service principal create & cancel", flush=True)
            finally:
                get_engine().dispose()

    print("E08 durable job parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
