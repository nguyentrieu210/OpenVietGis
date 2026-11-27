#!/usr/bin/env python3
"""E14 extension activation-history differential parity on isolated real PostGIS."""

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


def manifest(owner_url: str, fixture: dict, *, version: str, extension_key: str = "parity_ext"):
    mid = uuid4()
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO extensions.manifests (
                id, organization_id, extension_key, version, api_compatibility,
                display_name, description, permissions, commands, capabilities,
                entity_schemas, enabled, registered_by_actor_id, created_at
            ) VALUES (
                %s,%s,%s,%s,'1.x',
                'Parity extension','',%s::jsonb,%s::jsonb,%s::jsonb,
                %s::jsonb,true,%s,%s
            )
            """,
            (
                mid,
                fixture["org_a_id"],
                extension_key,
                version,
                json.dumps(["ext.parity_ext.read"]),
                json.dumps([]),
                json.dumps([]),
                json.dumps({}),
                fixture["user_a_id"],
                datetime(2026, 11, 26, 6, 0, tzinfo=UTC),
            ),
        )
    return mid


def activation(owner_url: str, fixture: dict, manifest_id, *, extension_key: str = "parity_ext"):
    aid = uuid4()
    now = datetime(2026, 11, 26, 6, 1, tzinfo=UTC)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO extensions.project_activations (
                id, organization_id, project_id, manifest_id, extension_key,
                status, activated_by_actor_id, activated_at, updated_at
            ) VALUES (%s,%s,%s,%s,%s,'active',%s,%s,%s)
            """,
            (
                aid,
                fixture["org_a_id"],
                fixture["project_a_id"],
                manifest_id,
                extension_key,
                fixture["user_a_id"],
                now,
                now,
            ),
        )
    return aid


def event(owner_url: str, fixture: dict, aid, *, event_name: str, manifest_id, previous=None, when=None):
    eid = uuid4()
    when = when or datetime(2026, 11, 26, 6, 2, tzinfo=UTC)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO extensions.activation_events (
                id, organization_id, project_id, activation_id, event,
                previous_manifest_id, manifest_id, status, actor_id, occurred_at
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,'active',%s,%s)
            """,
            (
                eid,
                fixture["org_a_id"],
                fixture["project_a_id"],
                aid,
                event_name,
                previous,
                manifest_id,
                fixture["user_a_id"],
                when,
            ),
        )
    return eid


def set_scoped_viewer(owner_url: str, fixture: dict) -> None:
    now = datetime.now(UTC)
    with psycopg.connect(owner_url) as conn:
        membership_id = conn.execute(
            """
            SELECT id FROM org.organization_memberships
            WHERE organization_id=%s AND user_id=%s
            """,
            (fixture["org_a_id"], fixture["user_a_id"]),
        ).fetchone()[0]
        conn.execute(
            """
            UPDATE org.organization_memberships
            SET role='member'
            WHERE organization_id=%s AND user_id=%s
            """,
            (fixture["org_a_id"], fixture["user_a_id"]),
        )
        conn.execute(
            """
            INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, scope_kind,
                project_id, role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (%s,%s,'membership',%s,'project',%s,'viewer',%s,'human',%s)
            """,
            (
                uuid4(),
                fixture["org_a_id"],
                membership_id,
                fixture["project_a_id"],
                now,
                fixture["user_a_id"],
            ),
        )
        conn.execute(
            """
            INSERT INTO gis_access.scoped_members(organization_id,project_id,user_id)
            VALUES (%s,%s,%s)
            ON CONFLICT DO NOTHING
            """,
            (fixture["org_a_id"], fixture["project_a_id"], fixture["user_a_id"]),
        )


def seed_overflow(owner_url: str, fixture: dict, manifest_id):
    aid = activation(owner_url, fixture, manifest_id, extension_key="parity_overflow")
    base = datetime(2026, 11, 26, 7, 0, tzinfo=UTC)
    rows = []
    for i in range(1001):
        rows.append(
            (
                uuid4(),
                fixture["org_a_id"],
                fixture["project_a_id"],
                aid,
                "activate",
                None,
                manifest_id,
                fixture["user_a_id"],
                base + timedelta(microseconds=i),
            )
        )
    with psycopg.connect(owner_url) as conn, conn.cursor() as cur:
        cur.executemany(
            """
                INSERT INTO extensions.activation_events (
                    id, organization_id, project_id, activation_id, event,
                    previous_manifest_id, manifest_id, status, actor_id, occurred_at
                ) VALUES (%s,%s,%s,%s,%s,%s,%s,'active',%s,%s)
                """,
            rows,
        )
    return aid


@parity.case
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

    with tempfile.TemporaryDirectory(prefix="ovg-e14-parity-") as directory:
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

            m1 = manifest(owner_url, fixture, version="1.0.0")
            m2 = manifest(owner_url, fixture, version="2.0.0")
            aid = activation(owner_url, fixture, m2)
            t = datetime(2026, 11, 26, 6, 2, 3, 123456, tzinfo=UTC)
            first = event(
                owner_url,
                fixture,
                aid,
                event_name="activate",
                manifest_id=m1,
                when=t,
            )
            second = event(
                owner_url,
                fixture,
                aid,
                event_name="transition",
                manifest_id=m2,
                previous=m1,
                when=t + timedelta(microseconds=1),
            )
            overflow_manifest = manifest(
                owner_url, fixture, version="1.0.0", extension_key="parity_overflow"
            )
            overflow = seed_overflow(owner_url, fixture, overflow_manifest)

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            base = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/extensions/activations"
            )
            headers = parity.session_headers(fixture["session_a_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    expected = oracle.get(f"{base}/{aid}/history", headers=headers)
                    actual = rust.get(f"{base}/{aid}/history", headers=headers)
                    compare_response("E14 activation history", expected, actual)
                    ids = [item["id"] for item in actual.json()["items"]]
                    require(ids == [str(first), str(second)], f"event order mismatch: {ids}")
                    print("PASS E14 event order", flush=True)

                    missing = uuid4()
                    compare_response(
                        "E14 missing activation",
                        oracle.get(f"{base}/{missing}/history", headers=headers),
                        rust.get(f"{base}/{missing}/history", headers=headers),
                    )

                    compare_response(
                        "E14 history cap",
                        oracle.get(f"{base}/{overflow}/history", headers=headers),
                        rust.get(f"{base}/{overflow}/history", headers=headers),
                    )

                    set_scoped_viewer(owner_url, fixture)
                    compare_response(
                        "E14 scoped history denied",
                        oracle.get(f"{base}/{aid}/history", headers=headers),
                        rust.get(f"{base}/{aid}/history", headers=headers),
                    )

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E14 foreign tenant denied",
                        oracle.get(f"{base}/{aid}/history", headers=foreign_headers),
                        rust.get(f"{base}/{aid}/history", headers=foreign_headers),
                    )
            finally:
                get_engine().dispose()

    print("E14 activation history parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
