from dataclasses import FrozenInstanceError, replace

import pytest

from openvietgis_api.domains.spatial import crs


def test_registry_is_finite_and_definition_only_is_not_readable() -> None:
    assert {profile.code for profile in crs.CRS_PROFILES} == {4326, 3857, 4756, 5899, 3405, 3406}
    assert len(crs.CRS_PROFILES) == 6
    for code in (4326, 3857, 4756, 5899):
        profile = crs.get_crs_profile(code)
        assert profile is not None
        assert crs.get_readable_crs_profile(code) is profile
        assert profile.disposition == "readable"
    for code in (3405, 3406):
        profile = crs.get_crs_profile(code)
        assert profile is not None
        assert profile.disposition == "definition_only"
        assert crs.get_readable_crs_profile(code) is None
        assert crs.get_coordinate_reference(code) is None
    for code in (0, 900913, 32648, 999999, -1):
        assert crs.get_crs_profile(code) is None
        assert crs.get_readable_crs_profile(code) is None


def test_native_axes_and_explicit_xy_coordinate_reference_are_distinct() -> None:
    for code in (4326, 4756):
        profile = crs.get_crs_profile(code)
        assert profile is not None
        assert profile.native_axes == ("latitude", "longitude")
        coordinate = crs.get_coordinate_reference(code)
        assert coordinate is not None
        assert coordinate.to_wire() == {
            "authority": "EPSG",
            "code": code,
            "coordinate_order": "xy",
            "x_axis": "longitude",
            "y_axis": "latitude",
            "unit": "degree",
        }
    for code in (3857, 5899):
        profile = crs.get_crs_profile(code)
        assert profile is not None
        assert profile.native_axes == ("easting", "northing")
        coordinate = crs.get_coordinate_reference(code)
        assert coordinate is not None
        assert coordinate.to_wire() == {
            "authority": "EPSG",
            "code": code,
            "coordinate_order": "xy",
            "x_axis": "easting",
            "y_axis": "northing",
            "unit": "metre",
        }


def test_web_mercator_definition_does_not_claim_precise_geodetic_datum_shift() -> None:
    geographic = crs.get_crs_profile(4326)
    projected = crs.get_crs_profile(3857)
    assert geographic is not None and projected is not None
    assert geographic.name == "WGS 84"
    assert not geographic.projection_parameters
    assert projected.name == "WGS 84 / Pseudo-Mercator"
    assert geographic.datum == projected.datum == "World Geodetic System 1984 ensemble"
    assert dict(projected.projection_parameters) == {
        "method": "Popular Visualisation Pseudo Mercator (EPSG:1024)",
        "latitude_of_origin": 0.0,
        "central_meridian": 0.0,
        "false_easting": 0.0,
        "false_northing": 0.0,
        "sphere_radius": 6378137.0,
    }


def test_vn2000_definition_metadata_distinguishes_projection_profiles() -> None:
    projected_parameters = {
        5899: (107.75, 0.9999, "VN-2000 / TM-3 107-45"),
        3405: (105.0, 0.9996, "VN-2000 / UTM zone 48N"),
        3406: (111.0, 0.9996, "VN-2000 / UTM zone 49N"),
    }
    for code, (central_meridian, scale, name) in projected_parameters.items():
        profile = crs.get_crs_profile(code)
        assert profile is not None
        assert profile.datum == "Vietnam 2000"
        assert profile.name == name
        assert profile.native_axes == ("easting", "northing")
        assert dict(profile.projection_parameters) == {
            "method": "Transverse Mercator (EPSG:9807)",
            "latitude_of_origin": 0.0,
            "central_meridian": central_meridian,
            "scale_factor": scale,
            "false_easting": 500000.0,
            "false_northing": 0.0,
        }
    profile = crs.get_crs_profile(5899)
    assert profile is not None
    assert "Quang Ninh" in profile.area_of_use
    assert crs.get_crs_profile(3405).area_of_use.endswith("west of 108°E.")
    assert crs.get_crs_profile(3406).area_of_use.endswith("east of 108°E.")


def test_profile_wire_uses_exact_contract_keys_and_does_not_mutate_registry() -> None:
    profile = crs.get_crs_profile(5899)
    assert profile is not None
    wire = profile.to_wire()
    assert set(wire) == {
        "authority",
        "code",
        "name",
        "datum",
        "native_axes",
        "unit",
        "coordinate_order",
        "x_axis",
        "y_axis",
        "projection_parameters",
        "area_of_use",
        "disposition",
    }
    assert wire["authority"] == "EPSG"
    assert wire["coordinate_order"] == "xy"
    assert wire["native_axes"] == ["easting", "northing"]
    wire["native_axes"].append("latitude")
    wire["projection_parameters"]["central_meridian"] = 0
    assert profile.native_axes == ("easting", "northing")
    assert dict(profile.projection_parameters)["central_meridian"] == 107.75
    assert replace(profile, name="other").name != profile.name
    with pytest.raises(FrozenInstanceError):
        profile.name = "changed"


def test_only_eight_directed_edges_have_frozen_operation_ids_and_tolerances() -> None:
    expected = {
        (4326, 3857): ("epsg-1024-forward", 0.001, "metre"),
        (3857, 4326): ("epsg-1024-inverse", 1e-8, "degree"),
        (4756, 4326): ("epsg-6960-forward", 3.0, "metre"),
        (4326, 4756): ("epsg-6960-inverse", 3.0, "metre"),
        (5899, 4756): ("epsg-5899-inverse-tm3", 1.0, "metre"),
        (4756, 5899): ("epsg-5899-forward-tm3", 1.0, "metre"),
        (5899, 4326): ("epsg-5899-to-4326-via-6960", 1.0, "metre"),
        (4326, 5899): ("epsg-4326-to-5899-via-6960", 1.0, "metre"),
    }
    assert len(crs.TRANSFORMATION_PROFILES) == 8
    assert {
        (edge.source_code, edge.target_code): (
            edge.operation_id,
            edge.tolerance,
            edge.tolerance_unit,
        )
        for edge in crs.TRANSFORMATION_PROFILES
    } == expected
    for source in (4326, 3857, 4756, 5899, 3405, 3406, 0, 9999):
        for target in (4326, 3857, 4756, 5899, 3405, 3406, 0, 9999):
            edge = crs.get_transformation_profile(source, target)
            if (source, target) not in expected:
                assert edge is None
            else:
                assert edge is not None
                assert (edge.operation_id, edge.tolerance, edge.tolerance_unit) == expected[
                    (source, target)
                ]
                assert edge.validated_area and edge.method and edge.evidence_id


def test_operation_steps_are_fixed_and_not_automatic_graph_chaining() -> None:
    web_pipe = (
        "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad "
        "+step +proj=webmerc +ellps=WGS84"
    )
    tm_pipe = (
        "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad "
        "+step +proj=tmerc +lat_0=0 +lon_0=107.75 +k=0.9999 "
        "+x_0=500000 +y_0=0 +ellps=WGS84"
    )
    datum_pipe = "urn:ogc:def:coordinateOperation:EPSG::6960"
    steps = {
        (4326, 3857): ((4326, 3857, web_pipe, False),),
        (3857, 4326): ((3857, 4326, web_pipe, True),),
        (4756, 4326): ((4756, 4326, datum_pipe, False),),
        (4326, 4756): ((4326, 4756, datum_pipe, True),),
        (5899, 4756): ((5899, 4756, tm_pipe, True),),
        (4756, 5899): ((4756, 5899, tm_pipe, False),),
        (5899, 4326): ((5899, 4756, tm_pipe, True), (4756, 4326, datum_pipe, False)),
        (4326, 5899): ((4326, 4756, datum_pipe, True), (4756, 5899, tm_pipe, False)),
    }
    for edge in crs.TRANSFORMATION_PROFILES:
        assert (
            tuple(
                (step.source_code, step.target_code, step.pipeline, step.inverse)
                for step in edge.steps
            )
            == steps[(edge.source_code, edge.target_code)]
        )
        assert set(edge.to_wire()) == {
            "source_code",
            "target_code",
            "operation_id",
            "method",
            "validated_area",
            "tolerance",
            "tolerance_unit",
            "evidence_id",
        }


def test_applicability_bounds_and_provenance_requirements_are_explicit() -> None:
    for edge in crs.TRANSFORMATION_PROFILES:
        codes = (edge.source_code, edge.target_code)
        if 5899 in codes:
            assert edge.geographic_bounds == crs.CoordinateBounds(107.15, 20.70, 107.18, 20.73)
        elif 4756 in codes:
            assert edge.geographic_bounds == crs.CoordinateBounds(106.88, 20.70, 108.07, 21.42)
        else:
            assert edge.geographic_bounds == crs.CoordinateBounds(
                -180.0, -85.0511287798066, 180.0, 85.0511287798066
            )
        assert edge.requires_trusted_fixture == (4756 in codes or 5899 in codes)
        box = edge.geographic_bounds
        assert box.contains(box.min_x, box.min_y)
        assert box.contains(box.max_x, box.max_y)
        assert not box.contains(box.min_x - 0.000001, box.min_y)
        assert not box.contains(float("nan"), box.min_y)
        assert not box.contains(box.max_x, float("inf"))


def test_native_square_bounds_and_approved_output_epsilon() -> None:
    profile = crs.get_crs_profile(3857)
    assert profile is not None
    edge = 20037508.342789244
    assert profile.native_bounds == crs.CoordinateBounds(-edge, -edge, edge, edge)
    assert crs.WEB_MERCATOR_OUTPUT_EPSILON_METRE == 1e-6
    assert profile.native_bounds.contains(edge, edge)
    assert not profile.native_bounds.contains(edge + 1e-7, edge)
    assert profile.native_bounds.contains(
        edge + 1e-7, edge, epsilon=crs.WEB_MERCATOR_OUTPUT_EPSILON_METRE
    )


def test_projected_native_range_does_not_claim_region_without_inverse_projection() -> None:
    profile = crs.get_crs_profile(5899)
    assert profile is not None
    assert profile.native_bounds is None
    assert crs.get_crs_profile(4326).native_bounds == crs.CoordinateBounds(
        -180.0, -90.0, 180.0, 90.0
    )
    assert crs.get_crs_profile(4756).native_bounds == crs.CoordinateBounds(
        -180.0, -90.0, 180.0, 90.0
    )
