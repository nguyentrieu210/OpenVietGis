#!/usr/bin/env python3
"""E06 cartography read differential parity on isolated real PostGIS."""

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

import benchmark_runner as baseline
import run as parity

API = "/api/v1"


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise AssertionError(detail)


def seed_cartography(owner_url: str, fixture: dict, artifacts: Path):
    layer_id = fixture["layers"]["edit_4326"]["layer_id"]
    style_id = uuid4()
    layout_old = uuid4()
    layout_new = uuid4()
    export_id = uuid4()
    designer_old = uuid4()
    designer_new = uuid4()
    created = datetime(2026, 11, 26, 1, 2, 3, 123456, tzinfo=UTC)
    style_spec = {
        "fill_color": "#2563eb",
        "stroke_color": "#1e3a8a",
        "stroke_width": 1.5,
        "opacity": 0.75,
    }
    legend = [{"label": "All features", "fill_color": "#2563eb", "stroke_color": "#1e3a8a"}]
    extent = {"min_x": 106.0, "min_y": 10.0, "max_x": 107.0, "max_y": 11.0}
    export_bytes = b"\x89PNG\r\n\x1a\nparity-seeded-png"
    export_sha = hashlib.sha256(export_bytes).hexdigest()
    storage_key = f"cartography/{fixture['org_a_id']}/{fixture['project_a_id']}/{export_id}.png"
    export_path = artifacts / storage_key
    export_path.parent.mkdir(parents=True, exist_ok=True)
    export_path.write_bytes(export_bytes)

    designer_doc = {
        "version": 1,
        "paper": "A4",
        "landscape": True,
        "dpi": 150,
        "items": [
            {
                "id": str(uuid4()),
                "kind": "map",
                "x": 10.0,
                "y": 10.0,
                "w": 200.0,
                "h": 150.0,
                "text": "",
                "font_size": 4.0,
                "color": "#15383b",
                "border": True,
                "grid": False,
                "camera": {
                    "lng": 106.66,
                    "lat": 10.70,
                    "zoom": 14.0,
                    "bearing": 0.0,
                    "reference_width": 900.0,
                },
                "map_id": None,
            }
        ],
    }

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
        conn.execute(
            """
            INSERT INTO cartography.print_exports (
                id, organization_id, project_id, layout_id, output_format, dpi,
                scale_denominator, scale_accuracy_note, revision_manifest,
                storage_key, sha256, byte_size, created_by_actor_id, created_at
            ) VALUES (%s,%s,%s,%s,'png',150,50000,'Approximate scale',%s::jsonb,%s,%s,%s,%s,%s)
            """,
            (
                export_id,
                fixture["org_a_id"],
                fixture["project_a_id"],
                layout_new,
                json.dumps({"layout_id": str(layout_new), "revision": 1}),
                storage_key,
                export_sha,
                len(export_bytes),
                fixture["user_a_id"],
                created + timedelta(seconds=2),
            ),
        )
        for did, name, when in (
            (designer_old, "Designer Old", created),
            (designer_new, "Designer New", created + timedelta(seconds=1)),
        ):
            conn.execute(
                """
                INSERT INTO cartography.designer_layouts (
                    organization_id, project_id, id, name, document,
                    revision, created_at, updated_at
                ) VALUES (%s,%s,%s,%s,%s::jsonb,1,%s,%s)
                """,
                (
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    did,
                    name,
                    json.dumps(designer_doc),
                    when,
                    when,
                ),
            )
    return (
        str(layer_id),
        str(layout_new),
        str(layout_old),
        str(export_id),
        export_bytes,
        export_sha,
        str(designer_new),
        str(designer_old),
    )


def compare_response(label: str, expected, actual) -> dict:
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text} rust={actual.text}",
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
            (
                layer_id,
                newest,
                oldest,
                export_id,
                export_bytes,
                export_sha,
                designer_new,
                designer_old,
            ) = seed_cartography(owner_url, fixture, artifacts)

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

                    exports = compare_response(
                        "E06 list exports",
                        oracle.get(f"{root}/exports", headers=headers),
                        rust.get(f"{root}/exports", headers=headers),
                    )
                    require(
                        [item["id"] for item in exports["items"]] == [export_id],
                        "export list mismatch",
                    )

                    exp_dl = oracle.get(f"{root}/exports/{export_id}/download", headers=headers)
                    act_dl = rust.get(f"{root}/exports/{export_id}/download", headers=headers)
                    require(
                        exp_dl.status_code == act_dl.status_code == 200,
                        f"download status {exp_dl.status_code} != {act_dl.status_code}",
                    )
                    require(
                        exp_dl.content == act_dl.content == export_bytes,
                        "downloaded bytes differ",
                    )
                    require(
                        exp_dl.headers.get("content-type") == act_dl.headers.get("content-type"),
                        "download content-type differs",
                    )
                    require(
                        exp_dl.headers.get("content-disposition")
                        == act_dl.headers.get("content-disposition"),
                        "download content-disposition differs",
                    )
                    require(
                        exp_dl.headers.get("x-content-sha256")
                        == act_dl.headers.get("x-content-sha256")
                        == export_sha,
                        "download sha256 header differs",
                    )
                    print("PASS E06 download export binary & headers", flush=True)

                    compare_response(
                        "E06 missing export download 404",
                        oracle.get(f"{root}/exports/{uuid4()}/download", headers=headers),
                        rust.get(f"{root}/exports/{uuid4()}/download", headers=headers),
                    )

                    designers = compare_response(
                        "E06 list designer layouts",
                        oracle.get(f"{root}/designer", headers=headers),
                        rust.get(f"{root}/designer", headers=headers),
                    )
                    require(
                        [item["id"] for item in designers["items"]] == [designer_new, designer_old],
                        "designer ordering must be newest first",
                    )

                    for label, path in (
                        ("style", f"{root}/layers/{layer_id}/style"),
                        ("layouts", f"{root}/layouts"),
                        ("exports", f"{root}/exports"),
                        ("download", f"{root}/exports/{export_id}/download"),
                        ("designer", f"{root}/designer"),
                    ):
                        compare_response(
                            f"E06 foreign tenant {label} denied",
                            oracle.get(path, headers=foreign),
                            rust.get(path, headers=foreign),
                        )
            finally:
                get_engine().dispose()

    print("E06 cartography read parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
