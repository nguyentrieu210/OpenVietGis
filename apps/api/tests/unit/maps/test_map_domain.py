from __future__ import annotations

from dataclasses import replace
from uuid import uuid4

import pytest

from openvietgis_api.domains.maps.domain import (
    Configuration,
    LayerReference,
    MapValidationError,
    SaveIntent,
    Style,
    Viewpoint,
    ViewpointIntent,
    map_identity,
)


def test_map_identity_is_stable_and_project_specific():
    project = uuid4()
    assert map_identity(project) == map_identity(project)
    assert map_identity(project) != map_identity(uuid4())


@pytest.mark.parametrize(
    "field,value",
    [
        ("longitude", True),
        ("longitude", float("nan")),
        ("longitude", 181),
        ("latitude", 86),
        ("zoom", -1),
        ("bearing", 181),
        ("pitch", 61),
        ("crs", 3857),
    ],
)
def test_viewpoint_rejects_invalid_numbers_and_crs(field, value):
    with pytest.raises(MapValidationError):
        replace(Viewpoint(106.0, 11.0, 10, 0, 0), **{field: value})


@pytest.mark.parametrize(
    "kwargs",
    [
        {"color": "url(secret)"},
        {"opacity": True},
        {"opacity": 1.1},
        {"width": 0},
        {"radius": 100},
        {"width": float("inf")},
    ],
)
def test_style_is_bounded_and_cannot_carry_renderer_expressions(kwargs):
    with pytest.raises(MapValidationError):
        replace(Style(), **kwargs)


def test_configuration_rejects_duplicate_or_too_many_layer_references():
    layer = LayerReference(uuid4(), True, Style())
    with pytest.raises(MapValidationError):
        Configuration((layer, layer))
    with pytest.raises(MapValidationError):
        Configuration(tuple(LayerReference(uuid4(), True, Style()) for _ in range(33)))


def test_viewpoint_preserve_and_clear_are_explicit():
    view = Viewpoint(106.0, 11.0, 10, 0, 0)
    old = Configuration(default_viewpoint=view)
    preserve = SaveIntent(uuid4(), 7, (), "grid", ViewpointIntent("preserve"))
    assert preserve.apply(old).default_viewpoint == view
    assert (
        replace(preserve, viewpoint=ViewpointIntent("clear")).apply(old).default_viewpoint is None
    )
    other = Viewpoint(107.0, 12.0, 9, 0, 0)
    assert (
        replace(preserve, viewpoint=ViewpointIntent("set", other)).apply(old).default_viewpoint
        == other
    )


def test_save_fingerprint_binds_expected_revision_and_explicit_intent():
    intent = SaveIntent(uuid4(), 0, (), "grid", ViewpointIntent("preserve"))
    assert intent.fingerprint == replace(intent, operation_id=uuid4()).fingerprint
    assert intent.fingerprint != replace(intent, expected_revision=1).fingerprint
    assert intent.fingerprint != replace(intent, viewpoint=ViewpointIntent("clear")).fingerprint


@pytest.mark.parametrize(
    "mode,value", [("set", None), ("preserve", Viewpoint(0, 0, 0, 0, 0)), ("bad", None)]
)
def test_viewpoint_intent_is_a_closed_union(mode, value):
    with pytest.raises(MapValidationError):
        ViewpointIntent(mode, value)


def test_layer_group_is_optional_bounded_and_omitted_when_unset():
    """Spec 056: project-shared QGIS-style layer group path."""
    plain = LayerReference(uuid4(), True, Style())
    assert "group" not in plain.to_wire()
    grouped = LayerReference(uuid4(), True, Style(), "Hiện trạng/Giao thông")
    assert grouped.to_wire()["group"] == "Hiện trạng/Giao thông"
    for bad in ("", "/", "A//B", " A", "A/B/C/D/E", "x" * 201):
        with pytest.raises(MapValidationError):
            LayerReference(uuid4(), True, Style(), bad)


def test_layer_display_is_bounded_json_and_omitted_when_unset():
    """Spec 056 đợt 3: project-shared display settings."""
    assert "display" not in LayerReference(uuid4(), True, Style()).to_wire()
    view = {"renderer": "graduated", "gradField": "dien_tich", "gradBreaks": [0, 10.5, 99]}
    assert LayerReference(uuid4(), True, Style(), None, view).to_wire()["display"] == view
    for bad in ({"x": "y" * 17_000}, {"n": float("nan")}, {"s": {1, 2}}):
        with pytest.raises(MapValidationError):
            LayerReference(uuid4(), True, Style(), None, bad)
