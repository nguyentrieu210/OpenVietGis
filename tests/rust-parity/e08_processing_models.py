#!/usr/bin/env python3
"""E08 processing-model CRUD differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import json
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


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def normalize_model(body: dict) -> dict:
    out = json.loads(json.dumps(body))
    out["id"] = "<id>"
    out["created_at"] = "<created_at>"
    out["updated_at"] = "<updated_at>"
    return out


@parity.case
def compare(label: str, expected, actual, normalize=lambda x: x):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text} rust={actual.text}",
    )
    if expected.status_code == 204:
        require(actual.text == "", f"{label}: Rust 204 body must be empty")
        print(f"PASS {label}", flush=True)
        return None
    ebody = normalize(expected.json())
    abody = normalize(actual.json())
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}; oracle={ebody}; rust={abody}")
    print(f"PASS {label}", flush=True)
    return actual.json()


def audit_count(owner_url: str, fixture: dict, prefix: str) -> int:
    with psycopg.connect(owner_url) as conn:
        return conn.execute(
            """
            SELECT count(*) FROM audit.audit_events
            WHERE organization_id=%s AND action LIKE %s
            """,
            (fixture["org_a_id"], prefix + "%"),
        ).fetchone()[0]


def fill_quota(owner_url: str, fixture: dict, graph: dict) -> None:
    with psycopg.connect(owner_url) as conn:
        for idx in range(50):
            conn.execute(
                """
                INSERT INTO processing.models(
                    organization_id,project_id,id,name,graph,revision
                ) VALUES (%s,%s,%s,%s,%s::jsonb,1)
                """,
                (
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    uuid4(),
                    f"Quota {idx:02d}",
                    json.dumps(graph),
                ),
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e08-models-") as directory:
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

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/processing/models"
            )
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )
            foreign = parity.session_headers(
                fixture["session_b_token"],
                **{"x-openvietgis-csrf": fixture["csrf_b_token"]},
            )

            step_buffer = str(uuid4())
            step_raster = str(uuid4())
            graph = {
                "version": 1,
                "nodes": [
                    {
                        "id": step_buffer,
                        "algorithm": "buffer",
                        "name": " Buffer output ",
                        "a": {
                            "type": "layer",
                            "id": fixture["layers"]["edit_4326"]["layer_id"],
                        },
                        "parameters": {"distance_meters": 5},
                    },
                    {
                        "id": step_raster,
                        "algorithm": "resample",
                        "name": "Raster output",
                        "a": {"type": "layer", "id": str(uuid4())},
                        "parameters": {"factor": 2, "resampling": "near"},
                    },
                ],
            }
            payload = {"name": "  Parity model  ", "graph": graph}

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    before = audit_count(owner_url, fixture, "processing.model.")

                    expected = oracle.post(root, json=payload, headers=csrf)
                    actual = rust.post(root, json=payload, headers=csrf)
                    rust_created = compare(
                        "E08 model create",
                        expected,
                        actual,
                        normalize_model,
                    )
                    require(expected.status_code == 201, expected.text)
                    oracle_id = expected.json()["id"]
                    rust_id = rust_created["id"]
                    require(rust_created["name"] == "Parity model", rust_created)
                    require(
                        rust_created["graph"]["nodes"][0]["name"] == "Buffer output",
                        rust_created,
                    )
                    require(
                        rust_created["graph"]["nodes"][0]["parameters"]["quad_segs"] == 8,
                        rust_created,
                    )
                    require(
                        rust_created["graph"]["nodes"][1]["parameters"]["band_a"] == 1,
                        rust_created,
                    )
                    print("PASS E08 model DAG normalization", flush=True)

                    compare(
                        "E08 model list shared DB",
                        oracle.get(root, headers=headers),
                        rust.get(root, headers=headers),
                    )

                    update_graph = json.loads(json.dumps(graph))
                    update_graph["nodes"][0]["parameters"]["distance_meters"] = 10
                    update_payload = {
                        "name": " Updated model ",
                        "graph": update_graph,
                        "revision": 1,
                    }
                    expected = oracle.put(
                        f"{root}/{oracle_id}", json=update_payload, headers=csrf
                    )
                    actual = rust.put(
                        f"{root}/{rust_id}", json=update_payload, headers=csrf
                    )
                    compare("E08 model update", expected, actual, normalize_model)
                    require(actual.json()["revision"] == 2, actual.text)

                    stale = dict(update_payload)
                    stale["name"] = "stale"
                    expected = oracle.put(f"{root}/{oracle_id}", json=stale, headers=csrf)
                    actual = rust.put(f"{root}/{rust_id}", json=stale, headers=csrf)
                    compare("E08 stale model update", expected, actual)

                    expected = oracle.delete(
                        f"{root}/{oracle_id}", params={"revision": 2}, headers=csrf
                    )
                    actual = rust.delete(
                        f"{root}/{rust_id}", params={"revision": 2}, headers=csrf
                    )
                    compare("E08 model delete", expected, actual)

                    expected = oracle.delete(
                        f"{root}/{oracle_id}", params={"revision": 2}, headers=csrf
                    )
                    actual = rust.delete(
                        f"{root}/{rust_id}", params={"revision": 2}, headers=csrf
                    )
                    compare("E08 stale model delete", expected, actual)

                    require(
                        audit_count(owner_url, fixture, "processing.model.") == before + 6,
                        "expected create/update/delete audit rows from both runtimes",
                    )
                    print("PASS E08 model audit append", flush=True)

                    cyclic_a, cyclic_b = str(uuid4()), str(uuid4())
                    invalid = {
                        "name": "Cycle",
                        "graph": {
                            "nodes": [
                                {
                                    "id": cyclic_a,
                                    "algorithm": "buffer",
                                    "name": "A",
                                    "a": {"type": "step", "id": cyclic_b},
                                    "parameters": {"distance_meters": 1},
                                },
                                {
                                    "id": cyclic_b,
                                    "algorithm": "buffer",
                                    "name": "B",
                                    "a": {"type": "step", "id": cyclic_a},
                                    "parameters": {"distance_meters": 1},
                                },
                            ]
                        },
                    }
                    compare(
                        "E08 cyclic model rejected",
                        oracle.post(root, json=invalid, headers=csrf),
                        rust.post(root, json=invalid, headers=csrf),
                    )

                    empty_name = {"name": "   ", "graph": graph}
                    compare(
                        "E08 blank model name rejected",
                        oracle.post(root, json=empty_name, headers=csrf),
                        rust.post(root, json=empty_name, headers=csrf),
                    )

                    compare(
                        "E08 foreign tenant denied",
                        oracle.post(root, json=payload, headers=foreign),
                        rust.post(root, json=payload, headers=foreign),
                    )

                    no_csrf = rust.post(root, json=payload, headers=headers)
                    require(no_csrf.status_code == 403, no_csrf.text)
                    print("PASS E08 model CSRF fail-closed", flush=True)

                    # All earlier rows were deleted; quota is now deterministic.
                    fill_quota(owner_url, fixture, graph)
                    compare(
                        "E08 model quota 50",
                        oracle.post(root, json=payload, headers=csrf),
                        rust.post(root, json=payload, headers=csrf),
                    )
            finally:
                get_engine().dispose()

    print("E08 processing model parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
