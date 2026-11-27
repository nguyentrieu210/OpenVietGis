"""Independent durable-job, output-consumer and failure-atomicity regressions."""

from __future__ import annotations

import hashlib
from uuid import uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m2.test_r006_r009_real import _import_geojson
from tests.integration.spatial.test_sources import _client


def _base(world):
    return f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"


def _point(fid, x, y, value="north"):
    return {
        "type": "Feature",
        "id": fid,
        "properties": {"zone": value},
        "geometry": {"type": "Point", "coordinates": [x, y]},
    }


def _polygon(fid, west, south, east, north, value="north"):
    return {
        "type": "Feature",
        "id": fid,
        "properties": {"zone": value},
        "geometry": {
            "type": "Polygon",
            "coordinates": [
                [[west, south], [east, south], [east, north], [west, north], [west, south]]
            ],
        },
    }


def _job(client, base, algorithm, layer, parameters, **extra):
    body = {
        "algorithm_id": algorithm,
        "input_layer_id": layer,
        "parameters": parameters,
        "output_name": "Integrity output",
        "operation_id": str(uuid4()),
        **extra,
    }
    result = client.post(f"{base}/processing/jobs", json=body)
    assert result.status_code == 201, result.text
    return result.json()


def _read_output(client, base, job):
    assert job["status"] == "succeeded", job
    endpoint = f"{base}/layers/{job['output_layer_id']}"
    detail = client.get(endpoint)
    assert detail.status_code == 200, detail.text
    source = detail.json()["source"]
    pins = {name: source[name] for name in ("content_revision", "schema_revision")}
    assert job["output_content_revision"] == pins["content_revision"]
    response = client.get(endpoint + "/features", params=pins)
    assert response.status_code == 200, response.text
    items = response.json()["items"]
    assert len(items) == job["feature_count"]
    return source, pins, items


def _derived_counts(world):
    with get_admin_engine().connect() as connection:
        return tuple(
            connection.execute(
                text("""
            SELECT (
                SELECT COUNT(*) FROM spatial.data_sources
                WHERE organization_id=:org AND project_id=:proj AND kind='derived_processing'
            ), (
                SELECT COUNT(*) FROM spatial.layers l JOIN spatial.data_sources s
                    ON l.organization_id=s.organization_id AND l.project_id=s.project_id
                    AND l.source_id=s.id
                WHERE l.organization_id=:org AND l.project_id=:proj AND s.kind='derived_processing'
            )
            """),
                {"org": world.organization_id, "proj": world.project_id},
            ).one()
        )


def test_stale_job_fails_durably_without_repinning(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        imported = _import_geojson(
            client, world, "input.geojson", [_point("p1", 105.8, 21.0)], "Input", "import"
        )
        source, layer = imported["target_source_id"], imported["target_layer_id"]
        job = _job(client, base, "buffer", layer, {"distance_meters": 1000}, execute_now=False)
        old = job["input_refs"]
        assert old["input_schema_revision"]
        changeset = client.post(
            f"{base}/sources/{source}/changesets",
            json={
                "base_content_revision": old["input_content_revision"],
                "operation_id": "change-p1",
                "summary": "Change pinned source",
                "operations": [
                    {"op_type": "update", "feature_id": "p1", "properties": {"zone": "south"}}
                ],
            },
        )
        assert changeset.status_code == 201, changeset.text
        before = _derived_counts(world)
        for attempt in (2, 3):
            failed = client.post(f"{base}/processing/jobs/{job['id']}/retry")
            assert failed.status_code == 200, failed.text
            record = failed.json()
            assert record["status"] == "failed" and record["error_code"] == "STALE_INPUT_PIN"
            assert record["input_refs"] == old
            assert record["attempt_count"] == attempt
            assert record["output_source_id"] is None and record["output_layer_id"] is None
            assert client.get(f"{base}/processing/jobs/{job['id']}").json() == record
            assert _derived_counts(world) == before
        corrected = _job(client, base, "buffer", layer, {"distance_meters": 1000})
        _read_output(client, base, corrected)
        assert corrected["input_refs"]["input_content_revision"] != old["input_content_revision"]
    with _client(world.wrong_user_id) as stranger:
        assert stranger.get(f"{base}/processing/jobs/{job['id']}").status_code in (403, 404)
        assert stranger.post(f"{base}/processing/jobs/{job['id']}/retry").status_code in (403, 404)
        assert stranger.get(f"{base}/layers/{corrected['output_layer_id']}").status_code in (
            403,
            404,
        )


def test_clip_retains_family_and_empty_output(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        inputs = _import_geojson(
            client,
            world,
            "inputs.geojson",
            [
                _polygon("a_edge_touch", 105.0, 21.0, 105.1, 21.1),
                _polygon("z_overlap", 105.15, 21.0, 105.25, 21.1),
            ],
            "Inputs",
            "import",
        )
        mask = _import_geojson(
            client,
            world,
            "mask.geojson",
            [
                _polygon("mask", 105.1, 21.0, 105.2, 21.1),
            ],
            "Mask",
            "import",
        )
        clipped = _job(
            client,
            base,
            "clip",
            inputs["target_layer_id"],
            {},
            overlay_layer_id=mask["target_layer_id"],
        )
        source, _, rows = _read_output(client, base, clipped)
        assert source["schema"]["geometry_family"] == "Polygon"
        assert len(rows) == 1 and rows[0]["attributes"]["source_feature_id"] == "z_overlap"
        assert rows[0]["geometry"]["type"] == "Polygon"
        far = _import_geojson(
            client,
            world,
            "far.geojson",
            [
                _polygon("far", 105.5, 21.0, 105.6, 21.1),
            ],
            "Far",
            "import",
        )
        empty = _job(
            client,
            base,
            "clip",
            inputs["target_layer_id"],
            {},
            overlay_layer_id=far["target_layer_id"],
        )
        source, _, rows = _read_output(client, base, empty)
        assert not rows and source["schema"]["geometry_family"] == "Polygon"
        points = _import_geojson(
            client,
            world,
            "points.geojson",
            [
                _point("pt", 105.8, 21.0),
            ],
            "Point",
            "import",
        )
        empty_point = _job(
            client,
            base,
            "clip",
            points["target_layer_id"],
            {},
            overlay_layer_id=far["target_layer_id"],
        )
        point_source, _, rows = _read_output(client, base, empty_point)
        assert point_source["schema"]["geometry_family"] == "Point" and not rows
        nonpolygon = client.post(
            f"{base}/processing/jobs",
            json={
                "algorithm_id": "clip",
                "input_layer_id": inputs["target_layer_id"],
                "overlay_layer_id": points["target_layer_id"],
                "parameters": {},
                "operation_id": str(uuid4()),
                "output_name": "Invalid mask",
            },
        )
        assert nonpolygon.status_code == 422 and nonpolygon.json()["code"] == "INVALID_OVERLAY"


def test_atomic_rollback_on_publication_failure_and_consumers(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        imported = _import_geojson(
            client,
            world,
            "input.geojson",
            [
                _point("p1", 105.8, 21.0),
            ],
            "Input",
            "import",
        )
        layer = imported["target_layer_id"]
        before = _derived_counts(world)

        # Inject a scoped database failure, not a monkeypatch of the retired Python repository.
        suffix = uuid4().hex
        function = f"audit.e08_reject_{suffix}"
        trigger = f"e08_reject_{suffix}"
        with get_admin_engine().begin() as connection:
            connection.exec_driver_sql(
                f"CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ "
                f"BEGIN IF NEW.organization_id = '{world.organization_id}'::uuid "
                "AND NEW.action = 'processing.job.succeeded' THEN "
                "RAISE EXCEPTION 'synthetic E08 publication failure'; END IF; RETURN NEW; END $$"
            )
            connection.exec_driver_sql(
                f"CREATE TRIGGER {trigger} BEFORE INSERT ON audit.audit_events "
                f"FOR EACH ROW EXECUTE FUNCTION {function}()"
            )
        try:
            failed = _job(client, base, "buffer", layer, {"distance_meters": 12.5})
        finally:
            with get_admin_engine().begin() as connection:
                connection.exec_driver_sql(f"DROP TRIGGER {trigger} ON audit.audit_events")
                connection.exec_driver_sql(f"DROP FUNCTION {function}()")
        assert failed["status"] == "failed" and failed["error_code"]
        assert failed["output_layer_id"] is None and _derived_counts(world) == before
        response = client.post(f"{base}/processing/jobs/{failed['id']}/retry")
        assert response.status_code == 200, response.text
        job = response.json()
        _, pins, rows = _read_output(client, base, job)
        assert rows[0]["attributes"]["buffer_meters"] == "12.5"
        endpoint = f"{base}/layers/{job['output_layer_id']}"
        stale = client.get(endpoint + "/features", params=pins | {"content_revision": "f" * 64})
        assert stale.status_code == 409, stale.text
        viewport = client.get(
            endpoint + "/viewport",
            params=pins
            | {
                "west": 105.7,
                "south": 20.9,
                "east": 105.9,
                "north": 21.1,
            },
        )
        assert viewport.status_code == 200, viewport.text
        assert [item["feature_id"] for item in viewport.json()["items"]] == [rows[0]["feature_id"]]
        query = client.post(endpoint + "/query", json={"limit": 50, "offset": 0})
        assert query.status_code == 200, query.text
        assert len(query.json()["items"]) == 1
        export = {
            "layer_id": job["output_layer_id"],
            **pins,
            "output_format": "geopackage",
            "output_crs": 4326,
        }
        preview = client.post(f"{base}/exports/preview", json=export)
        assert preview.status_code == 200, preview.text
        approved = [
            item["code"] for item in preview.json()["disclosures"] if item["requires_approval"]
        ]
        confirmed = client.post(
            f"{base}/exports/confirm",
            json=export
            | {
                "operation_id": str(uuid4()),
                "approved_disclosures": approved,
                "preview_fingerprint": preview.json()["preview_fingerprint"],
            },
        )
        assert confirmed.status_code == 200, confirmed.text
        artifact = confirmed.json()["artifact"]
        download = client.get(f"{base}/artifacts/{artifact['id']}/download")
        assert download.status_code == 200
        assert download.content.startswith(b"SQLite format 3\x00")
        assert hashlib.sha256(download.content).hexdigest() == artifact["sha256"]


def test_idempotency_and_null_dissolve_groups(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        imported = _import_geojson(
            client,
            world,
            "groups.geojson",
            [
                _polygon("null", 105.0, 21.0, 105.1, 21.1, None),
                _polygon("literal", 105.05, 21.0, 105.15, 21.1, "__null__"),
            ],
            "Groups",
            "import",
        )
        layer = imported["target_layer_id"]
        body = {
            "algorithm_id": "dissolve",
            "input_layer_id": layer,
            "parameters": {"group_by_field": "zone"},
            "output_name": "Grouped",
            "operation_id": "group-op",
        }
        first = client.post(f"{base}/processing/jobs", json=body)
        assert first.status_code == 201, first.text
        job = first.json()
        _, _, rows = _read_output(client, base, job)
        assert {item["attributes"]["group_value"] for item in rows} == {None, "__null__"}
        assert all(item["attributes"]["merged_count"] == 1 for item in rows)
        repeated = client.post(f"{base}/processing/jobs", json=body)
        assert repeated.status_code == 201 and repeated.json()["id"] == job["id"]
        conflict = client.post(f"{base}/processing/jobs", json=body | {"parameters": {}})
        assert conflict.status_code == 409 and conflict.json()["code"] == "OPERATION_ID_CONFLICT"
