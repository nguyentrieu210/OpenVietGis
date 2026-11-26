#!/usr/bin/env python3
"""E10 CAD document read-side differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
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

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def seed_version(owner_url: str, fixture: dict, doc_id, *, version: int, created: datetime, fmt="dxf"):
    filename = f"parcel-v{version}.{fmt}"
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO cad.document_versions (
                organization_id, project_id, document_id, version,
                filename, format, storage_key, sha256, byte_size, actor_id, created_at
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            """,
            (
                fixture["org_a_id"],
                fixture["project_a_id"],
                doc_id,
                version,
                filename,
                fmt,
                f"cad-doc/{fixture['org_a_id']}/{fixture['project_a_id']}/{doc_id}/{version}.{fmt}",
                ("%064x" % (version + 1))[-64:],
                100 + version,
                fixture["user_a_id"],
                created,
            ),
        )


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

    with tempfile.TemporaryDirectory(prefix="ovg-e10-parity-") as directory:
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

            doc_a = uuid4()
            doc_b = uuid4()
            base = datetime(2026, 11, 26, 5, 6, 7, 123456, tzinfo=UTC)
            seed_version(owner_url, fixture, doc_a, version=1, created=base)
            seed_version(owner_url, fixture, doc_a, version=2, created=base + timedelta(seconds=1))
            seed_version(owner_url, fixture, doc_b, version=1, created=base + timedelta(seconds=2), fmt="dwg")

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/cad/documents"
            )
            headers = parity.session_headers(fixture["session_a_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    expected = oracle.get(root, headers=headers)
                    actual = rust.get(root, headers=headers)
                    compare_response("E10 list CAD documents", expected, actual)
                    latest = {item["id"]: item["version"] for item in actual.json()["items"]}
                    require(latest[str(doc_a)] == 2, "latest CAD revision not selected")
                    require(latest[str(doc_b)] == 1, "single CAD revision missing")
                    print("PASS E10 latest-per-document selection", flush=True)

                    compare_response(
                        "E10 list CAD versions",
                        oracle.get(f"{root}/{doc_a}/versions", headers=headers),
                        rust.get(f"{root}/{doc_a}/versions", headers=headers),
                    )
                    versions = [item["version"] for item in rust.get(f"{root}/{doc_a}/versions", headers=headers).json()["items"]]
                    require(versions == [2, 1], f"version order mismatch: {versions}")
                    print("PASS E10 version history order", flush=True)

                    missing = uuid4()
                    compare_response(
                        "E10 missing CAD document",
                        oracle.get(f"{root}/{missing}/versions", headers=headers),
                        rust.get(f"{root}/{missing}/versions", headers=headers),
                    )

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E10 foreign tenant denied",
                        oracle.get(root, headers=foreign_headers),
                        rust.get(root, headers=foreign_headers),
                    )
            finally:
                get_engine().dispose()

    print("E10 CAD document parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
