"""SC-007 warmed restricted HTTP timings on the complete 10k reference profile.

Assertions and fixture creation are outside measured request completion time.
Every scenario performs 100 complete requests after five warm-ups, with fresh
request authorization/transactions and actual JSON geometry serialization.
"""

from __future__ import annotations

import json
import os
import platform
from collections.abc import Callable
from math import ceil
from pathlib import Path
from statistics import median
from time import perf_counter_ns

import pytest
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_features import PRIMARY, _setup
from tests.integration.spatial.test_layers import _counts
from tests.integration.spatial.test_layers import _path as _layer_path
from tests.integration.spatial.test_sources import _client
from tests.integration.spatial.test_sources import _path as _source_path

SAMPLES = 100
WARMUPS = 5
ROOT = Path(__file__).resolve().parents[4]


def _environment() -> dict:
    with get_engine().connect() as connection:
        role = connection.execute(
            text(
                "SELECT current_user,rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user"
            )
        ).one()
        assert tuple(role) == ("openvietgis_runtime", False, False)
        versions = (
            connection.execute(
                text("""
                SELECT current_setting('server_version') AS postgres,
                       postgis_full_version() AS postgis_full,
                       postgis_lib_version() AS postgis,
                       postgis_proj_version() AS proj
            """)
            )
            .mappings()
            .one()
        )
    compose = (ROOT / "infra/dev/compose.yaml").read_text(encoding="utf-8")
    images = sorted(
        {line.split("image:", 1)[1].strip() for line in compose.splitlines() if "image:" in line}
    )
    return {
        "runner_name": os.getenv("RUNNER_NAME", platform.node()),
        "runner_os": os.getenv("RUNNER_OS", platform.system()),
        "python": platform.python_version(),
        "architecture": platform.machine(),
        "commit": os.getenv("GITHUB_SHA", "local-unrecorded"),
        "declared_database_images": images,
        "observed_database_image_digest": os.getenv("OPENVIETGIS_TEST_DATABASE_IMAGE_DIGEST"),
        "database_role": role[0],
        **dict(versions),
        "measurement": "complete in-process HTTP response, real restricted PostgreSQL, no profiler",
        "samples_per_scenario": SAMPLES,
        "warmups_per_scenario": WARMUPS,
    }


def test_spatial_http_p95_10k_native_and_transformed(
    migrated_database: None,
    capsys: pytest.CaptureFixture[str],
) -> None:
    world = seed_access_world()
    environment = _environment()
    results: dict[str, dict] = {}
    fixture_records = []

    with _client(world.owner_user_id) as client:
        datasets = []
        for key, prefix, family, count in PRIMARY:
            setup_started = perf_counter_ns()
            path, source, layer_id, pins = _setup(client, world, key)
            with capsys.disabled():
                print(
                    f"R003_FIXTURE {key} count={count} "
                    f"setup_ms={(perf_counter_ns() - setup_started) / 1_000_000:.1f}",
                    flush=True,
                )
            assert source["validation_state"] == "ready"
            assert source["feature_count"] == count
            datasets.append((path, source, layer_id, pins, prefix, family))
            fixture_records.append(
                {
                    "key": key,
                    "family": family,
                    "count": count,
                    "fixture_version": source["provenance"]["fixture_version"],
                    "manifest_digest": source["provenance"]["manifest_digest"],
                    **pins,
                }
            )
        before_counts = _counts(world)
        assert sum(item[3] for item in PRIMARY) == before_counts[2] == 10000

        def measure(
            name: str,
            url: str,
            params: dict,
            check: Callable[[dict], None],
            budget_ms: float,
        ) -> None:
            samples: list[float] = []
            for iteration in range(WARMUPS + SAMPLES):
                started = perf_counter_ns()
                response = client.get(url, params=params)
                elapsed = (perf_counter_ns() - started) / 1_000_000
                assert response.status_code == 200, response.text
                check(response.json())
                if iteration >= WARMUPS:
                    samples.append(elapsed)
            assert len(samples) == SAMPLES
            results[name] = {
                "samples": len(samples),
                "warmups": WARMUPS,
                "p50_ms": median(samples),
                "p95_ms": sorted(samples)[ceil(0.95 * len(samples)) - 1],
                "max_ms": max(samples),
                "budget_p95_ms": budget_ms,
            }
            with capsys.disabled():
                print(
                    f"R003_SCENARIO {name} "
                    f"p95_ms={results[name]['p95_ms']:.3f} budget_ms={budget_ms:.1f}",
                    flush=True,
                )

        def check_catalog(body: dict) -> None:
            assert body["organization_id"] == str(world.organization_id)
            assert body["project_id"] == str(world.project_id)
            assert len(body["items"]) == 3
            assert body["next_after_id"] is None

        measure("source_list", _source_path(world), {}, check_catalog, 1000.0)
        measure("layer_list", _layer_path(world), {}, check_catalog, 1000.0)
        for path, source, layer_id, pins, prefix, family in datasets:

            def check_source(body: dict, expected=source) -> None:
                assert body == expected

            def check_layer(body: dict, expected=layer_id) -> None:
                assert body["id"] == expected

            measure(
                f"{family}_source_detail",
                _source_path(world) + "/" + source["id"],
                {},
                check_source,
                1000.0,
            )
            measure(
                f"{family}_layer_detail",
                _layer_path(world) + "/" + layer_id,
                {},
                check_layer,
                1000.0,
            )
            for response_crs in (4326, 3857):
                query = {**pins, "response_crs": response_crs}

                def check_features(
                    body: dict,
                    code=response_crs,
                    detail=False,
                    source=source,
                    layer_id=layer_id,
                    pins=pins,
                    prefix=prefix,
                    family=family,
                ) -> None:
                    context = body["context"]
                    assert context["source_id"] == source["id"]
                    assert context["layer_id"] == layer_id
                    assert all(context[key] == value for key, value in pins.items())
                    assert context["response_crs"]["code"] == code
                    assert (context["transformation"] is None) == (code == 4326)
                    features = [body["feature"]] if detail else body["items"]
                    assert len(features) == (1 if detail else 100)
                    assert features[0]["feature_id"] == f"{prefix}-000001"
                    if not detail:
                        assert features[-1]["feature_id"] == f"{prefix}-000100"
                        assert body["next_after_feature_id"] == f"{prefix}-000100"
                    assert all(
                        item["geometry"]["type"] == family
                        and item["geometry"]["coordinate_reference"]["code"] == code
                        for item in features
                    )

                measure(
                    f"{family}_page100_{response_crs}",
                    path,
                    {**query, "limit": 100},
                    check_features,
                    2000.0,
                )
                measure(
                    f"{family}_feature_detail_{response_crs}",
                    path + f"/{prefix}-000001",
                    query,
                    lambda body: check_features(body, detail=True),
                    1000.0,
                )
        assert _counts(world) == before_counts, "benchmark reads must not mutate catalog/storage"

    report = {"environment": environment, "fixtures": fixture_records, "results": results}
    destination = ROOT / "dist/performance/r003-spatial.json"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    # CI deliberately suppresses captured output; publish this non-secret,
    # synthetic measurement explicitly so exact runner evidence is reviewable.
    with capsys.disabled():
        print("R003_SPATIAL_BENCHMARK " + json.dumps(report, sort_keys=True), flush=True)
    slow = {name: item for name, item in results.items() if item["p95_ms"] > item["budget_p95_ms"]}
    assert not slow, json.dumps(slow, sort_keys=True)
