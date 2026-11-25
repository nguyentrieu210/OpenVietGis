"""R048 independent synthetic controls; no live data or external datum claims."""

from __future__ import annotations

import io
import math
from copy import deepcopy
from uuid import UUID

import ezdxf
import pytest

from openvietgis_api.adapters.cad.dxf_codec import parse_dxf_2d, write_dxf_2d
from openvietgis_api.domains.cad.domain import CadUnit
from openvietgis_api.domains.cad.georeference import (
    fit_placement,
    fit_web_placement,
    place_features,
    restore_features,
)
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError

DOC = UUID("00000000-0000-4000-8000-000000000048")


def drawing(*, with_z: bool = False, with_loss: bool = False) -> bytes:
    doc = ezdxf.new("R2010")
    doc.header["$INSUNITS"] = 4
    msp = doc.modelspace()
    msp.add_lwpolyline([(0, 0), (8000, 0), (8000, 3000), (0, 3000)], close=True)
    if with_loss:
        msp.add_text("native-only", dxfattribs={"insert": (1, 1)})
        msp.add_line((0, 0), (1000, 0))
    if with_z:
        msp.add_line((0, 0, 1), (1000, 0, 1))
    target = io.StringIO()
    doc.write(target)
    return target.getvalue().encode()


def test_independent_rotated_millimeter_rectangle():
    # Synthetic 90-degree rotation and translation calculated explicitly.
    native = (
        ((0, 0), (440000, 2290000)),
        ((8000, 0), (440000, 2290008)),
        ((0, 3000), (439997, 2290000)),
    )
    web = (
        ((0, 0), (11929000, 2359000)),
        ((8000, 0), (11929000, 2359008)),
        ((0, 3000), (11928997, 2359000)),
    )
    placement = fit_web_placement(
        document_id=DOC,
        version=1,
        crs_code=5899,
        units=CadUnit.MILLIMETER,
        controls=native,
        web_controls=web,
        tolerance_m=0.001,
        max_scale_deviation=0.001,
    )
    assert placement.native_reference is not None
    # Holdout point not included in calibration.
    assert math.dist(placement.native_reference.transform_m((8, 3)), (439997, 2290008)) <= 0.001
    assert math.dist(placement.transform_m((8, 3)), (11928997, 2359008)) <= 0.001
    family, features, summary, loss = parse_dxf_2d(drawing(), units=CadUnit.MILLIMETER)
    assert family == "Polygon"
    assert summary["published_feature_count"] == 1
    assert not loss["requires_acknowledgement"]
    bounds = place_features(features, placement)
    expected = {"min_x": 11928997, "max_x": 11929000, "min_y": 2359000, "max_y": 2359008}
    assert bounds == pytest.approx(expected, abs=0.001)
    ring = features[0]["geometry"]["coordinates"][0]
    # Independent area calculation after translating to the first vertex.
    x0, y0 = ring[0]
    local = [(x - x0, y - y0) for x, y in ring]
    area = (
        abs(sum(a[0] * b[1] - a[1] * b[0] for a, b in zip(local[:-1], local[1:], strict=True))) / 2
    )
    assert area == pytest.approx(24, abs=1e-6)
    assert placement.metadata()["survey_grade"] is False


def test_loss_and_unsupported_z_are_explicit():
    _, features, summary, loss = parse_dxf_2d(drawing(with_loss=True), units=CadUnit.MILLIMETER)
    assert len(features) == 1
    assert loss["unsupported_entities"] == {"TEXT": 1}
    assert loss["dropped_secondary_families"] == {"LineString": 1}
    assert loss["requires_acknowledgement"] and not loss["silent_loss"]
    assert {x["disposition"] for x in summary["entity_mapping"]} == {
        "mapped",
        "unsupported",
        "excluded_secondary_family",
    }
    with pytest.raises(PlatformValidationError) as err:
        parse_dxf_2d(drawing(with_z=True), units=CadUnit.MILLIMETER)
    assert err.value.code == "CAD_3D_UNSUPPORTED"


def test_polygon_export_roundtrip_preserves_geometry_and_units():
    _, features, _, _ = parse_dxf_2d(drawing(), units=CadUnit.MILLIMETER)
    payload = write_dxf_2d(features, layer_name="R048", units=CadUnit.MILLIMETER)
    result = ezdxf.read(io.StringIO(payload.decode()))
    assert result.header["$INSUNITS"] == 4
    polygon = list(result.modelspace())[0]
    assert polygon.closed
    vertices = [tuple(map(float, p)) for p in polygon.get_points("xy")]
    # Current exporter keeps the GeoJSON closing vertex; DXF closed bit is also set.
    if vertices[-1] == vertices[0]:
        vertices.pop()
    assert vertices == [(0, 0), (8000, 0), (8000, 3000), (0, 3000)]
    _, reread, _, _ = parse_dxf_2d(payload, units=CadUnit.MILLIMETER)
    assert reread[0]["geometry"] == features[0]["geometry"]


def test_invalid_controls_and_unsupported_crs_fail_closed():
    controls = (((0, 0), (100, 100)), ((1, 0), (101, 100)), ((2, 0), (102, 100)))
    for crs in (3857, 4326):
        with pytest.raises(PlatformValidationError) as err:
            fit_placement(
                document_id=DOC,
                version=1,
                crs_code=crs,
                units=CadUnit.METER,
                controls=controls,
                tolerance_m=0.001,
                max_scale_deviation=0.001,
            )
        assert err.value.code == "CAD_GEOREFERENCE_INVALID"


def _paired_placement():
    return fit_web_placement(
        document_id=DOC,
        version=2,
        crs_code=5899,
        units=CadUnit.MILLIMETER,
        controls=(
            ((0, 0), (440000, 2290000)),
            ((8000, 0), (440000, 2290008)),
            ((0, 3000), (439997, 2290000)),
        ),
        web_controls=(
            ((0, 0), (11929000, 2359000)),
            ((8000, 0), (11929000, 2359008)),
            ((0, 3000), (11928997, 2359000)),
        ),
        tolerance_m=0.001,
        max_scale_deviation=0.001,
    )


def test_inverse_edit_to_local_dxf_roundtrip():
    placement = _paired_placement()
    _, local, _, _ = parse_dxf_2d(drawing(), units=CadUnit.MILLIMETER)
    projected = deepcopy(local)
    place_features(projected, placement)
    # Simulate a reviewed GIS edit: change the local 8 m width to 9 m.
    ring = projected[0]["geometry"]["coordinates"][0]
    ring[1][1] += 1
    ring[2][1] += 1
    snapshot = deepcopy(projected)
    restored = restore_features(
        projected, placement, document_id=DOC, version=2, source_crs_code=3857
    )
    assert projected == snapshot
    assert restored[0]["feature_id"] == local[0]["feature_id"]
    assert restored[0]["attributes"] == local[0]["attributes"]
    payload = write_dxf_2d(restored, layer_name="R048", units=CadUnit.MILLIMETER)
    _, reread, _, _ = parse_dxf_2d(payload, units=CadUnit.MILLIMETER)
    expected = [(0, 0), (9, 0), (9, 3), (0, 3), (0, 0)]
    for a, b in zip(reread[0]["geometry"]["coordinates"][0], expected, strict=True):
        assert math.dist(a, b) <= 0.001
    place_features(reread, placement)
    for a, b in zip(reread[0]["geometry"]["coordinates"][0], ring, strict=True):
        assert math.dist(a, b) <= 0.001


@pytest.mark.parametrize("kind", ["Point", "LineString", "Polygon"])
def test_inverse_supported_geometry_non_mutating(kind):
    placement = _paired_placement()
    a, b, c = [list(placement.transform_m(x)) for x in [(0, 0), (8, 0), (8, 3)]]
    coords = a if kind == "Point" else [a, b] if kind == "LineString" else [[a, b, c, a]]
    features = [{"feature_id": "test", "geometry": {"type": kind, "coordinates": coords}}]
    original = deepcopy(features)
    restored = restore_features(
        features, placement, document_id=DOC, version=2, source_crs_code=3857
    )
    assert original == features
    place_features(restored, placement)
    assert restored[0]["geometry"]["type"] == kind
    expected = [a] if kind == "Point" else [a, b] if kind == "LineString" else [a, b, c, a]
    got = restored[0]["geometry"]["coordinates"]
    actual = [got] if kind == "Point" else got if kind == "LineString" else got[0]
    for p, q in zip(actual, expected, strict=True):
        assert math.dist(p, q) <= 0.001


def test_inverse_rejects_stale_version_wrong_crs_z_and_radius():
    placement = _paired_placement()
    feature = {"geometry": {"type": "Point", "coordinates": [11929000, 2359000]}}
    for doc, version in [(UUID(int=1), 2), (DOC, 1)]:
        with pytest.raises(PlatformConflictError) as err:
            restore_features(
                [feature], placement, document_id=doc, version=version, source_crs_code=3857
            )
        assert err.value.code == "STALE_CAD_REVISION"
    for crs in (4326, 5899):
        with pytest.raises(PlatformValidationError):
            restore_features([feature], placement, document_id=DOC, version=2, source_crs_code=crs)
    for coords in ([11929000, 2359000, 1], [float("nan"), 0], [11949000, 2359000]):
        with pytest.raises(PlatformValidationError):
            restore_features(
                [{"geometry": {"type": "Point", "coordinates": coords}}],
                placement,
                document_id=DOC,
                version=2,
                source_crs_code=3857,
            )


def test_inverse_rejects_open_ring_and_unsupported_geometry():
    placement = _paired_placement()
    a = [11929000, 2359000]
    b = [11929000, 2359001]
    c = [11928999, 2359001]
    for geom in (
        {"type": "Polygon", "coordinates": [[a, b, c, b]]},
        {"type": "MultiPoint", "coordinates": [a, b]},
    ):
        with pytest.raises(PlatformValidationError):
            restore_features(
                [{"geometry": geom}], placement, document_id=DOC, version=2, source_crs_code=3857
            )
