"""R004 bounds and truthful bounded output are independent of the renderer."""

import json
from uuid import uuid4

import pytest

from openvietgis_api.adapters.persistence.map_viewport import SqlMapViewportRepository
from openvietgis_api.domains.maps.viewport import (
    MAX_RENDER_SOURCE_FEATURES,
    MapReadUnavailableError,
    RenderFeature,
    ViewportBounds,
    build_viewport_response,
)
from openvietgis_api.domains.spatial.crs import get_coordinate_reference
from openvietgis_api.domains.spatial.domain import (
    GeometryDimensions,
    NativeGeometry,
    SpatialValidationError,
)
from openvietgis_api.domains.spatial.ports import SpatialTransformationOutOfAreaError


def context():
    reference = get_coordinate_reference(4326)
    assert reference is not None
    return {
        "organization_id": uuid4(),
        "project_id": uuid4(),
        "source_id": uuid4(),
        "layer_id": uuid4(),
        "content_revision": "a" * 64,
        "schema_revision": "b" * 64,
        "geometry_included": True,
        "response_crs": reference.to_wire(),
        "transformation": None,
    }


def point(identifier, x=106.6):
    return RenderFeature(
        identifier, NativeGeometry("Point", (x, 10.7), 4326, GeometryDimensions.XY), 1
    )


@pytest.mark.parametrize(
    "bounds",
    [
        (0, 0, 0, 1),
        (1, 0, 0, 1),
        (0, 1, 1, 0),
        (-181, 0, 1, 1),
        (0, -86, 1, 1),
        (0, 0, 181, 1),
        (0, 0, 1, 86),
        (True, 0, 1, 1),
        (0, 0, float("inf"), 1),
        (0, 0, float("nan"), 1),
        ("0", 0, 1, 1),
    ],
)
def test_reject_nonfinite_wrapped_or_degenerate_viewport(bounds):
    with pytest.raises(SpatialValidationError):
        ViewportBounds(*bounds)


def test_viewport_limits_are_not_source_completeness():
    wire = build_viewport_response(context(), iter([point("a"), point("b")]), limit=1)
    assert [item["feature_id"] for item in wire["items"]] == ["a"]
    assert wire["complete"] is False
    assert wire["limitation"] == "feature_limit"
    assert "attributes" not in wire["items"][0]


def test_position_budget_keeps_prefix_and_never_skips_offending_feature():
    wire = build_viewport_response(
        context(),
        iter([point("a"), RenderFeature("b", None, 4096), point("c")]),
        limit=10,
        position_budget=2,
    )
    assert [item["feature_id"] for item in wire["items"]] == ["a"]
    assert wire["complete"] is False
    assert wire["limitation"] == "position_limit"


def test_entire_json_envelope_counts_towards_byte_budget():
    snapshot = context()
    full = build_viewport_response(snapshot, iter([point("a"), point("b")]), limit=10)
    budget = len(json.dumps(full, default=str, separators=(",", ":"), ensure_ascii=False).encode())
    limited = build_viewport_response(
        snapshot,
        iter([point("a"), point("b")]),
        limit=10,
        byte_budget=budget - 1,
    )
    assert (
        len(json.dumps(limited, default=str, separators=(",", ":"), ensure_ascii=False).encode())
        <= budget - 1
    )
    assert limited["limitation"] == "byte_limit"
    assert limited["complete"] is False


def test_empty_viewport_is_complete_and_explicitly_empty():
    wire = build_viewport_response(context(), iter(()), limit=500)
    assert wire["items"] == []
    assert wire["complete"] is True
    assert wire["limitation"] is None


def test_unordered_or_wrong_crs_output_cannot_look_usable():
    with pytest.raises(SpatialValidationError):
        build_viewport_response(context(), iter([point("b"), point("a")]), limit=10)
    wrong = RenderFeature("a", NativeGeometry("Point", (1.0, 1.0), 3857, GeometryDimensions.XY), 1)
    with pytest.raises(SpatialValidationError):
        build_viewport_response(context(), iter([wrong]), limit=10)
    assert issubclass(MapReadUnavailableError, RuntimeError)


@pytest.mark.parametrize(
    "expected,count",
    [(4, 3), (4, 5), (MAX_RENDER_SOURCE_FEATURES + 1, MAX_RENDER_SOURCE_FEATURES + 1)],
)
def test_render_snapshot_summary_fails_closed_on_missing_or_overlimit_count(expected, count):
    with pytest.raises(MapReadUnavailableError):
        SqlMapViewportRepository._validate_summary(expected, count, True)


def test_render_snapshot_summary_requires_all_source_rows_valid():
    with pytest.raises(SpatialTransformationOutOfAreaError):
        SqlMapViewportRepository._validate_summary(4, 4, False)
    SqlMapViewportRepository._validate_summary(4, 4, True)
    SqlMapViewportRepository._validate_summary(0, 0, True)
