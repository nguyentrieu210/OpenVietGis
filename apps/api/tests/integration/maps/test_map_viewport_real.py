"""Real PostGIS viewport correctness, native preservation and bounded failures."""

from uuid import uuid4

import pytest
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_features import _setup
from tests.integration.spatial.test_sources import _client

BBOX = {"west": 106.5999, "south": 10.6999, "east": 106.6019, "north": 10.7009}


def root(world, layer):
    return (
        f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}/layers/{layer}"
    )


@pytest.mark.parametrize(
    "key,prefix,family",
    [
        ("reference_points_4326", "point", "Point"),
        ("reference_lines_4326", "line", "LineString"),
        ("reference_polygons_4326", "polygon", "Polygon"),
    ],
)
def test_exact_geographic_viewport_prefix_and_full_actual_extent(
    migrated_database, key, prefix, family
):
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        _, source, layer, pins = _setup(client, world, key)
        path = root(world, layer)
        result = client.get(path + "/viewport", params={**pins, **BBOX})
        assert result.status_code == 200, result.text
        wire = result.json()
        assert wire["complete"] is True and wire["limitation"] is None
        assert [item["feature_id"] for item in wire["items"]] == [
            f"{prefix}-000001",
            f"{prefix}-000002",
        ]
        assert all(set(item) == {"feature_id", "geometry"} for item in wire["items"])
        assert all(item["geometry"]["type"] == family for item in wire["items"])
        assert all(
            item["geometry"]["coordinate_reference"]["code"] == 4326 for item in wire["items"]
        )
        limited = client.get(path + "/viewport", params={**pins, **BBOX, "limit": 1})
        assert limited.status_code == 200
        assert limited.json()["complete"] is False
        assert limited.json()["limitation"] == "feature_limit"
        assert limited.json()["items"] == wire["items"][:1]
        empty = client.get(
            path + "/viewport",
            params={**pins, "west": 0, "south": 0, "east": 1, "north": 1},
        )
        assert empty.status_code == 200 and empty.json()["items"] == []
        assert empty.json()["complete"] is True
        extent = client.get(path + "/display-extent", params=pins)
        assert extent.status_code == 200, extent.text
        assert extent.json()["state"] == "available"
        expected = {
            "Point": [106.6, 10.7, 106.699, 10.739],
            "LineString": [106.6, 10.7, 106.6996, 10.7292],
            "Polygon": [106.6, 10.7, 106.6996, 10.7294],
        }[family]
        assert extent.json()["bounds"] == pytest.approx(expected, abs=1e-10)
        # Ordinary current source still has exactly the original content/schema pins.
        source_path = (
            f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}/"
            f"spatial/sources/{source['id']}"
        )
        after = client.get(source_path).json()
        assert all(
            after[name] == source[name]
            for name in (
                "content_revision",
                "schema_revision",
                "feature_count",
                "extent",
                "catalog_revision",
            )
        )


def test_direct_5899_control_operation_is_geographic_and_never_native_numbers(migrated_database):
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        _, _, layer, pins = _setup(client, world, "reference_points_5899")
        response = client.get(
            root(world, layer) + "/viewport",
            params={
                **pins,
                "west": 107.15,
                "south": 20.70,
                "east": 107.18,
                "north": 20.73,
            },
        )
        assert response.status_code == 200, response.text
        wire = response.json()
        assert wire["complete"] and len(wire["items"]) == 4
        assert wire["context"]["transformation"]["source_code"] == 5899
        assert wire["context"]["transformation"]["target_code"] == 4326
        # Independently published control pair HVS-QN1, ~1m accepted budget.
        first = wire["items"][0]
        assert first["feature_id"] == "HVS-QN1"
        assert first["geometry"]["coordinates"] == pytest.approx(
            [107.160886111111, 20.710688888889],
            abs=0.00002,
        )
        assert wire["context"]["response_crs"]["code"] == 4326


def test_display_domain_stale_and_foreign_layer_do_not_fabricate_empty(migrated_database):
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        _, _, layer, pins = _setup(client, world, "reference_points_4326_webmerc_out_of_area")
        path = root(world, layer)
        for suffix, extra in (("/viewport", BBOX), ("/display-extent", {})):
            response = client.get(path + suffix, params={**pins, **extra})
            assert response.status_code == 422, response.text
            assert response.json()["code"] == "TRANSFORMATION_OUT_OF_AREA"
            assert "items" not in response.json()
        metadata = client.get(path).json()
        flags = {cap["name"]: cap for cap in metadata["capabilities"]}
        assert not flags["render"]["technical_supported"]
        assert not flags["style"]["available"]
        stale = client.get(
            path + "/viewport",
            params={
                **pins,
                **BBOX,
                "content_revision": "0" * 64,
            },
        )
        assert stale.status_code == 409
        assert stale.json()["code"] == "STALE_READ_PIN"
        wrong = root(world, str(uuid4()))
        missing = client.get(wrong + "/viewport", params={**pins, **BBOX})
        assert missing.status_code == 404


def test_sql_failure_rolls_back_and_returns_safe_unavailable(migrated_database, monkeypatch):
    from sqlalchemy.exc import DBAPIError
    from sqlalchemy.orm import Session

    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        _, _, layer, pins = _setup(client, world, "reference_points_4326")
        original = Session.execute

        def timeout(session, statement, *args, **kwargs):
            if "candidate AS MATERIALIZED" in str(statement):
                raise DBAPIError("SELECT private_source", {}, RuntimeError("private SQL"), False)
            return original(session, statement, *args, **kwargs)

        with monkeypatch.context() as scoped:
            scoped.setattr(Session, "execute", timeout)
            failed = client.get(root(world, layer) + "/viewport", params={**pins, **BBOX})
        assert failed.status_code == 503, failed.text
        assert failed.json()["code"] == "MAP_READ_UNAVAILABLE"
        assert "private" not in failed.text and "items" not in failed.text
        # Reused runtime pool remains usable after rollback.
        assert client.get(root(world, layer) + "/display-extent", params=pins).status_code == 200


def test_runtime_has_no_unscoped_geometry_access(migrated_database):
    with get_engine().begin() as connection:
        assert (
            connection.execute(text("SELECT count(*) FROM spatial.reference_features")).scalar_one()
            == 0
        )


def test_single_scan_rejects_invalid_offscreen_geometry_and_count_mismatch(migrated_database):
    """A materialized viewport must still prove the full pinned source, not its hits."""
    world = seed_access_world()
    box = {"west": 107.15, "south": 20.70, "east": 107.18, "north": 20.73}
    with _client(world.owner_user_id) as client:
        _, source, layer, pins = _setup(client, world, "reference_points_4326_hvs")
        path = root(world, layer)
        baseline = client.get(path + "/viewport", params={**pins, **box})
        assert baseline.status_code == 200 and len(baseline.json()["items"]) == 4
        scope = {"org": world.organization_id, "proj": world.project_id, "src": source["id"]}
        with get_admin_engine().begin() as connection:
            # Test-only corruption outside the viewport, retaining the same pins.
            # The other three source geometries remain intersecting and valid.
            geometry = connection.execute(
                text("""
                SELECT ST_AsEWKB(geometry) FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:proj AND source_id=:src
                  AND feature_id='HVS-QN4'
                """),
                scope,
            ).scalar_one()
            changed = connection.execute(
                text("""
                UPDATE spatial.reference_features SET geometry=ST_SetSRID(ST_MakePoint(0,86),4326)
                WHERE organization_id=:org AND project_id=:proj AND source_id=:src
                  AND feature_id='HVS-QN4'
                """),
                scope,
            )
            assert changed.rowcount == 1
        try:
            for suffix, params in (("/viewport", {**box, **pins}), ("/display-extent", pins)):
                invalid = client.get(path + suffix, params=params)
                assert invalid.status_code == 422, invalid.text
                assert invalid.json()["code"] == "TRANSFORMATION_OUT_OF_AREA"
                assert "items" not in invalid.json() and "bounds" not in invalid.json()
        finally:
            with get_admin_engine().begin() as connection:
                connection.execute(
                    text("""
                    UPDATE spatial.reference_features SET geometry=ST_GeomFromEWKB(:geometry)
                    WHERE organization_id=:org AND project_id=:proj AND source_id=:src
                      AND feature_id='HVS-QN4'
                    """),
                    scope | {"geometry": geometry},
                )
        with get_admin_engine().begin() as connection:
            connection.execute(
                text("""
                UPDATE spatial.data_sources SET feature_count=feature_count+1
                WHERE organization_id=:org AND project_id=:proj AND id=:src
                """),
                scope,
            )
        try:
            for suffix, params in (("/viewport", {**box, **pins}), ("/display-extent", pins)):
                mismatch = client.get(path + suffix, params=params)
                assert mismatch.status_code == 503, mismatch.text
                assert mismatch.json()["code"] == "MAP_READ_UNAVAILABLE"
                assert "items" not in mismatch.json() and "bounds" not in mismatch.json()
        finally:
            with get_admin_engine().begin() as connection:
                connection.execute(
                    text("""
                    UPDATE spatial.data_sources SET feature_count=feature_count-1
                    WHERE organization_id=:org AND project_id=:proj AND id=:src
                    """),
                    scope,
                )
        recovered = client.get(path + "/viewport", params={**pins, **box})
        assert recovered.status_code == 200, recovered.text
        assert recovered.json() == baseline.json()


def test_empty_source_summary_remains_complete_without_phantom_feature(migrated_database):
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        _, _, layer, pins = _setup(client, world, "reference_empty_4326")
        path = root(world, layer)
        response = client.get(path + "/viewport", params={**pins, **BBOX})
        assert response.status_code == 200, response.text
        assert response.json()["items"] == [] and response.json()["complete"] is True
        extent = client.get(path + "/display-extent", params=pins)
        assert extent.status_code == 200, extent.text
        assert extent.json()["state"] == "empty" and extent.json()["bounds"] is None


def test_source_above_former_native_row_cap_renders(migrated_database, monkeypatch):
    """Regression: a hardcoded 10001-row native scan made every larger source 503."""
    from types import MappingProxyType

    from openvietgis_api.adapters.persistence import spatial_fixtures
    from openvietgis_api.domains.spatial.domain import GeometryFamily

    key = "reference_points_4326"
    larger = spatial_fixtures._primary(
        key, GeometryFamily.POINT, "point", "Point", 10002, (106.6, 10.7, 106.699, 10.8)
    )
    monkeypatch.setattr(
        spatial_fixtures,
        "REFERENCE_FIXTURES",
        MappingProxyType({**spatial_fixtures.REFERENCE_FIXTURES, key: larger}),
    )
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        _, source, layer, pins = _setup(client, world, key)
        assert source["feature_count"] == 10002
        path = root(world, layer)
        viewport = client.get(path + "/viewport", params={**pins, **BBOX})
        assert viewport.status_code == 200, viewport.text
        assert viewport.json()["complete"] is True
        assert [item["feature_id"] for item in viewport.json()["items"]] == [
            "point-000001",
            "point-000002",
        ]
        extent = client.get(path + "/display-extent", params=pins)
        assert extent.status_code == 200, extent.text
        assert extent.json()["bounds"] == pytest.approx([106.6, 10.7, 106.699, 10.8], abs=1e-10)
