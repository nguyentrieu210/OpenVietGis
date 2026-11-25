#!/usr/bin/env python3
"""E06 cartography read-side differential parity on isolated real PostGIS."""

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

import run as parity
import benchmark_runner as baseline

API = "/api/v1"


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def seed_cartography(owner_url: str, fixture: dict):
    layer_id = fixture["layers"]["edit_4326"]["layer_id"]
    style_id = uuid4()
    layout_id = uuid4()
    now = datetime(2026, 11, 26, 2, 3, 4, 123456, tzinfo=UTC)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO cartography.layer_styles (
                id, organization_id, project_id, layer_id, mode, style_spec,
                label_field, legend_items, revision, updated_by_actor_id,
                created_at, updated_at
            ) VALUES (
                %s, %s, %s, %s, 'single', %s::jsonb,
                NULL, %s::jsonb, 3, %s, %s, %s
            )
            """,
            (
                style_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                layer_id,
                json.dumps(
                    {
                        "fill_color": "#2563eb",
                        "stroke_color": "#1e3a8a",
                        "stroke_width": 1.5,
                        "opacity": 0.75,
                    }
                ),
                json.dumps(
                    [
                        {
                            "label": "Benchmark layer",
                            "fill_color": "#2563eb",
                            "stroke_color": "#1e3a8a",
                        }
                    ]
                ),
                fixture["user_a_id"],
                now,
                now,
            ),
        )
        conn.execute(
            """
            INSERT INTO cartography.print_layouts (
                id, organization_id, project_id, name, title, paper_size,
                orientation, crs_code, attribution, show_north_arrow,
                show_scale_bar, show_legend, layer_ids, extent,
                created_by_actor_id, created_at, updated_at
            ) VALUES (
                %s, %s, %s, 'Parity layout', 'Parity map', 'A4',
                'landscape', 4326, 'OpenVietGIS Project Data', true,
                true, true, %s::jsonb, %s::jsonb,
                %s, %s, %s
            )
            """,
            (
                layout_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                json.dumps([str(layer_id)]),
                json.dumps(
                    {"min_x": 106.0, "min_y": 10.0, "max_x": 107.0, "max_y": 11.0}
                ),
                fixture["user_a_id"],
                now,
                now,
            ),
        )
    return layer_id


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

    with tempfile.TemporaryDirectory(prefix="ovg-e06-parity-") as directory:
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
            layer_id = seed_cartography(owner_url, fixture)

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/cartography"
            )
            headers = parity.session_headers(fixture["session_a_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    compare_response(
                        "E06 get layer style",
                        oracle.get(f"{root}/layers/{layer_id}/style", headers=headers),
                        rust.get(f"{root}/layers/{layer_id}/style", headers=headers),
                    )
                    missing = uuid4()
                    compare_response(
                        "E06 missing style returns null",
                        oracle.get(f"{root}/layers/{missing}/style", headers=headers),
                        rust.get(f"{root}/layers/{missing}/style", headers=headers),
                    )
                    compare_response(
                        "E06 list layouts",
                        oracle.get(f"{root}/layouts", headers=headers),
                        rust.get(f"{root}/layouts", headers=headers),
                    )
                    foreign_headers = parity.session_headers(fixture["session_b_token"])
                    compare_response(
                        "E06 foreign tenant denied",
                        oracle.get(f"{root}/layouts", headers=foreign_headers),
                        rust.get(f"{root}/layouts", headers=foreign_headers),
                    )
            finally:
                get_engine().dispose()

    print("E06 cartography read parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
