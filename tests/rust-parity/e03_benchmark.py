"""E03 HTTP benchmark, isolated PostGIS per engine; no production access."""

from __future__ import annotations

import argparse
import asyncio
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
import psycopg
import run as parity

baseline = parity.baseline


async def workload(url, fixture):
    raw = []

    async def before(request):
        request.extensions["e03_start_ns"] = time.perf_counter_ns()

    async def after(response):
        await response.aread()
        raw.append({
            "method": response.request.method,
            "route": response.request.url.path.rsplit("/", 1)[-1],
            "status": response.status_code,
            "ms": (time.perf_counter_ns() - response.request.extensions["e03_start_ns"]) / 1e6,
        })

    headers = {"x-openvietgis-csrf": fixture["csrf_a_token"]}
    root = f"{url}/api/v1/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
    async with httpx.AsyncClient(
        cookies={"ovg_session": fixture["session_a_token"]},
        timeout=30, trust_env=False,
        event_hooks={"request": [before], "response": [after]},
    ) as client:
        snap_body = {
            "target": [0.25, 0.4], "tolerance": 2,
            "vertices": [[i / 10, 1] for i in range(100)],
            "segments": [[[i, 0], [i + 1, 0]] for i in range(100)],
        }

        async def snap(c, _i, _w):
            response = await c.post(f"{root}/editing/snap", headers=headers, json=snap_body)
            if response.status_code == 200 and response.json().get("snapped") is not True:
                raise AssertionError("snap response is not a hit")
            return response

        cold = await baseline.run_load_step(client, 1, 3, snap)
        editing = await baseline.run_edit_and_conflict_benchmark(
            client, url, fixture, headers, [1, 10, 20], 60,
        )
        warm = {}
        for concurrency in (1, 10, 20):
            warm[str(concurrency)] = await baseline.run_load_step(client, concurrency, 60, snap)

    return {"editing": editing, "snap": {"cold": cold, "warm": warm}, "raw_http": raw}


def run_engine(kind, binary):
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e03-bench-") as tmp:
        work = Path(tmp)
        artifacts = work / "artifacts"
        artifacts.mkdir()
        with parity.database(admin_url) as (owner_url, runtime_url):
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            fixture = baseline.seed_benchmark_database(
                owner_url, seed=parity.SEED, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )
            parity.verify_database_role(runtime_url)
            with psycopg.connect(owner_url) as conn:
                db_version = conn.execute("SELECT version(), postgis_full_version()").fetchone()

            with socket.socket() as probe:
                probe.bind(("127.0.0.1", 0))
                port = probe.getsockname()[1]
            url = f"http://127.0.0.1:{port}"
            env = dict(os.environ)
            env["PYTHONPATH"] = f"{baseline.REPO_ROOT}:{baseline.API_ROOT}"
            if kind == "python":
                command = [sys.executable, "-m", "uvicorn", "openvietgis_api.main:app",
                           "--host", "127.0.0.1", "--port", str(port), "--workers", "1",
                           "--log-level", "warning"]
            else:
                command = [str(binary)]
                env.update(OPENVIETGIS_BIND_ADDR=f"127.0.0.1:{port}",
                           OPENVIETGIS_DATABASE_URL=runtime_url,
                           OPENVIETGIS_DB_MAX_CONNECTIONS="15", RUST_LOG="warn")
            with (work / "server.log").open("w+") as log:
                proc = subprocess.Popen(command, cwd=work, env=env, stdout=log, stderr=log)
                monitor = baseline.ProcessMonitor(proc.pid)
                try:
                    baseline.wait_for_ready(url, timeout_s=30)
                    monitor.start()
                    result = asyncio.run(workload(url, fixture))
                    result["resources"] = monitor.stop()
                    result["database"] = list(db_version)
                    return result
                except BaseException:
                    log.seek(0)
                    print(log.read()[-5000:], file=sys.stderr)
                    raise
                finally:
                    monitor.stop()
                    proc.terminate()
                    try:
                        proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary = args.candidate.resolve(strict=True)
    output = args.output.resolve()
    with binary.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    result = {
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "candidate_sha256": digest, "host": platform.platform(),
        "cpu_count": os.cpu_count(), "seed": parity.SEED,
        "method": "Fresh database and process for each engine; warm database caches not flushed; "
                  "Python default SQLAlchemy pool (5+10), Rust max 15; debug CI binary; "
                  "single run; optimistic-lock 409 counted as expected.",
        "engines": {},
    }
    failed = False
    for kind in ("python", "rust"):
        record = run_engine(kind, binary)
        result["engines"][kind] = record
        for scenario in ("editing", "snap"):
            stats = record[scenario]
            groups = {"cold": stats["cold"], **stats["warm"]}
            for level, row in groups.items():
                print(f"E03 BENCH {kind} {scenario} {level}: "
                      f"p50={row['p50_ms']} p95={row['p95_ms']} p99={row['p99_ms']} "
                      f"errors={row['errors']} peak_rss={record['resources']['peak_rss_mib']} MiB",
                      flush=True)
                failed |= bool(row["errors"])
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
