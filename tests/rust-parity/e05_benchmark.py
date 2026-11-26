#!/usr/bin/env python3
"""Measure E05 viewport parity and latency against the same isolated PostGIS data."""

import argparse
import asyncio
import hashlib
import json
import os
import tempfile
from pathlib import Path

import httpx
from e04_benchmark import measure_phase, server
from run import (
    SEED,
    baseline,
    configure_python,
    database,
    difference,
    verify_database_role,
)

CONCURRENCIES = (1, 10, 20)
SCALES = ("1k", "10k", "30k")


def viewport_workload(fixture, scale):
    layer = fixture["layers"][f"epsg4326_{scale}"]
    path = (
        f"/api/v1/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
        f"/layers/{layer['layer_id']}/viewport"
    )
    params = {
        "content_revision": layer["content_revision"],
        "schema_revision": layer["schema_revision"],
        "west": 105.75,
        "south": 20.92 if scale == "1k" else 20.90,
        "east": 105.95,
        "north": 21.08 if scale == "1k" else 21.10,
        "limit": 500,
    }
    return path, params


async def measure(url, fixture, pid, count):
    headers = {"cookie": f"ovg_session={fixture['session_a_token']}"}
    monitor = baseline.ProcessMonitor(pid)
    monitor.start()
    report = {}
    try:
        async with httpx.AsyncClient(
            base_url=url, headers=headers, trust_env=False, timeout=30
        ) as client:
            for scale in SCALES:
                path, params = viewport_workload(fixture, scale)

                cold = await measure_phase(client, "GET", path, 1, 3, params=params)
                warm = {
                    str(concurrency): await measure_phase(
                        client, "GET", path, concurrency, count, params=params
                    )
                    for concurrency in CONCURRENCIES
                }
                reference = await client.get(path, params=params)
                report[scale] = {
                    "cold": cold,
                    "warm": warm,
                    "reference_status": reference.status_code,
                    "reference_body": reference.json() if reference.status_code == 200 else None,
                }
    finally:
        resources = monitor.stop()
    return {"scenarios": report, "resources": resources}


def compare(backends):
    comparison = {}
    for scale in SCALES:
        oracle = backends["python"]["scenarios"][scale]
        candidate = backends["rust"]["scenarios"][scale]
        candidate_ok = (
            candidate["reference_status"] == 200
            and candidate["cold"]["errors"] == 0
            and all(phase["errors"] == 0 for phase in candidate["warm"].values())
        )
        oracle_all_ok = (
            oracle["reference_status"] == 200
            and oracle["cold"]["errors"] == 0
            and all(phase["errors"] == 0 for phase in oracle["warm"].values())
        )
        oracle_ceiling = scale == "30k" and not oracle_all_ok
        parity = (
            oracle["reference_status"] == 200
            and candidate["reference_status"] == 200
            and difference(oracle["reference_body"], candidate["reference_body"]) is None
        )
        oracle_p95 = oracle["warm"]["20"]["p95_ms"]
        # a p95 ratio against an oracle that is already failing requests says nothing
        ratio = (
            candidate["warm"]["20"]["p95_ms"] / oracle_p95
            if candidate_ok and oracle_all_ok and oracle_p95 > 0
            else None
        )
        oracle_unreadable = scale == "30k" and oracle["reference_status"] == 503
        passes = candidate_ok and (
            oracle_unreadable
            or (oracle_ceiling and parity)
            or (oracle_all_ok and parity and ratio is not None and ratio <= 0.70)
        )
        comparison[scale] = {
            "oracle_status": oracle["reference_status"],
            "candidate_status": candidate["reference_status"],
            "candidate_all_phases_200": candidate_ok,
            "python_30k_ceiling_observed": oracle_ceiling,
            "response_parity": parity,
            "p95_ratio_at_concurrency_20": ratio,
            "passes": passes,
        }
        for scenario in (oracle, candidate):
            del scenario["reference_body"]
    return comparison


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--requests", type=int, default=40, choices=range(40, 121))
    args = parser.parse_args()
    binary, output = args.candidate.resolve(), args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("build the Rust candidate first")
    with binary.open("rb") as candidate_file:
        digest = hashlib.file_digest(candidate_file, "sha256").hexdigest()

    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e05-bench-") as directory:
        work = Path(directory)
        with database(admin) as (owner, runtime):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            configure_python(owner, runtime, artifacts)
            baseline.run_alembic_upgrade(owner, runtime)
            fixture = baseline.seed_benchmark_database(
                owner, seed=SEED, scales=list(SCALES), csrf_secret=baseline.DEFAULT_CSRF_SECRET
            )
            verify_database_role(runtime)
            backends = {}
            for kind in ("python", "rust"):
                with server(binary, runtime, work, kind) as (url, pid):
                    backends[kind] = asyncio.run(measure(url, fixture, pid, args.requests))
    comparison = compare(backends)
    report = {
        "seed": SEED,
        "candidate_sha256": digest,
        "cpu_count": os.cpu_count(),
        "method": "Same seed and requests; cold phase does not clear shared or OS caches.",
        "backends": backends,
        "comparison": comparison,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(comparison, indent=2), flush=True)
    return int(not all(item["passes"] for item in comparison.values()))


if __name__ == "__main__":
    raise SystemExit(main())
