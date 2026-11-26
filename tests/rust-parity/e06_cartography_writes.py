#!/usr/bin/env python3
"""E06 cartography write differential parity on isolated real PostGIS."""

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


def clone_layer_pair(owner_url: str, fixture: dict):
    base = fixture["layers"]["edit_4326"]
    oracle_id, rust_id = uuid4(), uuid4()
    with psycopg.connect(owner_url) as conn:
        row = conn.execute(
            """
            SELECT source_id, name, created_at, updated_at
            FROM spatial.layers
            WHERE organization_id=%s AND project_id=%s AND id=%s
            """,
            (fixture["org_a_id"], fixture["project_a_id"], base["layer_id"]),
        ).fetchone()
        require(row is not None, "base layer missing")
        source_id, _, created_at, updated_at = row
        for layer_id in (oracle_id, rust_id):
            conn.execute(
                """
                INSERT INTO spatial.layers (
                    id, organization_id, project_id, source_id, name, lifecycle,
                    layer_revision, created_at, updated_at
                ) VALUES (%s,%s,%s,%s,'E06 Style Pair','active',1,%s,%s)
                """,
                (
                    layer_id,
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    source_id,
                    created_at,
                    updated_at,
                ),
            )
    return str(oracle_id), str(rust_id), str(base["layer_id"])


def seed_exports(owner_url: str, fixture: dict, layer_id: str):
    base = datetime(2026, 11, 26, 6, 7, 8, 123456, tzinfo=UTC)
    layouts = []
    with psycopg.connect(owner_url) as conn:
        for idx in range(2):
            layout_id = uuid4()
            layouts.append(layout_id)
            when = base + timedelta(seconds=idx)
            conn.execute(
                """
                INSERT INTO cartography.print_layouts (
                    id, organization_id, project_id, name, title, paper_size,
                    orientation, crs_code, attribution, show_north_arrow,
                    show_scale_bar, show_legend, layer_ids, extent,
                    created_by_actor_id, created_at, updated_at
                ) VALUES (
                    %s,%s,%s,%s,%s,'A4','landscape',4326,
                    'OpenVietGIS Project Data',true,true,true,%s::jsonb,NULL,%s,%s,%s
                )
                """,
                (
                    layout_id,
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    f"Export layout {idx}",
                    f"Export title {idx}",
                    json.dumps([layer_id]),
                    fixture["user_a_id"],
                    when,
                    when,
                ),
            )
            conn.execute(
                """
                INSERT INTO cartography.print_exports (
                    id, organization_id, project_id, layout_id, output_format, dpi,
                    scale_denominator, scale_accuracy_note, revision_manifest,
                    storage_key, sha256, byte_size, created_by_actor_id, created_at
                ) VALUES (%s,%s,%s,%s,%s,150,1000,%s,%s::jsonb,%s,%s,%s,%s,%s)
                """,
                (
                    uuid4(),
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    layout_id,
                    "png" if idx == 0 else "pdf",
                    "Approximate display scale",
                    json.dumps({"layout_id": str(layout_id), "revision": idx + 1}),
                    f"cartography/{layout_id}/{idx}",
                    ("%064x" % (idx + 1))[-64:],
                    1000 + idx,
                    fixture["user_a_id"],
                    when,
                ),
            )


def normalize_style(body: dict) -> dict:
    out = json.loads(json.dumps(body))
    out["id"] = "<id>"
    out["layer_id"] = "<layer_id>"
    out["updated_at"] = "<updated_at>"
    return out


def normalize_layout(body: dict) -> dict:
    out = json.loads(json.dumps(body))
    out["id"] = "<id>"
    out["created_at"] = "<created_at>"
    return out


@parity.case
def compare(label: str, expected, actual, normalize=lambda x: x):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text} rust={actual.text}",
    )
    try:
        ebody, abody = normalize(expected.json()), normalize(actual.json())
    except ValueError:
        require(expected.text == actual.text, f"{label}: text mismatch")
        return None
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}; oracle={ebody}; rust={abody}")
    print(f"PASS {label}", flush=True)
    return actual.json()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e06-write-") as directory:
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
            oracle_layer, rust_layer, layout_layer = clone_layer_pair(owner_url, fixture)
            seed_exports(owner_url, fixture, layout_layer)

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            root = (
                f"{API}/organizations/{fixture['org_a_id']}"
                f"/projects/{fixture['project_a_id']}/cartography"
            )
            headers = parity.session_headers(fixture["session_a_token"])
            csrf = parity.session_headers(
                fixture["session_a_token"],
                **{"x-openvietgis-csrf": fixture["csrf_a_token"]},
            )
            foreign = parity.session_headers(
                fixture["session_b_token"],
                **{"x-openvietgis-csrf": fixture["csrf_b_token"]},
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    styles = [
                        (
                            "single",
                            {
                                "mode": "single",
                                "style_spec": {
                                    "fill_color": "#123456",
                                    "stroke_color": "#654321",
                                    "stroke_width": 2,
                                    "opacity": 0.5,
                                },
                                "label_field": "code",
                            },
                        ),
                        (
                            "categorical",
                            {
                                "mode": "categorical",
                                "style_spec": {
                                    "field": "ward",
                                    "categories": [
                                        {"value": "A", "color": "#112233"},
                                        {"value": 2, "label": "Two"},
                                    ],
                                    "default_color": "#abcdef",
                                },
                            },
                        ),
                        (
                            "range",
                            {
                                "mode": "range",
                                "style_spec": {
                                    "field": "metric_val",
                                    "breaks": [
                                        {"min": 0, "max": 12.5, "color": "#445566"},
                                        {"min": 1000000, "max": 2000000},
                                    ],
                                },
                            },
                        ),
                    ]
                    for label, payload in styles:
                        expected = oracle.put(
                            f"{root}/layers/{oracle_layer}/style",
                            json=payload,
                            headers=csrf,
                        )
                        actual = rust.put(
                            f"{root}/layers/{rust_layer}/style",
                            json=payload,
                            headers=csrf,
                        )
                        body = compare(
                            f"E06 upsert {label} style",
                            expected,
                            actual,
                            normalize_style,
                        )
                        require(body is not None, "style body missing")

                    expected = oracle.put(
                        f"{root}/layers/{oracle_layer}/style",
                        json={
                            "mode": "single",
                            "style_spec": {},
                            "label_field": "missing_field",
                        },
                        headers=csrf,
                    )
                    actual = rust.put(
                        f"{root}/layers/{rust_layer}/style",
                        json={
                            "mode": "single",
                            "style_spec": {},
                            "label_field": "missing_field",
                        },
                        headers=csrf,
                    )
                    compare("E06 unknown label field", expected, actual)

                    expected = oracle.put(
                        f"{root}/layers/{oracle_layer}/style",
                        json={
                            "mode": "range",
                            "style_spec": {
                                "field": "metric_val",
                                "breaks": [{"min": 20, "max": 10}],
                            },
                        },
                        headers=csrf,
                    )
                    actual = rust.put(
                        f"{root}/layers/{rust_layer}/style",
                        json={
                            "mode": "range",
                            "style_spec": {
                                "field": "metric_val",
                                "breaks": [{"min": 20, "max": 10}],
                            },
                        },
                        headers=csrf,
                    )
                    compare("E06 invalid range style", expected, actual)

                    layout_payload = {
                        "name": "  My layout  ",
                        "title": "  Demo title  ",
                        "paper_size": "A2",
                        "orientation": "portrait",
                        "crs_code": 4326,
                        "attribution": "  Demo attribution  ",
                        "layer_ids": [layout_layer],
                        "show_north_arrow": True,
                        "show_scale_bar": False,
                        "show_legend": True,
                        "extent": {
                            "min_x": 105.0,
                            "min_y": 10.0,
                            "max_x": 106.0,
                            "max_y": 11.0,
                        },
                    }
                    expected = oracle.post(f"{root}/layouts", json=layout_payload, headers=csrf)
                    actual = rust.post(f"{root}/layouts", json=layout_payload, headers=csrf)
                    body = compare("E06 create layout", expected, actual, normalize_layout)
                    require(body["name"] == "My layout", body)
                    require(body["paper_size"] == "A2", body)

                    invalid_extent = dict(layout_payload)
                    invalid_extent["name"] = "bad extent"
                    invalid_extent["extent"] = {
                        "min_x": 105.0,
                        "min_y": 10.0,
                        "max_x": 181.0,
                        "max_y": 11.0,
                    }
                    compare(
                        "E06 invalid layout extent",
                        oracle.post(f"{root}/layouts", json=invalid_extent, headers=csrf),
                        rust.post(f"{root}/layouts", json=invalid_extent, headers=csrf),
                    )

                    duplicate = dict(layout_payload)
                    duplicate["name"] = "duplicate"
                    duplicate["layer_ids"] = [layout_layer, layout_layer]
                    compare(
                        "E06 duplicate layout layers",
                        oracle.post(f"{root}/layouts", json=duplicate, headers=csrf),
                        rust.post(f"{root}/layouts", json=duplicate, headers=csrf),
                    )

                    compare(
                        "E06 list exports",
                        oracle.get(f"{root}/exports", headers=headers),
                        rust.get(f"{root}/exports", headers=headers),
                    )

                    compare(
                        "E06 foreign tenant write denied",
                        oracle.post(f"{root}/layouts", json=layout_payload, headers=foreign),
                        rust.post(f"{root}/layouts", json=layout_payload, headers=foreign),
                    )

                    no_csrf = rust.put(
                        f"{root}/layers/{rust_layer}/style",
                        json={"mode": "single", "style_spec": {}},
                        headers=headers,
                    )
                    require(no_csrf.status_code == 403, no_csrf.text)
                    print("PASS E06 style CSRF fail-closed", flush=True)
            finally:
                get_engine().dispose()

    print("E06 cartography write parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
