"""Real PostGIS R007 typed, revision, spatial and authority oracle regressions."""

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m2.test_r006_r009_real import _import_geojson
from tests.integration.spatial.test_features import _setup
from tests.integration.spatial.test_sources import _client


def _base(world):
    return f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"


def _point(fid, x, y, **properties):
    return {
        "type": "Feature",
        "id": fid,
        "properties": properties,
        "geometry": {"type": "Point", "coordinates": [x, y]},
    }


def _polygon(fid, west, south, east, north):
    return {
        "type": "Feature",
        "id": fid,
        "properties": {"name": fid},
        "geometry": {
            "type": "Polygon",
            "coordinates": [
                [[west, south], [east, south], [east, north], [west, north], [west, south]]
            ],
        },
    }


def _query(client, path, body):
    response = client.post(path, json=body)
    assert response.status_code == 200, response.text
    return response.json()


def _ids(client, path, flt):
    return _query(client, path, {"attribute_filter": flt})["selection_feature_ids"]


def test_typed_decimal_literal_and_revision_paging(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        target = _import_geojson(
            client,
            world,
            "query-values.geojson",
            [
                _point(
                    "a",
                    105.0,
                    21.0,
                    measure=1.1,
                    name="Đất %_x",
                    rank=0,
                    active=False,
                    observed="2026-11-21T00:00:00Z",
                ),
                _point(
                    "b",
                    105.1,
                    21.0,
                    measure=1.2,
                    name="Đất A",
                    rank=0,
                    active=True,
                    observed="2026-11-21T00:00:01Z",
                ),
                _point(
                    "c", 105.2, 21.0, measure=None, name=None, rank=2, active=False, observed=None
                ),
                _point(
                    "d",
                    105.3,
                    21.0,
                    measure=2.2,
                    name="",
                    rank=3,
                    active=False,
                    observed="2026-11-20T23:59:59Z",
                ),
            ],
            "Query values",
            "import",
        )
        layer = target["target_layer_id"]
        source = target["target_source_id"]
        path = f"{base}/layers/{layer}/query"
        edit = client.post(
            f"{base}/sources/{source}/changesets",
            json={
                "base_content_revision": target["source_content_revision"],
                "operation_id": "exact-decimals",
                "summary": "Exact precision fixture",
                "operations": [
                    {
                        "op_type": "update",
                        "feature_id": "a",
                        "properties": {
                            "measure": "1.000000000000000001",
                            "name": "Đất %_x",
                            "rank": 0,
                            "active": False,
                            "observed": "2026-11-21T00:00:00Z",
                        },
                    },
                    {
                        "op_type": "update",
                        "feature_id": "b",
                        "properties": {
                            "measure": "1.000000000000000002",
                            "name": "Đất A",
                            "rank": 0,
                            "active": True,
                            "observed": "2026-11-21T00:00:01Z",
                        },
                    },
                ],
            },
        )
        assert edit.status_code == 201, edit.text
        assert _ids(
            client, path, {"field": "observed", "op": "eq", "value": "2026-11-21T07:00:00+07:00"}
        ) == ["a"]
        assert _ids(
            client, path, {"field": "observed", "op": "gt", "value": "2026-11-21T07:00:00+07:00"}
        ) == ["b"]
        assert _ids(
            client, path, {"field": "measure", "op": "eq", "value": "1.000000000000000001"}
        ) == ["a"]
        assert _ids(
            client, path, {"field": "measure", "op": "gt", "value": "1.000000000000000001"}
        ) == ["b", "d"]
        assert _ids(client, path, {"field": "name", "op": "contains", "value": "%_"}) == ["a"]
        assert _ids(client, path, {"field": "name", "op": "prefix", "value": "Đất"}) == ["a", "b"]
        assert _ids(client, path, {"field": "name", "op": "contains", "value": "đất"}) == []
        assert _ids(
            client,
            path,
            {"op": "not", "condition": {"field": "name", "op": "contains", "value": "%_"}},
        ) == ["b", "d"]
        assert _ids(client, path, {"field": "name", "op": "eq", "value": ""}) == ["d"]
        assert _ids(client, path, {"field": "name", "op": "is_null"}) == ["c"]
        assert _ids(
            client,
            path,
            {
                "op": "and",
                "conditions": [
                    {"field": "rank", "op": "between", "value": [0, 2]},
                    {"field": "active", "op": "eq", "value": False},
                ],
            },
        ) == ["a", "c"]
        for wrong in (
            {"field": "rank", "op": "eq", "value": "0"},
            {"field": "rank", "op": "eq", "value": 0.2},
            {"field": "measure", "op": "eq", "value": 1.000000000000000001},
            {"field": "active", "op": "eq", "value": "false"},
            {"op": "and", "conditions": [7]},
        ):
            result = client.post(path, json={"attribute_filter": wrong})
            assert result.status_code == 422, result.text

        body = {"sort_field": "rank", "sort_direction": "asc", "limit": 1}
        first = _query(client, path, body)
        assert first["total_count"] == 4 and first["selection_feature_ids"] == ["a"]
        assert first["schema_revision"] and first["query_fingerprint"]
        continuation = body | {"expected_query_fingerprint": first["query_fingerprint"]}
        ids = first["selection_feature_ids"][:]
        for offset in (1, 2, 3):
            part = _query(client, path, continuation | {"offset": offset})
            ids.extend(part["selection_feature_ids"])
        assert ids == ["a", "b", "c", "d"] and not part["has_more"]
        final_page = _query(client, path, continuation | {"offset": 50_000})
        assert final_page["offset"] == 50_000
        assert not final_page["selection_feature_ids"] and not final_page["has_more"]
        assert client.post(path, json=continuation | {"offset": 50_001}).status_code == 422
        missing = client.post(path, json=body | {"offset": 1})
        assert missing.status_code == 409 and missing.json()["code"] == "MISSING_QUERY_PIN"
        changed = client.post(path, json=continuation | {"offset": 1, "sort_direction": "desc"})
        assert changed.status_code == 409 and changed.json()["code"] == "STALE_QUERY_PIN"
        assert client.post(path, json={"sort_direction": "wrong"}).status_code == 422
        assert client.post(path, json={"expected_schema_revision": "f" * 64}).status_code == 409
        edited = client.post(
            f"{base}/sources/{source}/changesets",
            json={
                "base_content_revision": first["content_revision"],
                "operation_id": "edit-after-page",
                "summary": "Invalidate page",
                "operations": [
                    {
                        "op_type": "update",
                        "feature_id": "a",
                        "properties": {
                            "measure": "1.000000000000000001",
                            "name": "Đất %_x",
                            "rank": 1,
                            "active": False,
                            "observed": "2026-11-21T00:00:00Z",
                        },
                    }
                ],
            },
        )
        assert edited.status_code == 201, edited.text
        stale = client.post(path, json=continuation | {"offset": 1})
        assert stale.status_code == 409 and stale.json()["code"] == "STALE_QUERY_PIN"


def test_spatial_explicit_bounds_reference_scope_and_saved_replay(migrated_database):
    world = seed_access_world()
    foreign = seed_access_world()
    base = _base(world)
    with _client(foreign.owner_user_id) as outsider:
        other = _import_geojson(
            outsider,
            foreign,
            "other.geojson",
            [
                _polygon("foreign", 105.0, 21.0, 106.0, 22.0),
            ],
            "Foreign",
            "import",
        )
    with _client(world.owner_user_id) as client:
        points = _import_geojson(
            client,
            world,
            "pts.geojson",
            [
                _point("boundary", 105.0, 21.0, name="boundary"),
                _point("inside", 105.05, 21.05, name="inside"),
                _point("outside", 105.15, 21.05, name="outside"),
            ],
            "Points",
            "import",
        )
        mask = _import_geojson(
            client,
            world,
            "mask.geojson",
            [
                _polygon("mask", 105.0, 21.0, 105.1, 21.1),
            ],
            "Mask",
            "import",
        )
        path = f"{base}/layers/{points['target_layer_id']}/query"
        sf = {"predicate": "within", "reference_layer_id": mask["target_layer_id"]}
        body = {"spatial_filter": sf, "limit": 1}
        current = _query(client, path, body)
        assert current["selection_feature_ids"] == ["inside"]
        assert current["reference_pin"]["layer_id"] == mask["target_layer_id"]
        assert _query(client, path, {"spatial_filter": sf | {"predicate": "intersects"}})[
            "selection_feature_ids"
        ] == ["boundary", "inside"]
        assert (
            _query(client, path, {"spatial_filter": sf | {"predicate": "contains"}})[
                "selection_feature_ids"
            ]
            == []
        )
        area = _polygon("ignored", 105.0, 21.0, 105.1, 21.1)["geometry"]
        drawn = {"predicate": "within", "geometry": area, "crs_code": 4326}
        assert _query(client, path, {"spatial_filter": drawn})["selection_feature_ids"] == [
            "inside"
        ]
        missing_crs = client.post(
            path, json={"spatial_filter": {"predicate": "within", "geometry": area}}
        )
        assert missing_crs.status_code == 422
        bad_geom = {
            "type": "Polygon",
            "coordinates": [
                [[105.0, 21.0], [105.1, 21.1], [105.0, 21.1], [105.1, 21.0], [105.0, 21.0]]
            ],
        }
        assert (
            client.post(path, json={"spatial_filter": drawn | {"geometry": bad_geom}}).status_code
            == 422
        )
        cross = client.post(
            path, json={"spatial_filter": sf | {"reference_layer_id": other["target_layer_id"]}}
        )
        assert cross.status_code == 404, cross.text
        assert (
            client.post(
                path, json={"spatial_filter": sf | {"reference_feature_id": "missing"}}
            ).status_code
            == 404
        )

        saved_body = {
            "layer_id": points["target_layer_id"],
            "name": "Pinned definition",
            "attribute_filter": {"field": "name", "op": "eq", "value": "inside"},
            "spatial_filter": sf,
        }
        saved = client.post(f"{base}/saved-queries", json=saved_body)
        assert saved.status_code == 201, saved.text
        reopened = next(
            x
            for x in client.get(f"{base}/saved-queries").json()["items"]
            if x["id"] == saved.json()["id"]
        )
        assert _query(
            client,
            path,
            {
                "attribute_filter": reopened["attribute_filter"],
                "spatial_filter": reopened["spatial_filter"],
            },
        )["selection_feature_ids"] == ["inside"]
        invalid_save = client.post(
            f"{base}/saved-queries",
            json=saved_body
            | {"spatial_filter": sf | {"reference_layer_id": other["target_layer_id"]}},
        )
        assert invalid_save.status_code == 404
        assert (
            client.post(
                f"{base}/saved-queries", json=saved_body | {"sort_field": "missing"}
            ).status_code
            == 422
        )

        edit = client.post(
            f"{base}/sources/{mask['target_source_id']}/changesets",
            json={
                "base_content_revision": current["reference_pin"]["content_revision"],
                "operation_id": "change-ref",
                "summary": "Invalidate ref pin",
                "operations": [
                    {"op_type": "update", "feature_id": "mask", "properties": {"name": "changed"}}
                ],
            },
        )
        assert edit.status_code == 201, edit.text
        stale = client.post(
            path,
            json=body | {"offset": 1, "expected_query_fingerprint": current["query_fingerprint"]},
        )
        assert stale.status_code == 409 and stale.json()["code"] == "STALE_QUERY_PIN"
        # 4000-reference fixture must be denied whole, not silently LIMIT 500.
        _, _, large_layer, _ = _setup(client, world, "reference_points_4326")
        oversized = client.post(
            path,
            json={"spatial_filter": {"predicate": "intersects", "reference_layer_id": large_layer}},
        )
        assert oversized.status_code == 422 and oversized.json()["code"] == "QUERY_TOO_COMPLEX"
    with _client(world.wrong_user_id) as stranger:
        assert stranger.post(path, json=body).status_code in (403, 404)
        assert stranger.get(f"{base}/saved-queries").status_code in (403, 404)


def test_approved_web_mercator_filter_transformation(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        target = _import_geojson(
            client,
            world,
            "points.geojson",
            [
                _point("inside", 0.001, 0.001, name="inside"),
                _point("outside", 1.0, 1.0, name="outside"),
            ],
            "Target",
            "import",
        )
        path = f"{base}/layers/{target['target_layer_id']}/query"
        # Independent analytical values: 0.001 degrees near equator ~=111 m,
        # so 0..200m projected rectangle contains only inside.
        area = _polygon("ignored", 0, 0, 200, 200)["geometry"]
        query = {"spatial_filter": {"predicate": "within", "geometry": area, "crs_code": 3857}}
        result = _query(client, path, query)
        assert result["selection_feature_ids"] == ["inside"]
        assert result["geometry_transformation"] == "epsg-1024-inverse"
        # Nonzero signed-area bowtie: Core ring validation alone is insufficient.
        crossed = {
            "type": "Polygon",
            "coordinates": [[[0, 0], [200, 200], [0, 200], [150, 0], [0, 0]]],
        }
        rejected = client.post(
            path,
            json={
                "spatial_filter": {
                    "predicate": "within",
                    "geometry": crossed,
                    "crs_code": 3857,
                }
            },
        )
        assert rejected.status_code == 422, rejected.text

        bad = client.post(
            path,
            json={"spatial_filter": {"predicate": "within", "geometry": area, "crs_code": 5899}},
        )
        assert bad.status_code == 422 and bad.json()["code"] == "UNSUPPORTED_CRS"
