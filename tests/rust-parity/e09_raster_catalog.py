#!/usr/bin/env python3
"""E09 raster catalog differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import uuid4

import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import run as parity
import benchmark_runner as baseline

API = "/api/v1"


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def seed_raster(owner_url: str, fixture: dict, *, label: str, created: datetime, collision=False):
    rid = uuid4()
    metadata = {
        "method": "ndvi",
        "width": 32,
        "height": 16,
        "crs": "EPSG:4326",
    }
    if collision:
        metadata.update(
            {
                "id": "metadata-id",
                "name": "metadata-name",
                "byte_size": 7,
                "created_at": "metadata-created",
            }
        )
    cog = b"II*\x00" + b"x" * 28
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO raster.layers (
                organization_id, project_id, id, name, metadata, cog, byte_size, created_at
            ) VALUES (%s, %s, %s, %s, %s::jsonb, %s, %s, %s)
            """,
            (
                fixture["org_a_id"],
                fixture["project_a_id"],
                rid,
                f"Raster {label}",
                json.dumps(metadata),
                cog,
                len(cog),
                created,
            ),
        )
    return rid


def compare_response(label: str, expected, actual):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}",
    )
    mismatch = parity.difference(expected.json(), actual.json())
    require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e09-parity-") as directory:
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

            base = datetime(2026, 11, 26, 4, 5, 6, 123456, tzinfo=UTC)
            seed_raster(owner_url, fixture, label="old", created=base)
            seed_raster(
                owner_url,
                fixture,
                label="collision",
                created=base + timedelta(seconds=1),
                collision=True,
            )

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/raster/layers"
            )
            headers = parity.session_headers(fixture["session_a_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    expected = oracle.get(root, headers=headers)
                    actual = rust.get(root, headers=headers)
                    compare_response("E09 list raster layers", expected, actual)
                    require(len(actual.json()["items"]) == 2, "expected two raster rows")
                    newest = actual.json()["items"][0]
                    require(newest["id"] == "metadata-id", "metadata must override id like Python")
                    require(newest["name"] == "metadata-name", "metadata must override name like Python")
                    require(newest["byte_size"] == 32, "canonical byte_size must override metadata")
                    require(
                        newest["created_at"].startswith("2026-11-26T04:05:07.123456"),
                        "canonical created_at must override metadata",
                    )
                    print("PASS E09 metadata unpack precedence", flush=True)

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E09 foreign tenant denied",
                        oracle.get(root, headers=foreign_headers),
                        rust.get(root, headers=foreign_headers),
                    )
            finally:
                get_engine().dispose()

    print("E09 raster catalog parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
