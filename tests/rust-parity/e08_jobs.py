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
    for key in ("id", "operation_id", "output_name", "updated_at", "completed_at"):
        if key in out:
            out[key] = f"<{key}>"
    return out


def seed_job(owner_url: str, fixture: dict, *, status: str, label: str):
    job_id = uuid4()
    created = datetime(2026, 11, 26, 1, 2, 3, 123456, tzinfo=UTC)
    completed = created if status in {"succeeded", "failed", "cancelled"} else None
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
                "SYNTHETIC_FAILURE" if status == "failed" else None,
                "synthetic failure" if status == "failed" else None,
                created,
                created,
                completed,
            ),
        )
    return job_id


def compare_response(label: str, expected, actual, *, normalize=None):
    require(expected.status_code == actual.status_code, f"{label}: status {expected.status_code} != {actual.status_code}")
    ebody = expected.json()
    abody = actual.json()
    if normalize is not None:
        ebody, abody = normalize(ebody), normalize(abody)
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)


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

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/processing"
            )
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
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
            finally:
                get_engine().dispose()

    print("E08 durable job parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
