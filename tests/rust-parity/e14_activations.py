#!/usr/bin/env python3
"""E14 extension activation read differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import json
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


def insert_manifest(
    owner_url: str,
    fixture: dict,
    *,
    manifest: dict,
    extension_key: str | None = None,
    version: str | None = None,
):
    mid = uuid4()
    key = extension_key or manifest["extension_key"]
    ver = version or manifest["version"]
    now = datetime(2026, 11, 26, 3, 12, 34, 123456, tzinfo=UTC)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO extensions.manifests (
                id, organization_id, extension_key, version, api_compatibility,
                display_name, description, permissions, commands, capabilities,
                entity_schemas, enabled, registered_by_actor_id, created_at,
                ui_contributions
            ) VALUES (
                %s,%s,%s,%s,%s,%s,%s,%s::jsonb,%s::jsonb,%s::jsonb,%s::jsonb,
                true,%s,%s,%s::jsonb
            )
            """,
            (
                mid,
                fixture["org_a_id"],
                key,
                ver,
                manifest["api_compatibility"],
                manifest["display_name"],
                manifest.get("description", ""),
                json.dumps(manifest["permissions"]),
                json.dumps(manifest["commands"]),
                json.dumps(manifest["capabilities"]),
                json.dumps(manifest["entity_schemas"]),
                fixture["user_a_id"],
                now,
                json.dumps(manifest.get("ui_contributions", [])),
            ),
        )
    return mid, now


def insert_activation(
    owner_url: str,
    fixture: dict,
    *,
    manifest_id,
    extension_key: str,
    when: datetime,
):
    aid = uuid4()
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO extensions.project_activations (
                id, organization_id, project_id, manifest_id, extension_key,
                status, activated_by_actor_id, activated_at, updated_at, revision
            ) VALUES (%s,%s,%s,%s,%s,'active',%s,%s,%s,1)
            """,
            (
                aid,
                fixture["org_a_id"],
                fixture["project_a_id"],
                manifest_id,
                extension_key,
                fixture["user_a_id"],
                when,
                when,
            ),
        )
    return aid


def compare_response(label: str, expected, actual):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}",
    )
    mismatch = parity.difference(expected.json(), actual.json())
    require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)
    return actual.json()


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

            shipped = json.loads(
                (ROOT / "modules" / "house_numbering" / "manifest.json").read_text()
            )
            official_mid, when = insert_manifest(owner_url, fixture, manifest=shipped)
            insert_activation(
                owner_url,
                fixture,
                manifest_id=official_mid,
                extension_key=shipped["extension_key"],
                when=when,
            )

            generic = {
                "extension_key": "generic_demo",
                "version": "1.0.0",
                "api_compatibility": "1.x",
                "display_name": "Generic",
                "description": "Generic test extension",
                "permissions": ["ext.generic_demo.read"],
                "commands": [],
                "capabilities": ["record_list"],
                "entity_schemas": {
                    "thing": {"fields": {"name": {"type": "text", "required": True}}}
                },
                "ui_contributions": [],
            }
            generic_mid, generic_when = insert_manifest(owner_url, fixture, manifest=generic)
            insert_activation(
                owner_url,
                fixture,
                manifest_id=generic_mid,
                extension_key=generic["extension_key"],
                when=generic_when,
            )

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/extensions/activations"
            )
            headers = parity.session_headers(fixture["session_a_token"])
            foreign = parity.session_headers(fixture["session_b_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    body = compare_response(
                        "E14 activation list compatible",
                        oracle.get(root, headers=headers),
                        rust.get(root, headers=headers),
                    )
                    by_key = {item["extension_key"]: item for item in body["items"]}
                    require(
                        by_key["house_numbering"]["runtime_compatible"] is True,
                        "official manifest must be runtime compatible",
                    )
                    require(
                        "runtime_compatible" not in by_key["generic_demo"],
                        "non-official extension must not receive runtime flag",
                    )

                    with psycopg.connect(owner_url) as conn:
                        conn.execute(
                            "UPDATE extensions.manifests SET capabilities=%s::jsonb WHERE id=%s",
                            (json.dumps(["different_capability"]), official_mid),
                        )

                    body = compare_response(
                        "E14 activation list runtime mismatch",
                        oracle.get(root, headers=headers),
                        rust.get(root, headers=headers),
                    )
                    by_key = {item["extension_key"]: item for item in body["items"]}
                    require(
                        by_key["house_numbering"]["runtime_compatible"] is False,
                        "modified official manifest must fail runtime compatibility",
                    )

                    compare_response(
                        "E14 foreign tenant denied",
                        oracle.get(root, headers=foreign),
                        rust.get(root, headers=foreign),
                    )
            finally:
                get_engine().dispose()

    print("E14 activation read parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
