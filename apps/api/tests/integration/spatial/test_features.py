"""US3 bounded native feature acceptance via actual restricted HTTP.

The oracle is the documented synthetic reference grid and frozen controls,
never the fixture registry, repository, database serialization or API output.
Owner connections are used only for synthetic setup.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from decimal import Decimal
from uuid import uuid4

import pytest
from fastapi.testclient import TestClient
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import AccessWorld, seed_access_world
from tests.integration.spatial.test_layers import _create
from tests.integration.spatial.test_layers import _path as _layer_path
from tests.integration.spatial.test_sources import _client, _error, _get, _register
from tests.integration.spatial.test_sources import _path as _source_path

PRIMARY = (
    ("reference_points_4326", "point", "Point", 4000),
    ("reference_lines_4326", "line", "LineString", 3000),
    ("reference_polygons_4326", "polygon", "Polygon", 3000),
)


def _coordinate_reference(code: int) -> dict:
    geographic = code in (4326, 4756)
    return {
        "authority": "EPSG",
        "code": code,
        "coordinate_order": "xy",
        "x_axis": "longitude" if geographic else "easting",
        "y_axis": "latitude" if geographic else "northing",
        "unit": "degree" if geographic else "metre",
    }


def _setup(client: TestClient, world: AccessWorld, key: str):
    source_id = _register(client, _source_path(world), key, key)["resource_id"]
    source = _get(client, _source_path(world), source_id)
    layer_id = _create(client, world, source_id, key)["resource_id"]
    pins = {key: source[key] for key in ("content_revision", "schema_revision")}
    return f"{_layer_path(world)}/{layer_id}/features", source, layer_id, pins


def _context(world, source, layer_id, pins, code: int | None) -> dict:
    return {
        "organization_id": str(world.organization_id),
        "project_id": str(world.project_id),
        "layer_id": layer_id,
        "source_id": source["id"],
        **pins,
        "geometry_included": code is not None,
        "response_crs": _coordinate_reference(code) if code is not None else None,
        "transformation": None,
    }


def _attributes(sequence: int, feature_id: str, name: str) -> dict:
    # Independent Decimal arithmetic, not the fixture's cents formatter.
    decimal = Decimal(sequence - 1) * Decimal("0.37")
    if sequence % 2 == 0:
        decimal = -decimal
    canonical_decimal = format(decimal.normalize(), "f") if decimal else "0"
    timestamp = datetime(2025, 1, 1, tzinfo=UTC) + timedelta(minutes=sequence - 1)
    return {
        "fld_feature_code": feature_id,
        "fld_name": name,
        "fld_rank": sequence % 1000,
        "fld_measure": None if sequence % 7 == 0 else canonical_decimal,
        "fld_active": sequence % 2 == 0,
        "fld_observed_at": (
            None if sequence % 11 == 0 else timestamp.strftime("%Y-%m-%dT%H:%M:%SZ")
        ),
        "fld_note": (
            None if sequence % 5 == 0 else "" if sequence % 5 == 1 else f"note-{sequence:06d}"
        ),
    }


def _coordinates(family: str, sequence: int) -> list:
    # Grid spaced at 0.001 degree; independent Decimal-based arithmetic to
    # avoid floating-point accumulation or using the fixture's geometry helper.
    row, column = divmod(sequence - 1, 100)
    x = Decimal("106.6") + Decimal("0.001") * column
    y = Decimal("10.7") + Decimal("0.001") * row

    def position(dx: str = "0", dy: str = "0") -> list[float]:
        return [float(x + Decimal(dx)), float(y + Decimal(dy))]

    if family == "Point":
        return position()
    if family == "LineString":
        return [position(), position("0.0003", "0.0002"), position("0.0006", "0.0001")]
    assert family == "Polygon"
    return [
        [
            position(),
            position("0.0006"),
            position("0.0006", "0.0004"),
            position("0", "0.0004"),
            position(),
        ]
    ]


def _assert_coordinates(actual: list, expected: list) -> None:
    assert isinstance(actual, list)
    assert len(actual) == len(expected)
    if isinstance(expected[0], list):
        for actual_part, expected_part in zip(actual, expected, strict=True):
            _assert_coordinates(actual_part, expected_part)
    else:
        assert all(type(value) in (int, float) for value in actual)
        assert actual == pytest.approx(expected, rel=0, abs=1e-10)


def _assert_feature(item, feature_id, attributes, family, coordinates, code=4326):
    assert set(item) == {"feature_id", "attributes", "geometry_state", "geometry"}
    assert item["feature_id"] == feature_id
    assert item["attributes"] == attributes
    # Python bool compares equal to int, so equality alone cannot prove wire types.
    assert {key: type(value) for key, value in item["attributes"].items()} == {
        key: type(value) for key, value in attributes.items()
    }
    assert item["geometry_state"] == "present"
    geometry = item["geometry"]
    assert set(geometry) == {"type", "coordinates", "coordinate_reference"}
    assert geometry["type"] == family
    assert geometry["coordinate_reference"] == _coordinate_reference(code)
    _assert_coordinates(geometry["coordinates"], coordinates)


def _read(client, path, params) -> dict:
    response = client.get(path, params=params)
    assert response.status_code == 200, response.text
    return response.json()


@pytest.mark.parametrize(("key", "prefix", "family", "count"), PRIMARY)
def test_all_primary_features_once_in_native_order(
    migrated_database: None, key: str, prefix: str, family: str, count: int
) -> None:
    world = seed_access_world()
    with get_engine().connect() as connection:
        assert tuple(
            connection.execute(
                text("""
            SELECT current_user,rolsuper,rolbypassrls
            FROM pg_roles WHERE rolname=current_user
        """)
            ).one()
        ) == ("openvietgis_runtime", False, False)
    with _client(world.owner_user_id) as client:
        path, source, layer_id, pins = _setup(client, world, key)
        context = _context(world, source, layer_id, pins, 4326)
        default_page = _read(client, path, pins)
        assert default_page["context"] == context
        assert [item["feature_id"] for item in default_page["items"]] == [
            f"{prefix}-{n:06d}" for n in range(1, 101)
        ]
        assert default_page["next_after_feature_id"] == f"{prefix}-000100"
        cursor = None
        seen = []
        # Non-divisor page size exercises short terminal pages. First call
        # omits the default geometry flag; next pages explicitly include it.
        for start in range(1, count + 1, 97):
            params = {**pins, "limit": 97}
            if cursor is not None:
                params.update(after_feature_id=cursor, include_geometry=True)
            page = _read(client, path, params)
            assert set(page) == {"context", "items", "next_after_feature_id"}
            assert page["context"] == context
            items = page["items"]
            stop = min(start + 97, count + 1)
            assert len(items) == stop - start
            for sequence, item in zip(range(start, stop), items, strict=True):
                feature_id = f"{prefix}-{sequence:06d}"
                name = f"{prefix.title()} {sequence:06d}"
                _assert_feature(
                    item,
                    feature_id,
                    _attributes(sequence, feature_id, name),
                    family,
                    _coordinates(family, sequence),
                )
                seen.append(item["feature_id"])
                if sequence in {1, 7, 11, 97, 98, count}:
                    assert _read(client, f"{path}/{feature_id}", pins) == {
                        "context": context,
                        "feature": item,
                    }
            cursor = page["next_after_feature_id"]
            assert cursor == (items[-1]["feature_id"] if stop <= count else None)

        assert seen == [f"{prefix}-{n:06d}" for n in range(1, count + 1)]
        assert len(set(seen)) == count
        assert _read(client, path, {**pins, "after_feature_id": seen[-1]}) == {
            "context": context,
            "items": [],
            "next_after_feature_id": None,
        }


def test_native_read_flags_cursor_and_scoped_detail(migrated_database: None) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        path, source, layer_id, pins = _setup(client, world, "reference_points_5899")
        context = _context(world, source, layer_id, pins, 5899)
        native = _read(client, path, {**pins, "limit": 3})
        assert native["context"] == context
        expected_ids = ["HVS-QN1", "HVS-QN2", "HVS-QN3", "HVS-QN4"]
        positions = [
            [438442.69, 2291039.83],
            [437957.55, 2291878.33],
            [438891.96, 2292185.83],
            [439365.92, 2291378.72],
        ]
        assert native["next_after_feature_id"] == "HVS-QN3"
        last = _read(client, path, {**pins, "after_feature_id": "HVS-QN3", "limit": 1})
        assert last["next_after_feature_id"] is None
        assert last["context"] == context
        for sequence, (item, feature_id, xy) in enumerate(
            zip(native["items"] + last["items"], expected_ids, positions, strict=True), 1
        ):
            _assert_feature(
                item,
                feature_id,
                _attributes(sequence, feature_id, f"Reference {feature_id}"),
                "Point",
                xy,
                5899,
            )
            assert _read(client, f"{path}/{feature_id}", {**pins, "response_crs": 5899}) == {
                "context": context,
                "feature": item,
            }

        native_all = _read(client, path, {**pins, "response_crs": 5899})
        assert native_all == {
            "context": context,
            "items": native["items"] + last["items"],
            "next_after_feature_id": None,
        }
        no_geometry = _read(client, path, {**pins, "include_geometry": False})
        omitted_context = _context(world, source, layer_id, pins, None)
        assert no_geometry == {
            "context": omitted_context,
            "items": [
                {key: value for key, value in item.items() if key != "geometry"}
                for item in native_all["items"]
            ],
            "next_after_feature_id": None,
        }
        assert _read(client, f"{path}/HVS-QN1", {**pins, "include_geometry": False}) == {
            "context": omitted_context,
            "feature": no_geometry["items"][0],
        }

        foreign_path, _, _, _ = _setup(client, world, "reference_points_3857")
        # This cursor exists in a different source within the same Project.
        assert foreign_path != path
        for cursor in ("HVS-QN0", "merc-000001"):
            _error(
                client.get(path, params={**pins, "after_feature_id": cursor}),
                422,
                "INVALID_CURSOR",
            )
        for suffix in ("", "/HVS-QN1"):
            assert (
                client.get(
                    path + suffix, params={**pins, "include_geometry": False, "response_crs": 5899}
                ).status_code
                == 422
            )
        for invalid_limit in (0, 1001):
            assert client.get(path, params={**pins, "limit": invalid_limit}).status_code == 422

        missing = client.get(f"{path}/not-a-real-id", params=pins)
        assert missing.status_code == 404
        foreign_feature = client.get(f"{path}/merc-000001", params=pins)
        assert foreign_feature.status_code == 404
        assert foreign_feature.json() == missing.json()
        missing_layer = client.get(f"{_layer_path(world)}/{uuid4()}/features", params=pins)
        assert missing_layer.status_code == 404


def test_empty_and_unavailable_native_reads(migrated_database: None) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        path, source, layer_id, pins = _setup(client, world, "reference_empty_4326")
        for include_geometry in (True, False):
            assert _read(client, path, {**pins, "include_geometry": include_geometry}) == {
                "context": _context(
                    world, source, layer_id, pins, 4326 if include_geometry else None
                ),
                "items": [],
                "next_after_feature_id": None,
            }
        _error(
            client.get(path, params={**pins, "after_feature_id": "point-000001"}),
            422,
            "INVALID_CURSOR",
        )
        assert client.get(f"{path}/point-000001", params=pins).status_code == 404

        path, source, _, pins = _setup(client, world, "reference_points_3857")
        # Synthetic setup models a ready Layer whose Source becomes unavailable.
        # There is intentionally no public mutation that can attach an invalid source.
        # Keep current pins to isolate readiness (not stale-pin rejection).
        with get_admin_engine().begin() as connection:
            result = connection.execute(
                text("""
                    UPDATE spatial.data_sources
                    SET validation_state='not_ready',
                        validation_reason=jsonb_build_object(
                            'code','FIXTURE_UNAVAILABLE','message','Synthetic unavailable.'),
                        extent_state='unavailable', extent=NULL
                    WHERE id=:id AND organization_id=:org AND project_id=:project
                """),
                {"id": source["id"], "org": world.organization_id, "project": world.project_id},
            )
            assert result.rowcount == 1
        for suffix in ("", "/merc-000001"):
            for include_geometry in (True, False):
                _error(
                    client.get(
                        path + suffix, params={**pins, "include_geometry": include_geometry}
                    ),
                    409,
                    "SOURCE_NOT_READY",
                )


def test_transform_expanded_page_preserves_order_and_coordinates(migrated_database: None) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        path, _, _, pins = _setup(client, world, "reference_points_4326")
        page = _read(client, path, {**pins, "response_crs": 3857, "limit": 1000})
        assert len(page["items"]) == 1000
        assert [item["feature_id"] for item in page["items"]] == [
            f"point-{n:06d}" for n in range(1, 1001)
        ]
        assert page["next_after_feature_id"] == "point-001000"
        for index in (0, 100, 101, 999):
            item = page["items"][index]
            single = _read(client, path + "/" + item["feature_id"], {**pins, "response_crs": 3857})
            assert single["feature"] == item
