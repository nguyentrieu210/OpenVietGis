#!/usr/bin/env python3
"""E15 House Numbering write differential parity on isolated real PostGIS."""

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


def compare(label: str, expected, actual, normalize=lambda x: x) -> None:
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text} rust={actual.text}",
    )
    try:
        ebody, abody = normalize(expected.json()), normalize(actual.json())
    except ValueError:
        require(expected.text == actual.text, f"{label}: text mismatch")
    else:
        mismatch = parity.difference(ebody, abody)
        require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)


def normalize_rule(body: dict) -> dict:
    out = dict(body)
    for key in ("id", "version", "created_at", "updated_at"):
        if key in out:
            out[key] = f"<{key}>"
    return out


def normalize_area(body: dict) -> dict:
    out = dict(body)
    for key in ("id", "name", "created_at", "updated_at"):
        if key in out:
            out[key] = f"<{key}>"
    return out


def downgrade_to_editor(owner_url: str, fixture: dict) -> None:
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
            UPDATE org.organization_memberships SET role='member'
            WHERE organization_id=%s AND user_id=%s
            """,
            (fixture["org_a_id"], fixture["user_a_id"]),
        )
        conn.execute(
            """
            UPDATE org.resource_grants SET revoked_at=%s
            WHERE organization_id=%s AND membership_id=%s AND revoked_at IS NULL
            """,
            (now, fixture["org_a_id"], membership_id),
        )
        conn.execute(
            """
            INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, scope_kind,
                project_id, role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (%s,%s,'membership',%s,'project',%s,'editor',%s,'human',%s)
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


def audit_count(owner_url: str, fixture: dict, action: str) -> int:
    with psycopg.connect(owner_url) as conn:
        return conn.execute(
            """
            SELECT count(*) FROM audit.audit_events
            WHERE organization_id=%s AND action=%s
            """,
            (fixture["org_a_id"], action),
        ).fetchone()[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e15-write-parity-") as directory:
        work = Path(directory)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)

            from modules.house_numbering.migrate import (
                upgrade as upgrade_house_numbering,
            )
            upgrade_house_numbering()

            fixture = baseline.seed_benchmark_database(
                owner_url,
                seed=20261126,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            org = f"{API}/organizations/{fixture['org_a_id']}"
            project = f"{org}/projects/{fixture['project_a_id']}"
            root = f"{project}/modules/house-numbering"
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
                    registered = oracle.post(
                        f"{org}/extensions/official/house_numbering/register",
                        headers=csrf,
                    )
                    require(registered.status_code == 201, registered.text)
                    activated = oracle.post(
                        f"{project}/extensions/activations",
                        json={"manifest_id": registered.json()["id"]},
                        headers=csrf,
                    )
                    require(activated.status_code == 201, activated.text)

                    before_rule_audit = audit_count(
                        owner_url, fixture, "house_numbering.rule.create"
                    )
                    expected = oracle.post(
                        f"{root}/rules",
                        json={
                            "version": "write-oracle",
                            "status": "authoritative",
                            "authority_ref": "QD-123",
                            "left_odd": False,
                            "suffix_style": "dash",
                            "max_distance_m": 200.5,
                        },
                        headers=csrf,
                    )
                    actual = rust.post(
                        f"{root}/rules",
                        json={
                            "version": "write-rust",
                            "status": "authoritative",
                            "authority_ref": "QD-123",
                            "left_odd": False,
                            "suffix_style": "dash",
                            "max_distance_m": 200.5,
                        },
                        headers=csrf,
                    )
                    compare("E15 create rule owner/manage", expected, actual, normalize_rule)
                    require(actual.status_code == 201, actual.text)
                    require(actual.json()["max_distance_m"] == "200.50", actual.text)
                    require(
                        audit_count(owner_url, fixture, "house_numbering.rule.create")
                        == before_rule_audit + 2,
                        "rule audit rows missing",
                    )
                    print("PASS E15 rule audit append", flush=True)

                    before_area_audit = audit_count(
                        owner_url, fixture, "house_numbering.area.create"
                    )
                    expected = oracle.post(
                        f"{root}/areas",
                        json={"name": " Area Oracle ", "kind": "historic"},
                        headers=csrf,
                    )
                    actual = rust.post(
                        f"{root}/areas",
                        json={"name": " Area Rust ", "kind": "historic"},
                        headers=csrf,
                    )
                    compare("E15 create area owner/edit", expected, actual, normalize_area)
                    require(actual.json()["name"] == "Area Rust", actual.text)
                    require(
                        audit_count(owner_url, fixture, "house_numbering.area.create")
                        == before_area_audit + 2,
                        "area audit rows missing",
                    )
                    print("PASS E15 area trim + audit", flush=True)

                    missing_csrf = rust.post(
                        f"{root}/areas",
                        json={"name": "No CSRF", "kind": "current"},
                        headers=headers,
                    )
                    require(missing_csrf.status_code == 403, missing_csrf.text)
                    print("PASS E15 write CSRF fail-closed", flush=True)

                    downgrade_to_editor(owner_url, fixture)

                    expected = oracle.post(
                        f"{root}/rules",
                        json={"version": "editor-oracle"},
                        headers=csrf,
                    )
                    actual = rust.post(
                        f"{root}/rules",
                        json={"version": "editor-rust"},
                        headers=csrf,
                    )
                    compare("E15 editor cannot create rule", expected, actual)

                    expected = oracle.post(
                        f"{root}/areas",
                        json={"name": "Editor Oracle", "kind": "custom"},
                        headers=csrf,
                    )
                    actual = rust.post(
                        f"{root}/areas",
                        json={"name": "Editor Rust", "kind": "custom"},
                        headers=csrf,
                    )
                    compare("E15 editor can create area", expected, actual, normalize_area)

                    expected = oracle.post(
                        f"{root}/areas",
                        json={
                            "name": "Broken link",
                            "source_id": str(uuid4()),
                            "feature_id": "missing",
                        },
                        headers=csrf,
                    )
                    actual = rust.post(
                        f"{root}/areas",
                        json={
                            "name": "Broken link",
                            "source_id": str(uuid4()),
                            "feature_id": "missing",
                        },
                        headers=csrf,
                    )
                    compare("E15 linked area missing feature", expected, actual)

                    expected = oracle.post(
                        f"{root}/areas",
                        json={"name": "Half link", "source_id": str(uuid4())},
                        headers=csrf,
                    )
                    actual = rust.post(
                        f"{root}/areas",
                        json={"name": "Half link", "source_id": str(uuid4())},
                        headers=csrf,
                    )
                    compare("E15 linked area pair validation", expected, actual)
            finally:
                get_engine().dispose()

    print("E15 House Numbering write parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
