"""Measure pinned viewport, feature page and MVT HTTP behavior on synthetic PostGIS."""

from __future__ import annotations

import hashlib
import json
import os
import resource
import time
from math import asinh, ceil, pi, radians, tan
from statistics import median
from uuid import uuid4

from fastapi.testclient import TestClient
from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.config import get_settings
from openvietgis_api.domains.maps.viewport import MAX_RENDER_SOURCE_FEATURES
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.spatial.test_sources import _client

from tools.test_database import assert_safe_test_target

SIZES = (10_000, 100_000, 1_000_000)
WARMUP_REQUESTS = 3
TIMED_REQUESTS = 20
INSERT_GRID = """
    INSERT INTO spatial.reference_features (
        organization_id, project_id, source_id, feature_id, attributes, geometry,
        geometry_state, content_revision, schema_revision
    )
    SELECT :org, :project, :source, 'scale-' || lpad(i::text, 7, '0'),
        '{}'::jsonb, ST_SetSRID(ST_MakePoint(
            106.8 + ((i * 7919) % 1000000) / 5000000.0,
            11.4 + ((i * 104729) % 1000000) / 5000000.0
        ), 4326), 'present', :content, :schema
    FROM generate_series(1::bigint, CAST(:size AS bigint)) AS series(i)
"""
GRID_SHA256 = hashlib.sha256(INSERT_GRID.encode()).hexdigest()


def measure(client: TestClient, path: str, params: dict) -> dict[str, float | int]:
    timings = []
    body_bytes = None
    cpu_start = time.process_time()
    for i in range(WARMUP_REQUESTS + TIMED_REQUESTS + 1):
        started = time.perf_counter()
        response = client.get(path, params=params)
        elapsed_ms = (time.perf_counter() - started) * 1_000
        if response.status_code != 200:
            return {
                "http_status": response.status_code,
                "failed_request_ms": round(elapsed_ms, 3),
                "successful_timed_requests": len(timings),
            }
        if i == 0:
            first_ms = elapsed_ms
            body_bytes = len(response.content)
        assert len(response.content) == body_bytes
        if i > WARMUP_REQUESTS:
            timings.append(elapsed_ms)
    assert body_bytes is not None
    return {
        "first_request_ms": round(first_ms, 3),
        "p50_ms": round(median(timings), 3),
        "p95_ms": round(sorted(timings)[ceil(len(timings) * 0.95) - 1], 3),
        "response_bytes": body_bytes,
        "cpu_seconds": round(time.process_time() - cpu_start, 3),
        "process_peak_rss_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
    }


def seed_layer(client: TestClient, world: AccessWorld, root: str, size: int):
    response = client.post(
        root + "/layers/manual",
        json={
            "operation_id": str(uuid4()),
            "name": f"Spec 059 synthetic grid {size}",
            "geometry_family": "Point",
            "fields": [],
        },
    )
    assert response.status_code == 201, response.text
    layer = response.json()
    path = root + "/layers/" + layer["layer_id"]
    source = client.get(path).json()["source"]
    pins = {
        "content_revision": hashlib.sha256(f"{GRID_SHA256}:{size}".encode()).hexdigest(),
        "schema_revision": source["schema_revision"],
    }
    bind = {
        "org": world.organization_id,
        "project": world.project_id,
        "source": layer["source_id"],
        "content": pins["content_revision"],
        "schema": pins["schema_revision"],
        "size": size,
    }
    extent = json.dumps(
        {"min_x": 106.8, "min_y": 11.4, "max_x": 107.0, "max_y": 11.6, "crs_code": 4326}
    )
    with get_admin_engine().begin() as db:
        db.execute(text("SET LOCAL statement_timeout = '180s'"))
        db.execute(
            text(
                "UPDATE spatial.data_sources SET kind='reference_fixture', "
                "fixture_version='spec059-grid-v1', manifest_digest=:fixture_digest, "
                "feature_count=:size, extent_state='available', extent=CAST(:extent AS jsonb), "
                "content_revision=:content WHERE id=:source AND project_id=:project"
            ),
            bind | {"extent": extent, "fixture_digest": GRID_SHA256},
        )
        assert db.execute(text(INSERT_GRID), bind).rowcount == size
        db.execute(text("ANALYZE spatial.reference_features"))
    assert client.get(path).json()["source"]["feature_count"] == size
    return layer["layer_id"], path, pins


def run_scale() -> None:
    if get_settings().environment != "test":
        raise RuntimeError("Run only on a fresh synthetic PostGIS test database.")
    assert_safe_test_target(get_settings().migration_db_url)
    assert_safe_test_target(get_settings().runtime_db_url)
    with get_engine().connect() as db:
        role = db.execute(
            text(
                "SELECT current_user, rolsuper, rolbypassrls FROM pg_roles "
                "WHERE rolname=current_user"
            )
        ).one()
        assert tuple(role) == ("openvietgis_runtime", False, False)
    sizes = tuple(
        int(part)
        for part in os.getenv("OVG059_SCALE_SIZES", "10000,100000,1000000").split(",")
    )
    if not sizes or not set(sizes) <= set(SIZES):
        raise RuntimeError(f"Allowed dataset sizes: {SIZES}")
    print(
        json.dumps(
            {
                "scope": "FastAPI test transport, real PostGIS, one client, Linux RSS",
                "cpu_count": os.cpu_count(),
                "fixture_sha256": GRID_SHA256,
                "warmups": WARMUP_REQUESTS,
                "timed_requests": TIMED_REQUESTS,
                "runtime_role": role[0],
            }
        ),
        flush=True,
    )
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    with _client(world.owner_user_id, raise_server_exceptions=False) as client:
        for size in sizes:
            layer, path, pins = seed_layer(client, world, root, size)
            z = 14
            x = int((106.9 + 180) / 360 * 2**z)
            y = int((1 - asinh(tan(radians(11.5))) / pi) / 2 * 2**z)
            tiles = f"{root}/pro/tiles/{layer}/{z}/{x}/{y}.mvt"
            bbox = {"west": 106.89, "south": 11.49, "east": 106.91, "north": 11.51}
            page = pins | {"limit": 100, "response_crs": 4326}
            scenarios: dict[str, object] = {}
            if size <= MAX_RENDER_SOURCE_FEATURES:
                scenarios["viewport"] = measure(client, path + "/viewport", pins | bbox)
            else:
                response = client.get(path + "/viewport", params=pins | bbox)
                assert response.status_code == 422, response.text
                assert response.json()["code"] == "UNSUPPORTED_TRANSFORMATION"
                scenarios["viewport"] = {"status": 422, "profile_cap": MAX_RENDER_SOURCE_FEATURES}
            scenarios["feature_page"] = measure(client, path + "/features", page)
            scenarios["cluster_tile"] = measure(client, tiles, pins)
            print(json.dumps({"rows": size, **scenarios}), flush=True)


if __name__ == "__main__":
    run_scale()
