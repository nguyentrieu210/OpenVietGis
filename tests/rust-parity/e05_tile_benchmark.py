#!/usr/bin/env python3
"""Qualify bounded MVT reads on pinned 30k, 100k and one-million-point sources."""

import argparse
import asyncio
import hashlib
import json
import os
import re
import tempfile
from pathlib import Path

import httpx
from e04_benchmark import measure_phase, server
from e05_benchmark import CONCURRENCIES, viewport_workload
from run import SEED, baseline, configure_python, database, verify_database_role

TILE_SCALES = ("30k", "100k", "1m")
TILE_FEATURE_LIMIT = 10_000
TILE_BYTE_LIMIT = 4 * 1024 * 1024
# The deterministic fixture has points in this Hanoi tile at all three scales.
# At z=17 the FastAPI professional tile route serves individual points.
LOCAL_TILE = "17/104075/57711"
MVT_TYPE = "application/vnd.mapbox-vector-tile"


def tile_workload(fixture, scale, backend, coordinate):
    layer = fixture["layers"][f"epsg4326_{scale}"]
    base = f"/api/v1/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
    layer_id = layer["layer_id"]
    tile_path = f"layers/{layer_id}/tiles" if backend == "rust" else f"pro/tiles/{layer_id}"
    pins = {key: layer[key] for key in ("content_revision", "schema_revision")}
    return f"{base}/{tile_path}/{coordinate}.mvt", pins


def tile_metadata(response, scale):
    body = response.content
    # The generated feature IDs are the only values with this prefix in the MVT.
    prefix = f"p{scale}-".encode("ascii")
    identifiers = set(re.findall(re.escape(prefix) + rb"[0-9]{7}", body))
    return {
        "status": response.status_code,
        "content_type": response.headers.get("content-type"),
        "cache_control": response.headers.get("cache-control"),
        "complete": response.headers.get("x-openvietgis-tile-complete"),
        "payload_bytes": len(body),
        "sha256": hashlib.sha256(body).hexdigest(),
        "feature_ids": len(identifiers),
    }, identifiers


async def measure_tiles(url, fixture, pid, kind, count):
    report = {}
    headers = {"cookie": f"ovg_session={fixture['session_a_token']}"}
    monitor = baseline.ProcessMonitor(pid)
    monitor.start()
    try:
        async with httpx.AsyncClient(
            base_url=url, headers=headers, trust_env=False, timeout=30
        ) as client:
            for scale in TILE_SCALES:
                path, pins = tile_workload(fixture, scale, kind, LOCAL_TILE)
                cold = await measure_phase(client, "GET", path, 1, 3, params=pins)
                warm = {
                    str(concurrency): await measure_phase(
                        client, "GET", path, concurrency, count, params=pins
                    )
                    for concurrency in CONCURRENCIES
                }
                local, identifiers = tile_metadata(await client.get(path, params=pins), scale)
                entry = {
                    "source_features": fixture["layers"][f"epsg4326_{scale}"]["feature_count"],
                    "cold": cold,
                    "warm": warm,
                    "local_tile": local,
                    "local_ids": identifiers,
                }
                if kind == "rust":
                    path, pins = tile_workload(fixture, scale, kind, "0/0/0")
                    world, identifiers = tile_metadata(await client.get(path, params=pins), scale)
                    expected = {
                        f"p{scale}-{index:07d}".encode("ascii")
                        for index in range(1, TILE_FEATURE_LIMIT + 1)
                    }
                    world["exact_bounded_prefix"] = identifiers == expected
                    entry["world_tile"] = world
                    if scale != "30k":
                        path, params = viewport_workload(fixture, scale)
                        oversized = await client.get(path, params=params)
                        entry["json_viewport_status"] = oversized.status_code
                report[scale] = entry
    finally:
        resources = monitor.stop()
    return {"scenarios": report, "resources": resources}


def successful(entry):
    local = entry["local_tile"]
    return (
        local["status"] == 200
        and local["content_type"] == MVT_TYPE
        and local["feature_ids"] > 0
        and 0 < local["payload_bytes"] <= TILE_BYTE_LIMIT
        and entry["cold"]["errors"] == 0
        and all(result["errors"] == 0 for result in entry["warm"].values())
    )


def compare(backends):
    comparison = {}
    for scale in TILE_SCALES:
        oracle = backends["python"]["scenarios"][scale]
        candidate = backends["rust"]["scenarios"][scale]
        oracle_ok = successful(oracle)
        rust_ok = successful(candidate)
        # Rust selects from the tile plus buffer. FastAPI prefilters to the
        # tile envelope before encoding, so its local IDs must be included.
        expected_local_ids = oracle.pop("local_ids") <= candidate.pop("local_ids")
        local = candidate["local_tile"]
        world = candidate["world_tile"]
        bounded = (
            world["status"] == 200
            and world["content_type"] == MVT_TYPE
            and world["cache_control"] == "private, no-store"
            and world["complete"] == "false"
            and world["exact_bounded_prefix"]
            and 0 < world["payload_bytes"] <= TILE_BYTE_LIMIT
        )
        viewport_cap = scale == "30k" or candidate["json_viewport_status"] == 422
        comparison[scale] = {
            "python_local_success": oracle_ok,
            "rust_local_success": rust_ok,
            "rust_local_includes_oracle_ids": expected_local_ids,
            "bounded_world_tile": bounded,
            "oversized_json_viewport_rejected": viewport_cap if scale != "30k" else None,
            "local_mvt_p95_ratio_at_concurrency_20": (
                candidate["warm"]["20"]["p95_ms"] / oracle["warm"]["20"]["p95_ms"]
                if oracle_ok and rust_ok
                else None
            ),
            "passes": (
                oracle_ok
                and rust_ok
                and expected_local_ids
                and local["complete"] == "true"
                and bounded
                and viewport_cap
            ),
        }
    return comparison


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--requests", type=int, default=40, choices=range(20, 81))
    args = parser.parse_args()
    binary, output = args.candidate.resolve(), args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("build the Rust candidate first")
    with binary.open("rb") as candidate_file:
        digest = hashlib.file_digest(candidate_file, "sha256").hexdigest()

    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with tempfile.TemporaryDirectory(prefix="ovg-e05-tile-bench-") as directory:
        work = Path(directory)
        with database(admin) as (owner, runtime):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            configure_python(owner, runtime, artifacts)
            baseline.run_alembic_upgrade(owner, runtime)
            fixture = baseline.seed_benchmark_database(
                owner, seed=SEED, scales=list(TILE_SCALES), csrf_secret=baseline.DEFAULT_CSRF_SECRET
            )
            verify_database_role(runtime)
            backends = {}
            for kind in ("python", "rust"):
                with server(binary, runtime, work, kind) as (url, pid):
                    backends[kind] = asyncio.run(
                        measure_tiles(url, fixture, pid, kind, args.requests)
                    )
    comparison = compare(backends)
    report = {
        "seed": SEED,
        "seed_duration_s": fixture["seed_duration_s"],
        "candidate_sha256": digest,
        "cpu_count": os.cpu_count(),
        "local_tile": LOCAL_TILE,
        "local_requests_per_concurrency": args.requests,
        "method": (
            "Same pinned point sources and tile address. FastAPI professional MVT and native "
            "Rust MVT have different buffer selection and output metadata. Ratios characterize "
            "these tile routes and are not the SC-003 JSON viewport target. The single Rust "
            "world tile per scale exercises the cap. Shared DB and OS caches are not cleared."
        ),
        "backends": backends,
        "comparison": comparison,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(comparison, indent=2), flush=True)
    return int(not all(entry["passes"] for entry in comparison.values()))


if __name__ == "__main__":
    raise SystemExit(main())
