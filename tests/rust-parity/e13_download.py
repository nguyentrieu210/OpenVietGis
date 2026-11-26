#!/usr/bin/env python3
"""E13 offline-package download differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import hashlib
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


def layer_pin(owner_url: str, fixture: dict) -> dict:
    layer_id = fixture["layers"]["edit_4326"]["layer_id"]
    with psycopg.connect(owner_url) as conn:
        row = conn.execute(
            """
            SELECT l.id, l.layer_revision, s.id, s.content_revision, s.schema_revision
            FROM spatial.layers l
            JOIN spatial.data_sources s
              ON s.organization_id=l.organization_id
             AND s.project_id=l.project_id
             AND s.id=l.source_id
            WHERE l.organization_id=%s AND l.project_id=%s AND l.id=%s
            """,
            (fixture["org_a_id"], fixture["project_a_id"], layer_id),
        ).fetchone()
    require(row is not None, "seed layer missing")
    return {
        "layer_id": str(row[0]),
        "layer_revision": int(row[1]),
        "source_id": str(row[2]),
        "content_revision": str(row[3]),
        "schema_revision": str(row[4]),
        "snapshot_sha256": "b" * 64,
    }


def seed_package(
    owner_url: str,
    artifacts: Path,
    fixture: dict,
    pin: dict,
    *,
    label: str,
    actor_id=None,
    revoked=False,
    expired=False,
    override_pin=None,
) -> tuple[str, bytes, str]:
    package_id = uuid4()
    created = datetime(2026, 11, 26, 1, 2, 3, 123456, tzinfo=UTC)
    payload = (
        b"SQLite format 3\x00"
        + f"OpenVietGIS-E13-{label}-{package_id}".encode()
        + bytes(range(32))
    )
    digest = hashlib.sha256(payload).hexdigest()
    storage_key = (
        f"field/{fixture['org_a_id']}/{fixture['project_a_id']}/{package_id}.gpkg"
    )
    target = artifacts / storage_key
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(payload)
    bbox = {"min_x": 106.0, "min_y": 10.0, "max_x": 107.0, "max_y": 11.0}
    expires_at = created - timedelta(hours=1) if expired else created + timedelta(days=7)
    revoked_at = created + timedelta(hours=1) if revoked else None

    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO field.offline_packages (
                id, organization_id, project_id, name, profile, aoi_bbox, crs_code,
                layer_pins, feature_count, storage_key, sha256, byte_size,
                expires_at, revoked_at, created_by_actor_id, created_at
            ) VALUES (
                %s, %s, %s, %s, 'standard_pwa', %s::jsonb, 4326,
                %s::jsonb, 3, %s, %s, %s,
                %s, %s, %s, %s
            )
            """,
            (
                package_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                f"E13 {label}",
                json.dumps(bbox),
                json.dumps([override_pin or pin]),
                storage_key,
                digest,
                len(payload),
                expires_at,
                revoked_at,
                actor_id or fixture["user_a_id"],
                created,
            ),
        )
    return str(package_id), payload, digest


def compare_json(label: str, expected, actual) -> None:
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text} rust={actual.text}",
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

    with tempfile.TemporaryDirectory(prefix="ovg-e13-download-") as directory:
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
            pin = layer_pin(owner_url, fixture)

            valid_id, valid_bytes, valid_sha = seed_package(
                owner_url, artifacts, fixture, pin, label="valid"
            )
            other_id, _, _ = seed_package(
                owner_url,
                artifacts,
                fixture,
                pin,
                label="other-creator",
                actor_id=uuid4(),
            )
            revoked_id, _, _ = seed_package(
                owner_url, artifacts, fixture, pin, label="revoked", revoked=True
            )
            expired_id, _, _ = seed_package(
                owner_url, artifacts, fixture, pin, label="expired", expired=True
            )
            broken_pin = dict(pin)
            broken_pin["layer_id"] = str(uuid4())
            unavailable_id, _, _ = seed_package(
                owner_url,
                artifacts,
                fixture,
                pin,
                label="unavailable",
                override_pin=broken_pin,
            )

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/field/packages"
            )
            headers = parity.session_headers(fixture["session_a_token"])
            foreign = parity.session_headers(fixture["session_b_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    expected = oracle.get(f"{root}/{valid_id}/download", headers=headers)
                    actual = rust.get(f"{root}/{valid_id}/download", headers=headers)
                    require(expected.status_code == actual.status_code == 200, (
                        expected.status_code,
                        expected.text,
                        actual.status_code,
                        actual.text,
                    ))
                    require(expected.content == actual.content == valid_bytes, "download bytes differ")
                    for key in (
                        "content-type",
                        "content-disposition",
                        "x-content-sha256",
                        "cache-control",
                    ):
                        require(
                            expected.headers.get(key) == actual.headers.get(key),
                            f"{key}: {expected.headers.get(key)!r} != {actual.headers.get(key)!r}",
                        )
                    require(actual.headers["x-content-sha256"] == valid_sha, "sha header mismatch")
                    print("PASS E13 verified package download body+headers", flush=True)

                    for package_id, label in (
                        (other_id, "creator-only 404"),
                        (revoked_id, "revoked package"),
                        (expired_id, "expired package"),
                        (unavailable_id, "unavailable packaged layer"),
                    ):
                        compare_json(
                            f"E13 download {label}",
                            oracle.get(f"{root}/{package_id}/download", headers=headers),
                            rust.get(f"{root}/{package_id}/download", headers=headers),
                        )

                    missing = str(uuid4())
                    compare_json(
                        "E13 download missing package",
                        oracle.get(f"{root}/{missing}/download", headers=headers),
                        rust.get(f"{root}/{missing}/download", headers=headers),
                    )

                    compare_json(
                        "E13 download foreign tenant denied",
                        oracle.get(f"{root}/{valid_id}/download", headers=foreign),
                        rust.get(f"{root}/{valid_id}/download", headers=foreign),
                    )
            finally:
                get_engine().dispose()

    print("E13 download parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
