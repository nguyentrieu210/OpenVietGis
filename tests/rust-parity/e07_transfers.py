#!/usr/bin/env python3
"""Strict E07 round-trip gate against both the baseline and Rust candidate.

Uses the same isolated database/session setup as run.py, never production.
Each engine creates its own assets and jobs for GeoJSON, GeoPackage and ZIP
Shapefile. Generated IDs may differ; semantic reports, revision digests and
GeoJSON payloads must agree. Binary outputs are re-inspected, not byte-compared.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import tempfile
import time
import zipfile
from pathlib import Path
from uuid import uuid4

from run import (
    ROOT,
    SEED,
    baseline,
    configure_python,
    database,
    project_path,
    require,
    rust_client,
    session_headers,
    verify_database_role,
)


def response(res, status):
    require(
        res.status_code == status,
        f"{res.request.method} {res.request.url.path}: expected {status}, got "
        f"{res.status_code}: {res.text[:600]}",
    )
    return res.json()


def fixtures():
    from openvietgis_api.adapters.formats.geopackage import write_geopackage
    from openvietgis_api.adapters.formats.shapefile_zip import write_shapefile_zip
    from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
    from openvietgis_api.domains.transfers.domain import ExportFieldMapping

    points = [
        ("p1", {"code": "Điểm A", "rank": 0, "active": False}, (106.6, 10.7)),
        ("p2", {"code": "", "rank": None, "active": None}, (106.7, 10.8)),
    ]
    doc = {
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "id": fid,
                "properties": attrs,
                "geometry": {"type": "Point", "coordinates": coords},
            }
            for fid, attrs, coords in points
        ],
    }
    yield "geojson", "sample.geojson", json.dumps(doc, ensure_ascii=False).encode()
    mappings = [
        ExportFieldMapping(name, name, name, kind)
        for name, kind in [
            ("code", FieldType.TEXT),
            ("rank", FieldType.INTEGER),
            ("active", FieldType.BOOLEAN),
        ]
    ]
    args = {
        "layer_name": "e07_points",
        "crs_code": 4326,
        "geometry_family": "Point",
        "field_mappings": mappings,
        "features": [
            (fid, attrs, NativeGeometry("Point", coords, 4326)) for fid, attrs, coords in points
        ],
    }
    yield "geopackage", "sample.gpkg", write_geopackage(**args)[0]
    yield "shapefile_zip", "sample.zip", write_shapefile_zip(**args)[0]


def roundtrip(client, manifest, name, fmt, filename, data):
    base = project_path(manifest["org_a_id"], manifest["project_a_id"])
    headers = session_headers(
        manifest["session_a_token"], **{"x-openvietgis-csrf": manifest["csrf_a_token"]}
    )
    outsider = session_headers(
        manifest["session_b_token"], **{"x-openvietgis-csrf": manifest["csrf_b_token"]}
    )
    files = {"file": (filename, data, "application/octet-stream")}
    require(
        client.post(f"{base}/imports/upload", files=files).status_code == 401,
        "unauthenticated upload was not denied",
    )
    require(
        client.post(
            f"{base}/imports/upload",
            files=files,
            headers=session_headers(manifest["session_a_token"]),
        ).status_code
        == 403,
        "missing CSRF was not denied",
    )
    upload = response(client.post(f"{base}/imports/upload", files=files, headers=headers), 201)
    asset, inspection = upload["asset"], upload["inspection"]
    require(asset["sha256"] == hashlib.sha256(data).hexdigest(), "asset digest differs")
    require(inspection["detected_format"] == fmt, "wrong format")
    require(
        inspection["confirmable"] and inspection["feature_count"] == 2, "fixture not confirmable"
    )
    inspect_body = {
        "source_asset_id": asset["id"],
        "selected_member": inspection["selected_member"],
        "target_crs": 4326,
        "declared_source_crs": inspection["resolved_source_crs"],
        "declared_encoding": inspection["resolved_encoding"],
    }
    foreign = project_path(manifest["org_b_id"], manifest["project_b_id"])
    require(
        client.post(f"{foreign}/imports/inspect", json=inspect_body, headers=outsider).status_code
        == 404,
        "cross-tenant asset leak",
    )
    inspected = response(
        client.post(f"{base}/imports/inspect", json=inspect_body, headers=headers), 200
    )["inspection"]
    body = dict(
        inspect_body,
        operation_id=str(uuid4()),
        source_name=f"E07 {name} {fmt}",
        layer_name=f"E07 {name} {fmt}",
        approved_conversions=[
            c["code"] for c in inspected["conversions"] if c["requires_approval"]
        ],
        inspection_fingerprint=inspected["inspection_fingerprint"],
    )
    stale = dict(body, inspection_fingerprint="0" * 64)
    require(
        client.post(f"{base}/imports/confirm", json=stale, headers=headers).status_code == 409,
        "stale inspection accepted",
    )
    job = response(client.post(f"{base}/imports/confirm", json=body, headers=headers), 200)
    require(job["status"] == "succeeded", "import job failed")
    replay = response(client.post(f"{base}/imports/confirm", json=body, headers=headers), 200)
    require(replay["id"] == job["id"], "import replay created another job")
    require(
        client.post(
            f"{base}/imports/confirm", json=dict(body, layer_name="Changed intent"), headers=headers
        ).status_code
        == 409,
        "operation intent conflict accepted",
    )
    result = job["result_report"]
    require(
        result["feature_count"] == 2 and result["stored_crs"] == 4326,
        "wrong imported data count/CRS",
    )
    export = {
        "layer_id": job["target_layer_id"],
        "content_revision": result["content_revision"],
        "schema_revision": result["schema_revision"],
        "output_format": fmt,
        "output_crs": 4326,
    }
    preview = response(client.post(f"{base}/exports/preview", json=export, headers=headers), 200)
    confirmed = dict(
        export,
        operation_id=str(uuid4()),
        preview_fingerprint=preview["preview_fingerprint"],
        approved_disclosures=[d["code"] for d in preview["disclosures"] if d["requires_approval"]],
    )
    exported = response(
        client.post(f"{base}/exports/confirm", json=confirmed, headers=headers), 200
    )
    require(exported["status"] == "succeeded", "export job failed")
    artifact = exported["artifact"]
    require(artifact["comparison_report"]["exported_feature_count"] == 2, "export count differs")
    path = f"{base}/artifacts/{artifact['id']}/download"
    download = client.get(path, headers=headers)
    require(download.status_code == 200, "download failed")
    require(
        download.headers.get("x-content-sha256")
        == artifact["sha256"]
        == hashlib.sha256(download.content).hexdigest(),
        "download digest differs",
    )
    require(client.get(path, headers=outsider).status_code == 404, "cross-tenant artifact leak")
    again = response(client.post(f"{base}/exports/confirm", json=confirmed, headers=headers), 200)
    require(again["id"] == exported["id"], "export replay created another job")
    require(
        response(client.get(f"{base}/jobs/{job['id']}", headers=headers), 200)["status"]
        == "succeeded",
        "durable job lookup failed",
    )
    if fmt == "geojson":
        payload = download.json()
        require(
            payload["type"] == "FeatureCollection" and len(payload["features"]) == 2,
            "invalid GeoJSON export",
        )
    else:
        # Re-import the binary artifact with the reference adapter via HTTP;
        # no assumption that different writers produce byte-identical binary files.
        payload = None
    second = response(
        client.post(
            f"{base}/imports/upload",
            files={"file": (filename, download.content, "application/octet-stream")},
            headers=headers,
        ),
        201,
    )["inspection"]
    require(
        second["feature_count"] == 2 and second["resolved_source_crs"] == 4326,
        "round-trip inspection differs",
    )
    if fmt == "geojson" and name == "rust":
        check_binary_exports(client, base, headers, outsider, export)
    return {
        "source_crs": result["source_crs"],
        "stored_crs": result["stored_crs"],
        "schema_revision": result["schema_revision"],
        "content_revision": result["content_revision"],
        "field_mappings": preview["field_mappings"],
        "disclosures": preview["disclosures"],
        "geojson": payload,
    }


def check_binary_exports(client, base, headers, outsider, export):
    from openvietgis_api.adapters.formats.geopackage import (
        inspect_geopackage,
        parse_geopackage_member,
    )
    from openvietgis_api.adapters.formats.shapefile_zip import (
        inspect_shapefile_zip,
        parse_shapefile_member,
    )

    for fmt, media, inspect, parse in [
        ("geopackage", "application/geopackage+sqlite3", inspect_geopackage, parse_geopackage_member),
        ("shapefile_zip", "application/zip", inspect_shapefile_zip, parse_shapefile_member),
    ]:
        options = dict(export, output_format=fmt)
        preview = response(client.post(f"{base}/exports/preview", json=options, headers=headers), 200)
        request = dict(
            options,
            operation_id=str(uuid4()),
            preview_fingerprint=preview["preview_fingerprint"],
            approved_disclosures=[d["code"] for d in preview["disclosures"] if d["requires_approval"]],
        )
        job = response(client.post(f"{base}/exports/confirm", json=request, headers=headers), 200)
        require(job["status"] == "succeeded", f"{fmt} export failed")
        artifact = job["artifact"]
        path = f"{base}/artifacts/{artifact['id']}/download"
        download = client.get(path, headers=headers)
        require(download.status_code == 200, f"{fmt} download failed")
        require(download.headers["content-type"].split(";")[0] == media, "binary media type mismatch")
        require(hashlib.sha256(download.content).hexdigest() == artifact["sha256"], "binary digest mismatch")
        require(client.get(path, headers=outsider).status_code == 404, "binary cross-tenant leak")
        members = inspect(download.content)
        require(len(members) == 1 and members[0].eligible, "invalid binary vector member")
        parsed = parse(download.content, member_name=members[0].name, declared_crs=4326)
        require(parsed.detected_crs == 4326 and len(parsed.features) == 2, "binary geometry/CRS mismatch")
        require(not parsed.findings, "binary export failed independent reader validation")
        require(
            response(client.post(f"{base}/exports/confirm", json=request, headers=headers), 200)["id"]
            == job["id"], "binary replay created another job",
        )


def unsafe_archive(name="../escape.shp", *, symlink=False):
    buff = io.BytesIO()
    with zipfile.ZipFile(buff, "w") as archive:
        info = zipfile.ZipInfo(name)
        if symlink:
            info.create_system = 3
            info.external_attr = 0o120777 << 16
        archive.writestr(info, b"" if name.endswith("/") else b"not a shapefile")
    return buff.getvalue()


def corrupt_archive():
    buff = io.BytesIO()
    with zipfile.ZipFile(buff, "w", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("sample.shp", b"ORIGINAL")
    return buff.getvalue().replace(b"ORIGINAL", b"CORRUPTD", 1)


def duplicate_archive():
    buff = io.BytesIO()
    with zipfile.ZipFile(buff, "w") as archive:
        archive.writestr("sample.shp", b"a")
        archive.writestr("SAMPLE.SHP", b"b")
    return buff.getvalue()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--candidate", type=Path, default=ROOT / "target/debug/openvietgis-api-rust"
    )
    args = parser.parse_args()
    binary = args.candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust candidate first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    failures = []
    with tempfile.TemporaryDirectory(prefix="ovg-e07-parity-") as directory:
        work = Path(directory)
        os.chdir(work)
        with database(admin) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            configure_python(owner_url, runtime_url, artifacts)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            manifest = baseline.seed_benchmark_database(
                owner_url, seed=SEED, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
            )
            verify_database_role(runtime_url)
            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    rust_client(binary, runtime_url, work) as rust,
                ):
                    for fmt, filename, data in fixtures():
                        try:
                            start = time.monotonic()
                            expected = roundtrip(oracle, manifest, "python", fmt, filename, data)
                            oracle_ms = (time.monotonic() - start) * 1000
                            start = time.monotonic()
                            actual = roundtrip(rust, manifest, "rust", fmt, filename, data)
                            rust_ms = (time.monotonic() - start) * 1000
                            differences = {
                                key: {"expected": expected[key], "actual": actual.get(key)}
                                for key in expected
                                if actual.get(key) != expected[key]
                            }
                            require(
                                actual == expected,
                                f"semantic or revision mismatch: {fmt}: "
                                f"{json.dumps(differences, ensure_ascii=False, sort_keys=True)}",
                            )
                            print(
                                f"PASS E07 {fmt} roundtrip; "
                                f"Python {oracle_ms:.1f} ms Rust {rust_ms:.1f} ms "
                                "(single sample)",
                                flush=True,
                            )
                        except (AssertionError, KeyError, ValueError) as exc:
                            failures.append(fmt)
                            print(f"FAIL E07 {fmt}: {exc}", flush=True)
                    base = project_path(manifest["org_a_id"], manifest["project_a_id"])
                    headers = session_headers(
                        manifest["session_a_token"],
                        **{"x-openvietgis-csrf": manifest["csrf_a_token"]},
                    )
                    for name, data, code in [
                        (
                            "legacy.geojson",
                            b'{"type":"FeatureCollection","crs":{},"features":[]}',
                            "LEGACY_GEOJSON_CRS",
                        ),
                        ("traversal.zip", unsafe_archive(), "ARCHIVE_PATH_TRAVERSAL"),
                        (
                            "dir-traversal.zip", unsafe_archive("../escape/"),
                            "ARCHIVE_PATH_TRAVERSAL",
                        ),
                        (
                            "win-traversal.zip", unsafe_archive("C:/escape.shp"),
                            "ARCHIVE_PATH_TRAVERSAL",
                        ),
                        (
                            "symlink.zip", unsafe_archive("link.shp", symlink=True),
                            "ARCHIVE_SYMLINK_REJECTED",
                        ),
                        ("duplicate.zip", duplicate_archive(), "ARCHIVE_DUPLICATE_ENTRY"),
                        ("corrupt.zip", corrupt_archive(), "INVALID_ZIP"),
                    ]:
                        try:
                            for label, client in [("Python", oracle), ("Rust", rust)]:
                                res = client.post(
                                    f"{base}/imports/upload",
                                    files={"file": (name, data)},
                                    headers=headers,
                                )
                                expected_code = code
                                require(
                                    res.status_code == 422
                                    and res.json().get("code") == expected_code,
                                    f"{label} expected {expected_code}, got "
                                    f"{res.status_code}: {res.text[:300]}",
                                )
                            print(f"PASS E07 {code}", flush=True)
                        except (AssertionError, ValueError) as exc:
                            failures.append(code)
                            print(f"FAIL E07 {code}: {exc}", flush=True)
            finally:
                get_engine().dispose()
    print(f"E07 strict gate: {len(failures)} failure(s): {', '.join(failures)}", flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
