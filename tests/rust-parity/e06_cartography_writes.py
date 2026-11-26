#!/usr/bin/env python3
"""E06 cartography and layout-designer write differential parity on isolated real PostGIS."""

from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import os
import sys
import tempfile
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import uuid4

import psycopg
from PIL import Image

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
    with psycopg.connect(owner_url) as conn:
        for idx in range(2):
            layout_id = uuid4()
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


def normalize_designer(body: dict) -> dict:
    out = json.loads(json.dumps(body))
    out["id"] = "<id>"
    out["created_at"] = "<created_at>"
    out["updated_at"] = "<updated_at>"
    return out


def normalize_preview(body: dict) -> dict:
    out = json.loads(json.dumps(body))
    if "revision_manifest" in out and isinstance(out["revision_manifest"], dict):
        out["revision_manifest"]["renderer_profile"] = "<renderer_profile>"
    if "revision_fingerprint" in out:
        out["revision_fingerprint"] = "<revision_fingerprint>"
    return out


@parity.case
def compare(label: str, expected, actual, normalize=lambda x: x):
    require(
        expected.status_code == actual.status_code,
        f"{label}: status {expected.status_code} != {actual.status_code}; "
        f"oracle={expected.text} rust={actual.text}",
    )
    if expected.status_code == 204:
        require(actual.text == "", f"{label}: Rust 204 body must be empty")
        print(f"PASS {label}", flush=True)
        return None
    try:
        ebody, abody = normalize(expected.json()), normalize(actual.json())
    except ValueError:
        require(expected.text == actual.text, f"{label}: text mismatch")
        return None
    mismatch = parity.difference(ebody, abody)
    require(mismatch is None, f"{label}: {mismatch}; oracle={ebody}; rust={abody}")
    print(f"PASS {label}", flush=True)
    return actual.json()


def audit_count(owner_url: str, fixture: dict, prefix: str) -> int:
    with psycopg.connect(owner_url) as conn:
        return conn.execute(
            """
            SELECT count(*) FROM audit.audit_events
            WHERE organization_id=%s AND action LIKE %s
            """,
            (fixture["org_a_id"], prefix + "%"),
        ).fetchone()[0]


def fill_designer_quota(owner_url: str, fixture: dict, doc: dict) -> None:
    with psycopg.connect(owner_url) as conn:
        for idx in range(50):
            conn.execute(
                """
                INSERT INTO cartography.designer_layouts (
                    organization_id, project_id, id, name, document, revision
                ) VALUES (%s,%s,%s,%s,%s::jsonb,1)
                """,
                (
                    fixture["org_a_id"],
                    fixture["project_a_id"],
                    uuid4(),
                    f"Quota {idx:02d}",
                    json.dumps(doc),
                ),
            )


def run_designer_parity(oracle, rust, owner_url, fixture, root, headers, csrf, foreign):
    designer_root = f"{root}/designer"
    camera = {"lng": 106.66, "lat": 10.70, "zoom": 14, "bearing": 10, "reference_width": 900}
    map_a, map_b = str(uuid4()), str(uuid4())
    doc = {
        "version": 1,
        "paper": "A4",
        "landscape": True,
        "dpi": 300,
        "items": [
            {
                "id": map_a,
                "kind": "map",
                "x": 5,
                "y": 20,
                "w": 150,
                "h": 140,
                "camera": camera,
                "grid": True,
            },
            {
                "id": map_b,
                "kind": "map",
                "x": 160,
                "y": 20,
                "w": 130,
                "h": 140,
                "camera": {**camera, "zoom": 17},
            },
            {
                "id": str(uuid4()),
                "kind": "scale",
                "x": 5,
                "y": 175,
                "w": 110,
                "h": 15,
                "map_id": map_a,
            },
            {
                "id": str(uuid4()),
                "kind": "north",
                "x": 120,
                "y": 175,
                "w": 20,
                "h": 20,
                "map_id": map_a,
            },
            {
                "id": str(uuid4()),
                "kind": "text",
                "x": 5,
                "y": 3,
                "w": 280,
                "h": 15,
                "text": "BẢN ĐỒ BÌNH HƯNG",
                "font_size": 6,
            },
        ],
    }
    payload = {"name": "  Layout 2 frames  ", "document": doc}
    before = audit_count(owner_url, fixture, "cartography.designer.")

    exp = oracle.post(designer_root, json=payload, headers=csrf)
    act = rust.post(designer_root, json=payload, headers=csrf)
    rust_created = compare("E06 designer create", exp, act, normalize_designer)
    require(exp.status_code == 201, exp.text)
    oracle_id = exp.json()["id"]
    rust_id = rust_created["id"]
    require(rust_created["name"] == "Layout 2 frames", rust_created)
    require(rust_created["revision"] == 1, rust_created)

    compare(
        "E06 designer list shared DB",
        oracle.get(designer_root, headers=headers),
        rust.get(designer_root, headers=headers),
    )

    update_payload = {"name": "  Revision 2  ", "document": doc, "revision": 1}
    exp_u = oracle.put(f"{designer_root}/{oracle_id}", json=update_payload, headers=csrf)
    act_u = rust.put(f"{designer_root}/{rust_id}", json=update_payload, headers=csrf)
    compare("E06 designer update", exp_u, act_u, normalize_designer)
    require(act_u.json()["revision"] == 2, act_u.text)

    compare(
        "E06 designer stale update conflict",
        oracle.put(f"{designer_root}/{oracle_id}", json=update_payload, headers=csrf),
        rust.put(f"{designer_root}/{rust_id}", json=update_payload, headers=csrf),
    )
    compare(
        "E06 designer stale delete conflict",
        oracle.delete(f"{designer_root}/{oracle_id}", params={"revision": 1}, headers=csrf),
        rust.delete(f"{designer_root}/{rust_id}", params={"revision": 1}, headers=csrf),
    )
    compare(
        "E06 designer delete",
        oracle.delete(f"{designer_root}/{oracle_id}", params={"revision": 2}, headers=csrf),
        rust.delete(f"{designer_root}/{rust_id}", params={"revision": 2}, headers=csrf),
    )

    require(
        audit_count(owner_url, fixture, "cartography.designer.") == before + 6,
        "expected create/update/delete designer audit rows from both runtimes",
    )
    print("PASS E06 designer audit append", flush=True)

    overflow = copy.deepcopy(doc)
    overflow["items"][0]["w"] = 300
    bad_map = copy.deepcopy(doc)
    bad_map["items"][2]["map_id"] = str(uuid4())
    extra_cam = copy.deepcopy(doc)
    extra_cam["items"][0]["camera"]["url"] = "https://example.com"
    bad_dpi = copy.deepcopy(doc)
    bad_dpi["paper"] = "A2"

    for label, bad_payload in (
        ("blank name", {"name": "   ", "document": doc}),
        ("element overflow", {"name": "bad", "document": overflow}),
        ("unknown map_id", {"name": "bad", "document": bad_map}),
        ("extra camera field", {"name": "bad", "document": extra_cam}),
        ("A2 at 300 dpi", {"name": "bad", "document": bad_dpi}),
    ):
        compare(
            f"E06 designer reject {label}",
            oracle.post(designer_root, json=bad_payload, headers=csrf),
            rust.post(designer_root, json=bad_payload, headers=csrf),
        )

    compare(
        "E06 designer foreign tenant denied",
        oracle.post(designer_root, json=payload, headers=foreign),
        rust.post(designer_root, json=payload, headers=foreign),
    )
    no_csrf = rust.post(designer_root, json=payload, headers=headers)
    require(no_csrf.status_code == 403, no_csrf.text)
    print("PASS E06 designer CSRF fail-closed", flush=True)

    fill_designer_quota(owner_url, fixture, doc)
    compare(
        "E06 designer quota 50",
        oracle.post(designer_root, json=payload, headers=csrf),
        rust.post(designer_root, json=payload, headers=csrf),
    )


def run_print_export_parity(oracle, rust, fixture, root, headers, csrf, foreign):
    edit_layer = fixture["layers"]["edit_4326"]["layer_id"]
    poly_layer = fixture["layers"]["epsg4326_1k"]["layer_id"]

    # Create a multi-layer A4 landscape layout for preview/export/download parity.
    layout_payload = {
        "name": "Pinned print",
        "title": "Pinned print map",
        "paper_size": "A4",
        "orientation": "landscape",
        "crs_code": 4326,
        "attribution": "OpenVietGIS Parity",
        "layer_ids": [poly_layer, edit_layer],
        "show_north_arrow": True,
        "show_scale_bar": True,
        "show_legend": True,
    }
    created = rust.post(f"{root}/layouts", json=layout_payload, headers=csrf)
    require(created.status_code == 201, created.text)
    layout_id = created.json()["id"]
    exports_path = f"{root}/layouts/{layout_id}/exports"

    settings_png = {"output_format": "png", "dpi": 150}
    exp_prev = oracle.post(f"{exports_path}/preview", json=settings_png, headers=csrf)
    act_prev = rust.post(f"{exports_path}/preview", json=settings_png, headers=csrf)
    rust_prev = compare("E06 print preview PNG", exp_prev, act_prev, normalize_preview)
    manifest = rust_prev["revision_manifest"]
    independent_fp = hashlib.sha256(
        json.dumps(
            manifest,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
            allow_nan=False,
        ).encode("utf-8")
    ).hexdigest()
    require(
        rust_prev["revision_fingerprint"] == independent_fp,
        f"fingerprint mismatch: {rust_prev['revision_fingerprint']} != {independent_fp}",
    )
    require(
        manifest["pixel_width"] == 1754 and manifest["pixel_height"] == 1240,
        manifest,
    )
    print("PASS E06 print revision_fingerprint SHA-256", flush=True)

    # Cross-CRS Web Mercator layout preview parity (4326 -> 3857 via ST_TransformPipeline).
    mercator_layout = dict(layout_payload)
    mercator_layout["name"] = "Mercator print"
    mercator_layout["crs_code"] = 3857
    mercator_layout["layer_ids"] = [edit_layer]
    m_res = rust.post(f"{root}/layouts", json=mercator_layout, headers=csrf)
    require(m_res.status_code == 201, m_res.text)
    m_id = m_res.json()["id"]
    compare(
        "E06 cross-CRS 4326->3857 print preview",
        oracle.post(f"{root}/layouts/{m_id}/exports/preview", json=settings_png, headers=csrf),
        rust.post(f"{root}/layouts/{m_id}/exports/preview", json=settings_png, headers=csrf),
        normalize_preview,
    )

    # Unsupported CRS transform (4326 -> 5899) rejected identically.
    vn_layout = dict(layout_payload)
    vn_layout["name"] = "Unsupported transform"
    vn_layout["crs_code"] = 5899
    vn_layout["layer_ids"] = [edit_layer]
    vn_res = rust.post(f"{root}/layouts", json=vn_layout, headers=csrf)
    require(vn_res.status_code == 201, vn_res.text)
    vn_id = vn_res.json()["id"]
    compare(
        "E06 unsupported print transform rejected",
        oracle.post(f"{root}/layouts/{vn_id}/exports/preview", json=settings_png, headers=csrf),
        rust.post(f"{root}/layouts/{vn_id}/exports/preview", json=settings_png, headers=csrf),
    )

    # Large paper at 300 DPI rejected identically.
    a2_layout = dict(layout_payload)
    a2_layout["name"] = "A2 layout"
    a2_layout["paper_size"] = "A2"
    a2_layout["layer_ids"] = [edit_layer]
    a2_res = rust.post(f"{root}/layouts", json=a2_layout, headers=csrf)
    require(a2_res.status_code == 201, a2_res.text)
    a2_id = a2_res.json()["id"]
    compare(
        "E06 A2 300 DPI rejected",
        oracle.post(
            f"{root}/layouts/{a2_id}/exports/preview",
            json={"output_format": "png", "dpi": 300},
            headers=csrf,
        ),
        rust.post(
            f"{root}/layouts/{a2_id}/exports/preview",
            json={"output_format": "png", "dpi": 300},
            headers=csrf,
        ),
    )

    # Missing fingerprint -> 422; stale fingerprint -> 409 STALE_PRINT_REVISION.
    compare(
        "E06 export missing fingerprint 422",
        oracle.post(exports_path, json=settings_png, headers=csrf),
        rust.post(exports_path, json=settings_png, headers=csrf),
    )
    compare(
        "E06 export stale fingerprint 409",
        oracle.post(
            exports_path,
            json={**settings_png, "expected_revision_fingerprint": "0" * 64},
            headers=csrf,
        ),
        rust.post(
            exports_path,
            json={**settings_png, "expected_revision_fingerprint": "0" * 64},
            headers=csrf,
        ),
    )

    # Execute real PNG export on Rust and verify artifact + download + Pillow parse!
    rust_png = rust.post(
        exports_path,
        json={**settings_png, "expected_revision_fingerprint": rust_prev["revision_fingerprint"]},
        headers=csrf,
    )
    require(rust_png.status_code == 201, rust_png.text)
    png_rec = rust_png.json()
    dl_png = rust.get(f"{root}/exports/{png_rec['id']}/download", headers=headers)
    require(dl_png.status_code == 200, dl_png.text)
    require(dl_png.headers.get("content-type") == "image/png", dl_png.headers)
    require(
        hashlib.sha256(dl_png.content).hexdigest()
        == png_rec["sha256"]
        == dl_png.headers.get("x-content-sha256"),
        "PNG sha256 mismatch",
    )
    require(len(dl_png.content) == png_rec["byte_size"], "PNG byte_size mismatch")
    with Image.open(io.BytesIO(dl_png.content)) as img:
        require(img.format == "PNG" and img.size == (1754, 1240), f"bad PNG {img.format} {img.size}")
    print("PASS E06 Rust PNG export + download + Pillow verification", flush=True)

    # Execute real PDF export on Rust and verify PDF container!
    settings_pdf = {"output_format": "pdf", "dpi": 150}
    pdf_prev = rust.post(f"{exports_path}/preview", json=settings_pdf, headers=csrf)
    require(pdf_prev.status_code == 200, pdf_prev.text)
    rust_pdf = rust.post(
        exports_path,
        json={
            **settings_pdf,
            "expected_revision_fingerprint": pdf_prev.json()["revision_fingerprint"],
        },
        headers=csrf,
    )
    require(rust_pdf.status_code == 201, rust_pdf.text)
    pdf_rec = rust_pdf.json()
    dl_pdf = rust.get(f"{root}/exports/{pdf_rec['id']}/download", headers=headers)
    require(dl_pdf.status_code == 200, dl_pdf.text)
    require(dl_pdf.headers.get("content-type") == "application/pdf", dl_pdf.headers)
    require(
        dl_pdf.content.startswith(b"%PDF-1.4\n") and dl_pdf.content.endswith(b"%%EOF\n"),
        "invalid PDF container",
    )
    require(
        hashlib.sha256(dl_pdf.content).hexdigest()
        == pdf_rec["sha256"]
        == dl_pdf.headers.get("x-content-sha256"),
        "PDF sha256 mismatch",
    )
    print("PASS E06 Rust PDF export + download verification", flush=True)

    # Style change invalidates previously previewed fingerprint.
    style_up = rust.put(
        f"{root}/layers/{edit_layer}/style",
        json={"mode": "single", "style_spec": {"fill_color": "#dc2626"}},
        headers=csrf,
    )
    require(style_up.status_code == 200, style_up.text)
    stale_after_style = rust.post(
        exports_path,
        json={**settings_png, "expected_revision_fingerprint": rust_prev["revision_fingerprint"]},
        headers=csrf,
    )
    require(
        stale_after_style.status_code == 409
        and stale_after_style.json()["code"] == "STALE_PRINT_REVISION",
        stale_after_style.text,
    )
    print("PASS E06 style mutation invalidates preview pin", flush=True)

    # Foreign tenant and missing CSRF fail-closed checks on preview/export.
    compare(
        "E06 foreign tenant preview denied",
        oracle.post(f"{exports_path}/preview", json=settings_png, headers=foreign),
        rust.post(f"{exports_path}/preview", json=settings_png, headers=foreign),
    )
    compare(
        "E06 foreign tenant export denied",
        oracle.post(
            exports_path,
            json={**settings_png, "expected_revision_fingerprint": "0" * 64},
            headers=foreign,
        ),
        rust.post(
            exports_path,
            json={**settings_png, "expected_revision_fingerprint": "0" * 64},
            headers=foreign,
        ),
    )
    require(
        rust.post(f"{exports_path}/preview", json=settings_png, headers=headers).status_code == 403,
        "preview without CSRF must fail",
    )
    require(
        rust.post(
            exports_path,
            json={**settings_png, "expected_revision_fingerprint": "0" * 64},
            headers=headers,
        ).status_code
        == 403,
        "export without CSRF must fail",
    )
    print("PASS E06 preview/export CSRF fail-closed", flush=True)


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

                    run_print_export_parity(oracle, rust, fixture, root, headers, csrf, foreign)

                    compare(
                        "E06 list exports after Rust PNG/PDF publish",
                        oracle.get(f"{root}/exports", headers=headers),
                        rust.get(f"{root}/exports", headers=headers),
                    )

                    run_designer_parity(
                        oracle, rust, owner_url, fixture, root, headers, csrf, foreign
                    )
            finally:
                get_engine().dispose()

    print("E06 cartography write parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
