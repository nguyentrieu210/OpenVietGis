#!/usr/bin/env python3
"""E06 cartography read differential parity on isolated real PostGIS."""

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


def seed_cartography(owner_url: str, fixture: dict):
    layer_id = fixture["layers"]["edit_4326"]["layer_id"]
    style_id = uuid4()
    layout_old = uuid4()
    layout_new = uuid4()
    created = datetime(2026, 11, 26, 1, 2, 3, 123456, tzinfo=UTC)
    style_spec = {
        "fill_color": "#2563eb",
        "stroke_color": "#1e3a8a",
        "stroke_width": 1.5,
        "opacity": 0.75,
    }
    legend = [{"label": "All features", "color": "#2563eb"}]
    extent = {"min_x": 106.0, "min_y": 10.0, "max_x": 107.0, "max_y": 11.0}
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO cartography.layer_styles (
                id, organization_id, project_id, layer_id, mode, style_spec,
                label_field, legend_items, revision, updated_by_actor_id,
                created_at, updated_at
            ) VALUES (%s,%s,%s,%s,'single',%s::jsonb,NULL,%s::jsonb,2,%s,%s,%s)
            """,
            (
                style_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                layer_id,
                json.dumps(style_spec),
                json.dumps(legend),
                fixture["user_a_id"],
                created,
                created,
            ),
        )
        for layout_id, label, when, paper, orient in (
            (layout_old, "Old", created, "A4", "landscape"),
            (layout_new, "New", created + timedelta(seconds=1), "A3", "portrait"),
        ):
            conn.execute(
                """
                INSERT INTO cartography.print_layouts (
                    id, organization_id, project_id, name, title, paper_size,
                    orientation, crs_code, attribution, show_north_arrow,
                    show_scale_bar, show_legend, layer_ids, extent,
                    created_by_actor_id, created_at, updated_at
                ) VALUES (
                    %s,%s,%s,%s,%s,%s,%s,4326,'OpenVietGIS Project Data',
                    true,true,true,%s::jsonb,%s::jsonb,%s,%s,%s
                )
                """,
                (
                    layout_id,
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    f"E06 {label}",
                    f"E06 {label} title",
                    paper,
                    orient,
                    json.dumps([str(layer_id)]),
                    json.dumps(extent),
                    fixture["user_a_id"],
                    when,
                    when,
                ),
            )
    return str(layer_id), str(layout_new), str(layout_old)


@parity.case
def compare_response(label: str, expected, actual) -> dict:
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}",
    )
    ebody = expected.json()
    abody = actual.json()
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}")
    print(f"PASS {label}", flush=True)
    return abody


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
            layer_id, newest, oldest = seed_cartography(owner_url, fixture)

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/cartography"
            )
            headers = parity.session_headers(fixture["session_a_token"])
            foreign = parity.session_headers(fixture["session_b_token"])

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    body = compare_response(
                        "E06 layer style",
                        oracle.get(f"{root}/layers/{layer_id}/style", headers=headers),
                        rust.get(f"{root}/layers/{layer_id}/style", headers=headers),
                    )
                    require(body["style"]["revision"] == 2, "style revision mismatch")

                    missing_layer = uuid4()
                    body = compare_response(
                        "E06 missing style is null",
                        oracle.get(f"{root}/layers/{missing_layer}/style", headers=headers),
                        rust.get(f"{root}/layers/{missing_layer}/style", headers=headers),
                    )
                    require(body == {"style": None}, "missing style must be null")

                    layouts = compare_response(
                        "E06 list layouts",
                        oracle.get(f"{root}/layouts", headers=headers),
                        rust.get(f"{root}/layouts", headers=headers),
                    )
                    require(
                        [item["id"] for item in layouts["items"]] == [newest, oldest],
                        "layout ordering must be newest first",
                    )

                    compare_response(
                        "E06 foreign tenant style denied",
                        oracle.get(f"{root}/layers/{layer_id}/style", headers=foreign),
                        rust.get(f"{root}/layers/{layer_id}/style", headers=foreign),
                    )
                    compare_response(
                        "E06 foreign tenant layouts denied",
                        oracle.get(f"{root}/layouts", headers=foreign),
                        rust.get(f"{root}/layouts", headers=foreign),
                    )
            finally:
                get_engine().dispose()

    print("E06 cartography read parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
