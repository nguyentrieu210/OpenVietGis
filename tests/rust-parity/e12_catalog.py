#!/usr/bin/env python3
"""E12 integration catalog differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
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


def set_project_role(owner_url: str, fixture: dict, role: str, *, scoped: bool) -> None:
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
            UPDATE org.resource_grants
            SET revoked_at=%s
            WHERE organization_id=%s AND membership_id=%s AND revoked_at IS NULL
            """,
            (now, fixture["org_a_id"], membership_id),
        )
        conn.execute(
            """
            INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, scope_kind,
                project_id, role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (%s,%s,'membership',%s,'project',%s,%s,%s,'human',%s)
            """,
            (
                uuid4(),
                fixture["org_a_id"],
                membership_id,
                fixture["project_a_id"],
                role,
                now,
                fixture["user_a_id"],
            ),
        )
        if scoped:
            conn.execute(
                """
                INSERT INTO gis_access.scoped_members(organization_id,project_id,user_id)
                VALUES (%s,%s,%s)
                ON CONFLICT DO NOTHING
                """,
                (fixture["org_a_id"], fixture["project_a_id"], fixture["user_a_id"]),
            )
        else:
            conn.execute(
                """
                DELETE FROM gis_access.scoped_members
                WHERE organization_id=%s AND project_id=%s AND user_id=%s
                """,
                (fixture["org_a_id"], fixture["project_a_id"], fixture["user_a_id"]),
            )


@parity.case
def compare_response(label: str, expected, actual, *, require_no_store: bool = True):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}",
    )
    mismatch = parity.difference(expected.json(), actual.json())
    require(mismatch is None, f"{label}: {mismatch}")
    if require_no_store:
        require(
            expected.headers.get("cache-control") == "no-store",
            f"{label}: Python cache-control missing",
        )
        require(
            actual.headers.get("cache-control") == "no-store",
            f"{label}: Rust cache-control missing",
        )
    print(f"PASS {label}", flush=True)


def allowed_actions(body: dict, adapter_key: str) -> set[str]:
    adapter = next(item for item in body["adapters"] if item["key"] == adapter_key)
    return {action["action"] for action in adapter["actions"] if action["allowed"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e12-parity-") as directory:
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

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/integration-hub/catalog"
            )
            headers = parity.session_headers(fixture["session_a_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    for role, scoped in (
                        ("viewer", True),
                        ("editor", False),
                        ("manager", False),
                    ):
                        set_project_role(owner_url, fixture, role, scoped=scoped)
                        expected = oracle.get(root, headers=headers)
                        actual = rust.get(root, headers=headers)
                        compare_response(f"E12 catalog role={role} scoped={scoped}", expected, actual)
                        body = actual.json()
                        require(len(body["adapters"]) == 12, "catalog must contain 12 adapters")
                        require(body["scoped_access"] is scoped, "scoped_access mismatch")
                        qgis = allowed_actions(body, "qgis")
                        if role == "viewer":
                            require(qgis == {"catalog.discover", "data.export"}, f"viewer qgis={qgis}")
                        else:
                            require(
                                qgis == {"catalog.discover", "data.export", "sync.push"},
                                f"{role} qgis={qgis}",
                            )
                        print(f"PASS E12 action matrix {role}", flush=True)

                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E12 foreign tenant denied",
                        oracle.get(root, headers=foreign_headers),
                        rust.get(root, headers=foreign_headers),
                        require_no_store=False,
                    )
            finally:
                get_engine().dispose()

    print("E12 integration catalog parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
