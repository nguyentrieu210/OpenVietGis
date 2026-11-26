#!/usr/bin/env python3
"""E13 offline-package list/revoke differential parity on isolated real PostGIS."""

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

import benchmark_runner as baseline
import run as parity

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
    fixture: dict,
    pin: dict,
    *,
    label: str,
    actor_id=None,
    override_pin: dict | None = None,
):
    package_id = uuid4()
    created = datetime(2026, 11, 26, 1, 2, 3, 123456, tzinfo=UTC)
    bbox = {"min_x": 106.0, "min_y": 10.0, "max_x": 107.0, "max_y": 11.0}
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO field.offline_packages (
                id, organization_id, project_id, name, profile, aoi_bbox, crs_code,
                layer_pins, feature_count, storage_key, sha256, byte_size,
                expires_at, revoked_at, created_by_actor_id, created_at
            ) VALUES (
                %s, %s, %s, %s, 'standard_pwa', %s::jsonb, 4326,
                %s::jsonb, 3, %s, %s, 4096,
                %s, NULL, %s, %s
            )
            """,
            (
                package_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                f"E13 {label}",
                json.dumps(bbox),
                json.dumps([override_pin or pin]),
                f"field/{fixture['org_a_id']}/{fixture['project_a_id']}/{package_id}.gpkg",
                "a" * 64,
                created + timedelta(days=7),
                actor_id or fixture["user_a_id"],
                created,
            ),
        )
    return package_id


def normalize_package(body: dict) -> dict:
    out = dict(body)
    for key in ("id", "name", "revoked_at"):
        if key in out:
            out[key] = f"<{key}>"
    return out


def compare_response(label: str, expected, actual, *, normalize=None):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}",
    )
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

    with tempfile.TemporaryDirectory(prefix="ovg-e13-parity-") as directory:
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
            visible = seed_package(owner_url, fixture, pin, label="visible")
            oracle_revoke = seed_package(owner_url, fixture, pin, label="revoke-oracle")
            rust_revoke = seed_package(owner_url, fixture, pin, label="revoke-rust")
            seed_package(
                owner_url,
                fixture,
                pin,
                label="other-actor",
                actor_id=fixture["user_b_id"],
            )
            broken_pin = dict(pin)
            broken_pin["layer_id"] = str(uuid4())
            seed_package(
                owner_url,
                fixture,
                pin,
                label="unavailable-layer",
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
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    expected = oracle.get(root, headers=headers)
                    actual = rust.get(root, headers=headers)
                    compare_response("E13 list writable packages", expected, actual)
                    ids = {item["id"] for item in actual.json()["items"]}
                    require(str(visible) in ids, "visible package missing")
                    require(len(ids) == 3, f"actor/layer revalidation failed: {len(ids)}")

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E13 foreign tenant denied",
                        oracle.get(root, headers=foreign_headers),
                        rust.get(root, headers=foreign_headers),
                    )

                    expected = oracle.post(f"{root}/{oracle_revoke}/revoke", headers=csrf)
                    actual = rust.post(f"{root}/{rust_revoke}/revoke", headers=csrf)
                    compare_response(
                        "E13 revoke package",
                        expected,
                        actual,
                        normalize=normalize_package,
                    )
                    require(actual.json()["revoked_at"] is not None, "Rust revoke timestamp missing")

                    first = rust.post(f"{root}/{rust_revoke}/revoke", headers=csrf)
                    second = rust.post(f"{root}/{rust_revoke}/revoke", headers=csrf)
                    require(first.status_code == second.status_code == 200, "revoke must be idempotent")
                    require(
                        first.json()["revoked_at"] == second.json()["revoked_at"],
                        "idempotent revoke changed timestamp",
                    )
                    print("PASS E13 idempotent revoke", flush=True)

                    missing = uuid4()
                    compare_response(
                        "E13 missing package",
                        oracle.post(f"{root}/{missing}/revoke", headers=csrf),
                        rust.post(f"{root}/{missing}/revoke", headers=csrf),
                    )

                    no_csrf = rust.post(f"{root}/{visible}/revoke", headers=headers)
                    require(no_csrf.status_code == 403, "Rust revoke without CSRF must fail closed")
                    print("PASS E13 revoke CSRF fail-closed", flush=True)
            finally:
                get_engine().dispose()

    print("E13 offline package parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
