#!/usr/bin/env python3
"""E04 comparative HTTP benchmark on a unique loopback PostGIS test database."""

import argparse
import asyncio
import contextlib
import hashlib
import json
import os
import platform
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import httpx
from run import ROOT, SEED, baseline, configure_python, database, difference, verify_database_role


@contextlib.contextmanager
def server(binary, runtime_url, work, kind):
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    env = {key: val for key, val in os.environ.items() if not key.startswith("OVG_")}
    if kind == "python":
        command = [
            sys.executable,
            "-m",
            "uvicorn",
            "openvietgis_api.main:app",
            "--host",
            "127.0.0.1",
            "--port",
            str(port),
            "--workers",
            "1",
            "--log-level",
            "warning",
        ]
        env["PYTHONPATH"] = f"{ROOT}:{ROOT / 'apps/api'}"
    else:
        command = [str(binary)]
        env.update(
            OPENVIETGIS_DATABASE_URL=runtime_url,
            OPENVIETGIS_BIND_ADDR=f"127.0.0.1:{port}",
            OPENVIETGIS_DB_MAX_CONNECTIONS="15",
            RUST_LOG="warn",
        )
    url = f"http://127.0.0.1:{port}"
    with (
        (work / f"{kind}.log").open("w+") as log,
        subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=log) as proc,
    ):
        try:
            baseline.wait_for_ready(url)
            if proc.poll() is not None:
                raise RuntimeError(f"{kind} exited before benchmark")
            yield url, proc.pid
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def workloads(fixture):
    root = f"/api/v1/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
    layers = fixture["layers"]
    for name, key, body in (
        (
            "attribute_1k",
            "epsg4326_1k",
            {
                "attribute_filter": {"field": "ward", "op": "eq", "value": "HN-05"},
                "sort_field": "metric_val",
                "limit": 100,
            },
        ),
        (
            "spatial_bbox_query_30k",
            "epsg4326_30k",
            {
                "spatial_filter": {
                    "predicate": "intersects",
                    "crs_code": 4326,
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [105.75, 20.90],
                                [105.95, 20.90],
                                [105.95, 21.10],
                                [105.75, 21.10],
                                [105.75, 20.90],
                            ]
                        ],
                    },
                },
                "limit": 200,
                "offset": 0,
            },
        ),
    ):
        layer = layers[key]
        yield (
            name,
            f"{root}/layers/{layer['layer_id']}/query",
            body
            | {
                "expected_content_revision": layer["content_revision"],
                "expected_schema_revision": layer["schema_revision"],
            },
        )


async def measure_phase(client, method, path, concurrency, requests, **request_options):
    samples = []

    async def call(session, index, worker):
        start = time.perf_counter_ns()
        status = "transport_error"
        try:
            result = await session.request(method, path, **request_options)
            status = result.status_code
            return result
        finally:
            samples.append(
                {
                    "index": index,
                    "worker": worker,
                    "ms": (time.perf_counter_ns() - start) / 1e6,
                    "status": status,
                }
            )

    result = await baseline.run_load_step(client, concurrency, requests, call)
    result["raw_samples"] = sorted(samples, key=lambda sample: sample["index"])
    return result


async def measure(url, fixture, pid, count):
    monitor = baseline.ProcessMonitor(pid)
    monitor.start()
    report, bodies = {}, {}
    headers = {
        "cookie": f"ovg_session={fixture['session_a_token']}",
        "x-openvietgis-csrf": fixture["csrf_a_token"],
    }
    try:
        async with httpx.AsyncClient(
            base_url=url, headers=headers, trust_env=False, timeout=120
        ) as client:
            for name, path, body in workloads(fixture):
                cold = await measure_phase(client, "POST", path, 1, 3, json=body)
                warm = {
                    str(concurrency): await measure_phase(
                        client,
                        "POST",
                        path,
                        concurrency,
                        max(concurrency, count if name == "attribute_1k" else 5),
                        json=body,
                    )
                    for concurrency in (1, 10, 20)
                }
                report[name] = {"cold": cold, "warm": warm, "rss_after": monitor.snapshot_rss_mib()}
                response = await client.post(path, json=body)
                try:
                    payload = response.json()
                except ValueError:
                    payload = None
                bodies[name] = (response.status_code, payload)
    finally:
        resources = monitor.stop()
    return report, bodies, resources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--requests", type=int, default=20, choices=range(20, 101))
    args = parser.parse_args()
    binary, output = args.candidate.resolve(), args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("build the Rust candidate first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with binary.open("rb") as candidate_file:
        candidate_digest = hashlib.file_digest(candidate_file, "sha256").hexdigest()
    report = {
        "seed": SEED,
        "platform": platform.platform(),
        "cpu_count": os.cpu_count(),
        "candidate_sha256": candidate_digest,
        "method": "Same synthetic database and HTTP payloads; first 3 scenario requests "
        "are cold-ish, DB/OS caches are not flushed. RSS excludes DB and OS.",
        "backends": {},
    }
    with tempfile.TemporaryDirectory(prefix="ovg-e04-bench-") as directory:
        work = Path(directory)
        with database(admin) as (owner, runtime):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            configure_python(owner, runtime, artifacts)
            baseline.run_alembic_upgrade(owner, runtime)
            fixture = baseline.seed_benchmark_database(
                owner, seed=SEED, scales=["1k", "30k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
            )
            verify_database_role(runtime)
            responses = {}
            for kind in ("python", "rust"):
                with server(binary, runtime, work, kind) as (url, pid):
                    scenarios, responses[kind], resources = asyncio.run(
                        measure(url, fixture, pid, args.requests)
                    )
                    report["backends"][kind] = {"scenarios": scenarios, "resources": resources}
            report["comparison"] = {}
            for name, _, _ in workloads(fixture):
                ref = report["backends"]["python"]["scenarios"][name]["warm"]["20"]
                candidate = report["backends"]["rust"]["scenarios"][name]["warm"]["20"]
                oracle_status, oracle_body = responses["python"][name]
                rust_status, rust_body = responses["rust"][name]
                report["comparison"][name] = {
                    "response_parity": oracle_status == rust_status == 200
                    and difference(oracle_body, rust_body) is None,
                    "p95_ratio_rust_to_python": candidate["p95_ms"] / ref["p95_ms"]
                    if ref["p95_ms"]
                    else None,
                    "error_rate_nonincrease": all(
                        report["backends"]["rust"]["scenarios"][name]["warm"][str(c)][
                            "error_rate_pct"
                        ]
                        <= report["backends"]["python"]["scenarios"][name]["warm"][str(c)][
                            "error_rate_pct"
                        ]
                        for c in (1, 10, 20)
                    ),
                    "all_phases_success": all(
                        not scenario["cold"]["errors"]
                        and all(not phase["errors"] for phase in scenario["warm"].values())
                        for scenario in (
                            report["backends"][kind]["scenarios"][name]
                            for kind in ("python", "rust")
                        )
                    ),
                    "p95_feature_api_nonregression": candidate["p95_ms"] <= ref["p95_ms"],
                    # The parent viewport 0.7 target is not an E04 query endpoint gate.
                    "p95_stretch_70pct": candidate["p95_ms"] <= 0.7 * ref["p95_ms"],
                }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["comparison"], indent=2), flush=True)
    # A fast error response does not qualify as a performance improvement.
    return int(
        not all(
            item["response_parity"]
            and item["all_phases_success"]
            and item["error_rate_nonincrease"]
            and item["p95_feature_api_nonregression"]
            for item in report["comparison"].values()
        )
    )


if __name__ == "__main__":
    raise SystemExit(main())
