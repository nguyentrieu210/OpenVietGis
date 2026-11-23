"""Independent manifest expectations; no datastore or transform dependency."""

from dataclasses import FrozenInstanceError
from datetime import UTC, datetime
from uuid import UUID

import pytest

from openvietgis_api.adapters.persistence import spatial_fixtures as fixtures
from openvietgis_api.domains.spatial.domain import (
    ExtentState,
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    GeometryState,
    SpatialValidationError,
    ValidationReasonCode,
    ValidationState,
    validate_ready_geometry,
)
from openvietgis_api.domains.spatial.service import verify_source_pins

ORG = UUID(int=1)
PROJECT = UUID(int=2)
SOURCE = UUID(int=3)
NOW = datetime(2026, 11, 18, tzinfo=UTC)


def materialize(key: str):
    return fixtures.materialize_reference_fixture(
        key,
        organization_id=ORG,
        project_id=PROJECT,
        source_id=SOURCE,
        name="Reference",
        now=NOW,
    )


def fixture(key: str) -> fixtures.ReferenceFixture:
    result = fixtures.get_reference_fixture(key)
    assert result is not None
    return result


def test_immutable_allowlist_rejects_unknown_and_arbitrary_binding() -> None:
    for key in ("unknown", "../private", "postgresql://host/db", "SELECT 1", ""):
        assert fixtures.get_reference_fixture(key) is None
        with pytest.raises(SpatialValidationError, match="unknown reference fixture"):
            materialize(key)
    assert set(fixtures.REFERENCE_FIXTURES) == {
        "reference_points_4326",
        "reference_lines_4326",
        "reference_polygons_4326",
        "reference_points_3857",
        "reference_points_4756",
        "reference_points_4326_quang_ninh",
        "reference_points_4756_hvs",
        "reference_points_4326_hvs",
        "reference_points_5899",
        "reference_points_4326_webmerc_out_of_area",
        "reference_empty_4326",
        "reference_unknown_crs",
        "reference_conflicting_metadata",
        "reference_unsupported_dimensions",
        "reference_unsupported_dimensions_m",
        "reference_mixed_geometry",
        "reference_unsupported_family",
        "reference_null_geometry",
        "reference_empty_geometry",
        "reference_invalid_geometry",
        "reference_axis_conflict",
        "reference_out_of_domain",
        "reference_definition_only_crs",
        "reference_wrapping_extent",
        "reference_correctable_crs",
    }
    with pytest.raises(FrozenInstanceError):
        fixture("reference_points_4326").expected_count = 0
    with pytest.raises(TypeError):
        fixtures.REFERENCE_FIXTURES["other"] = fixture("reference_points_4326")


@pytest.mark.parametrize(
    ("key", "count", "family", "first_id", "last_id", "bounds", "first_coordinates"),
    [
        (
            "reference_points_4326",
            4000,
            GeometryFamily.POINT,
            "point-000001",
            "point-004000",
            (106.6, 10.7, 106.699, 10.739),
            (106.6, 10.7),
        ),
        (
            "reference_lines_4326",
            3000,
            GeometryFamily.LINE_STRING,
            "line-000001",
            "line-003000",
            (106.6, 10.7, 106.6996, 10.7292),
            ((106.6, 10.7), (106.6003, 10.7002), (106.6006, 10.7001)),
        ),
        (
            "reference_polygons_4326",
            3000,
            GeometryFamily.POLYGON,
            "polygon-000001",
            "polygon-003000",
            (106.6, 10.7, 106.6996, 10.7294),
            (
                (
                    (106.6, 10.7),
                    (106.6006, 10.7),
                    (106.6006, 10.7004),
                    (106.6, 10.7004),
                    (106.6, 10.7),
                ),
            ),
        ),
    ],
)
def test_primary_fixtures_have_independent_counts_native_bounds_and_stable_geometry(
    key, count, family, first_id, last_id, bounds, first_coordinates
) -> None:
    manifest = fixture(key)
    result = materialize(key)
    assert manifest.expected_count == count
    assert manifest.expected_native_bounds == bounds
    assert result.source.feature_count == count
    assert result.source.geometry_family == family
    assert result.source.dimensions == GeometryDimensions.XY
    assert result.source.source_crs == result.source.stored_crs == 4326
    assert [row.feature_id for row in result.features] == sorted(
        {row.feature_id for row in result.features}
    )
    assert result.features[0].feature_id == first_id
    assert result.features[-1].feature_id == last_id
    assert result.features[0].geometry.coordinates == first_coordinates
    positions = []
    for row in result.features:
        assert row.geometry is not None
        positions.extend(validate_ready_geometry(row.geometry, family, 4326))
        assert row.geometry.srid == 4326
        assert row.geometry.type == family.value
        assert row.geometry.dimensions == GeometryDimensions.XY
        assert row.geometry_state == GeometryState.PRESENT
    xs, ys = zip(*positions, strict=True)
    assert (min(xs), min(ys), max(xs), max(ys)) == bounds
    # Materializing candidate rows does not imply actual PostGIS registration readiness.
    assert result.source.validation_state == ValidationState.NOT_READY
    assert result.source.extent_state == ExtentState.UNAVAILABLE
    assert result.source.extent is None
    verify_source_pins(result.source, result.fields, result.features)


def test_typed_attributes_are_stable_and_distinguish_zero_null_and_empty() -> None:
    result = materialize("reference_points_4326")
    field_definitions = [
        (field.field_id, field.type, field.nullable, field.ordinal) for field in result.fields
    ]
    assert field_definitions == [
        ("fld_feature_code", FieldType.TEXT, False, 0),
        ("fld_name", FieldType.TEXT, False, 1),
        ("fld_rank", FieldType.INTEGER, False, 2),
        ("fld_measure", FieldType.DECIMAL, True, 3),
        ("fld_active", FieldType.BOOLEAN, False, 4),
        ("fld_observed_at", FieldType.TIMESTAMP, True, 5),
        ("fld_note", FieldType.TEXT, True, 6),
    ]
    one, two = result.features[:2]
    assert dict(one.attributes) == {
        "fld_feature_code": "point-000001",
        "fld_name": "Point 000001",
        "fld_rank": 1,
        "fld_measure": "0",
        "fld_active": False,
        "fld_observed_at": "2025-01-01T00:00:00Z",
        "fld_note": "",
    }
    assert two.attributes["fld_measure"] == "-0.37"
    assert two.attributes["fld_active"] is True
    assert result.features[4].attributes["fld_note"] is None
    assert result.features[6].attributes["fld_measure"] is None
    assert result.features[10].attributes["fld_observed_at"] is None
    assert result.features[999].attributes["fld_rank"] == 0
    with pytest.raises(TypeError):
        one.attributes["fld_measure"] = "999"


def test_materialization_uses_same_content_pins_but_distinct_scoped_owners() -> None:
    one = materialize("reference_points_5899")
    two = fixtures.materialize_reference_fixture(
        "reference_points_5899",
        organization_id=UUID(int=11),
        project_id=UUID(int=12),
        source_id=UUID(int=13),
        name="Other name",
        now=NOW,
    )
    assert one.source.id != two.source.id
    assert one.source.manifest_digest == two.source.manifest_digest
    assert one.source.content_revision == two.source.content_revision
    assert one.source.schema_revision == two.source.schema_revision
    for row in (*two.fields, *two.features):
        assert row.organization_id == two.source.organization_id
        assert row.project_id == two.source.project_id
        assert row.source_id == two.source.id
        assert row.schema_revision == two.source.schema_revision
    for row in two.features:
        assert row.content_revision == two.source.content_revision
    assert one.features[0].geometry == two.features[0].geometry
    other_manifest = materialize("reference_points_4326_hvs").source.manifest_digest
    assert one.source.manifest_digest != other_manifest
    verify_source_pins(two.source, two.fields, two.features)


def test_published_hvs_native_coordinates_are_separate_narrow_control_fixtures() -> None:
    expected = {
        "reference_points_5899": [
            (438442.69, 2291039.83),
            (437957.55, 2291878.33),
            (438891.96, 2292185.83),
            (439365.92, 2291378.72),
        ],
        "reference_points_4756_hvs": [
            (107.159011111111, 20.711691666667),
            (107.154322222222, 20.71925),
            (107.163283333333, 20.722058333333),
            (107.167861111111, 20.714783333333),
        ],
        "reference_points_4326_hvs": [
            (107.160886111111, 20.710688888889),
            (107.156197222222, 20.718247222222),
            (107.165158333333, 20.721055555556),
            (107.169736111111, 20.713780555556),
        ],
    }
    for key, coords in expected.items():
        manifest = fixture(key)
        result = materialize(key)
        assert manifest.provenance == fixtures.FixtureProvenance.QUANG_NINH_HVS
        assert manifest.expected_count == 4
        assert [row.feature_id for row in result.features] == [f"HVS-QN{i}" for i in range(1, 5)]
        for row, expected_xy in zip(result.features, coords, strict=True):
            assert row.geometry.coordinates == pytest.approx(expected_xy, rel=0, abs=1e-11)


def test_wide_geographic_controls_are_not_mixed_into_narrow_fixtures() -> None:
    vn = materialize("reference_points_4756")
    wgs = materialize("reference_points_4326_quang_ninh")
    expected = {
        "CT1": ((107.761333333333, 20.970638888889), (107.763222222222, 20.969638888889)),
        "QY1": ((106.883111111111, 20.8355), (106.885, 20.8345)),
        "VH1": ((108.019444444444, 21.411111111111), (108.021333333333, 21.410111111111)),
        "VH10": ((107.946111111111, 21.23625), (107.948, 21.23525)),
        "VH2": ((108.059805555556, 21.414472222222), (108.061694444444, 21.413472222222)),
    }
    assert fixture("reference_points_4756").provenance == (
        fixtures.FixtureProvenance.QUANG_NINH_CIRCULAR
    )
    assert fixture("reference_points_4326_quang_ninh").provenance == (
        fixtures.FixtureProvenance.QUANG_NINH_CIRCULAR
    )
    assert {row.feature_id for row in vn.features} == set(expected)
    assert {row.feature_id for row in wgs.features} == set(expected)
    for row in vn.features:
        assert row.geometry.coordinates == pytest.approx(
            expected[row.feature_id][0], rel=0, abs=1e-11
        )
    for row in wgs.features:
        assert row.geometry.coordinates == pytest.approx(
            expected[row.feature_id][1], rel=0, abs=1e-11
        )


def test_independent_web_mercator_native_fixture_and_out_of_area_controls() -> None:
    result = materialize("reference_points_3857")
    expected = [
        (11866657.718562961, 1198103.040560261),
        (11877745.139845973, 1201433.906852360),
        (11928662.674934816, 2357927.646019978),
    ]
    for row, xy in zip(result.features, expected, strict=True):
        assert row.geometry.srid == 3857
        assert row.geometry.coordinates == pytest.approx(xy, rel=0, abs=1e-7)
    assert fixture("reference_points_4326_webmerc_out_of_area").expected_reason is None
    outside = materialize("reference_points_4326_webmerc_out_of_area").features[0]
    assert outside.geometry.coordinates == (0.0, 86.0)


@pytest.mark.parametrize(
    ("key", "reason", "family", "dimensions", "states", "srids"),
    [
        (
            "reference_unknown_crs",
            ValidationReasonCode.MISSING_CRS,
            GeometryFamily.POINT,
            GeometryDimensions.XY,
            [GeometryState.PRESENT],
            [None],
        ),
        (
            "reference_conflicting_metadata",
            ValidationReasonCode.GEOMETRY_FAMILY_MISMATCH,
            GeometryFamily.LINE_STRING,
            GeometryDimensions.XY,
            [GeometryState.PRESENT],
            [4326],
        ),
        (
            "reference_unsupported_dimensions",
            ValidationReasonCode.UNSUPPORTED_DIMENSIONS,
            GeometryFamily.POINT,
            GeometryDimensions.XYZ,
            [GeometryState.UNSUPPORTED],
            [4326],
        ),
        (
            "reference_unsupported_dimensions_m",
            ValidationReasonCode.UNSUPPORTED_DIMENSIONS,
            GeometryFamily.POINT,
            GeometryDimensions.XYM,
            [GeometryState.UNSUPPORTED],
            [4326],
        ),
        (
            "reference_mixed_geometry",
            ValidationReasonCode.UNSUPPORTED_GEOMETRY,
            GeometryFamily.MIXED,
            GeometryDimensions.XY,
            [GeometryState.PRESENT, GeometryState.PRESENT],
            [4326, 4326],
        ),
        (
            "reference_unsupported_family",
            ValidationReasonCode.UNSUPPORTED_GEOMETRY,
            GeometryFamily.UNSUPPORTED,
            GeometryDimensions.XY,
            [GeometryState.UNSUPPORTED],
            [4326],
        ),
        (
            "reference_null_geometry",
            ValidationReasonCode.NULL_GEOMETRY,
            GeometryFamily.UNKNOWN,
            GeometryDimensions.UNKNOWN,
            [GeometryState.ABSENT],
            [None],
        ),
        (
            "reference_empty_geometry",
            ValidationReasonCode.EMPTY_GEOMETRY,
            GeometryFamily.POINT,
            GeometryDimensions.XY,
            [GeometryState.EMPTY],
            [4326],
        ),
        (
            "reference_invalid_geometry",
            ValidationReasonCode.INVALID_GEOMETRY,
            GeometryFamily.POLYGON,
            GeometryDimensions.XY,
            [GeometryState.INVALID],
            [4326],
        ),
        (
            "reference_axis_conflict",
            ValidationReasonCode.AXIS_CONFLICT,
            GeometryFamily.POINT,
            GeometryDimensions.XY,
            [GeometryState.INVALID],
            [4326],
        ),
        (
            "reference_out_of_domain",
            ValidationReasonCode.COORDINATE_OUT_OF_DOMAIN,
            GeometryFamily.POINT,
            GeometryDimensions.XY,
            [GeometryState.INVALID],
            [4326],
        ),
        (
            "reference_definition_only_crs",
            ValidationReasonCode.UNSUPPORTED_CRS,
            GeometryFamily.POINT,
            GeometryDimensions.XY,
            [GeometryState.PRESENT],
            [3405],
        ),
        (
            "reference_wrapping_extent",
            ValidationReasonCode.EXTENT_WRAP_UNSUPPORTED,
            GeometryFamily.LINE_STRING,
            GeometryDimensions.XY,
            [GeometryState.PRESENT],
            [4326],
        ),
    ],
)
def test_negative_fixtures_retain_raw_state_and_independent_expected_failure(
    key, reason, family, dimensions, states, srids
) -> None:
    manifest = fixture(key)
    result = materialize(key)
    assert manifest.expected_reason == reason
    assert result.source.geometry_family == family
    assert result.source.dimensions == dimensions
    assert [row.geometry_state for row in result.features] == states
    assert [row.geometry.srid if row.geometry else None for row in result.features] == srids
    assert result.source.validation_state == ValidationState.NOT_READY
    assert result.source.extent is None
    verify_source_pins(result.source, result.fields, result.features)


def test_empty_dataset_is_not_empty_geometry_and_correctable_crs_has_explicit_evidence() -> None:
    empty = materialize("reference_empty_4326")
    assert empty.source.feature_count == 0
    assert not empty.features and len(empty.fields) == 7
    assert fixture("reference_empty_4326").expected_reason is None
    correctable = fixture("reference_correctable_crs")
    result = materialize(correctable.key)
    assert correctable.expected_reason == ValidationReasonCode.CRS_MISMATCH
    assert correctable.correctable_to_crs == 4326
    assert correctable.axis_control_xy == pytest.approx(
        (107.160886111111, 20.710688888889), rel=0, abs=1e-11
    )
    assert result.source.source_crs == 3857
    assert result.source.stored_crs == result.features[0].geometry.srid == 4326
    assert result.features[0].geometry.coordinates == pytest.approx(
        correctable.axis_control_xy, rel=0, abs=1e-11
    )
    swapped = fixture("reference_axis_conflict")
    assert swapped.correctable_to_crs is None
    assert swapped.axis_control_xy == correctable.axis_control_xy
    assert materialize(swapped.key).features[0].geometry.coordinates == tuple(
        reversed(swapped.axis_control_xy)
    )
