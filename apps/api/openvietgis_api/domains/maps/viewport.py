"""Bounded, explicit geographic presentation values; no renderer dependency."""

from __future__ import annotations

import json
from collections.abc import Iterable
from dataclasses import dataclass
from math import isfinite
from typing import Literal, TypedDict

from openvietgis_api.domains.spatial.crs import (
    WEB_MERCATOR_MAX_LATITUDE,
    get_coordinate_reference,
)
from openvietgis_api.domains.spatial.domain import (
    NativeGeometry,
    SpatialValidationError,
    validate_ready_geometry,
)
from openvietgis_api.domains.spatial.service import (
    FeatureGeometryWire,
    FeatureReadContextWire,
)

MAX_VIEWPORT_FEATURES = 1000
MAX_VIEWPORT_POSITIONS = 20_000
MAX_VIEWPORT_BYTES = 2 * 1024 * 1024
MAX_RENDER_SOURCE_FEATURES = 50_000
MAP_STATEMENT_TIMEOUT_MS = 2000

type Limitation = Literal["feature_limit", "position_limit", "byte_limit"]


class MapReadUnavailableError(RuntimeError):
    """The bounded read failed; never an empty/complete successful response."""


@dataclass(frozen=True, slots=True)
class ViewportBounds:
    west: float
    south: float
    east: float
    north: float

    def __post_init__(self) -> None:
        values = (self.west, self.south, self.east, self.north)
        if any(
            isinstance(value, bool) or not isinstance(value, int | float) or not isfinite(value)
            for value in values
        ):
            raise SpatialValidationError("finite geographic bounds required")
        if not (
            -180 <= self.west < self.east <= 180
            and -WEB_MERCATOR_MAX_LATITUDE <= self.south < self.north <= WEB_MERCATOR_MAX_LATITUDE
        ):
            raise SpatialValidationError("nonwrapping geographic viewport required")


@dataclass(frozen=True, slots=True)
class RenderFeature:
    feature_id: str
    geometry: NativeGeometry | None
    positions: int


class RenderFeatureWire(TypedDict):
    feature_id: str
    geometry: FeatureGeometryWire


class ViewportWire(TypedDict):
    context: FeatureReadContextWire
    items: list[RenderFeatureWire]
    complete: bool
    limitation: Limitation | None


class DisplayExtentWire(TypedDict):
    context: FeatureReadContextWire
    state: Literal["available", "empty", "unavailable"]
    bounds: tuple[float, float, float, float] | None


def serialized_size(value: object) -> int:
    # Matches the UTF-8 JSON response encoder: compact, actual envelope, no NaN.
    return len(
        json.dumps(
            value,
            default=str,
            separators=(",", ":"),
            ensure_ascii=False,
            allow_nan=False,
        ).encode("utf-8")
    )


def build_viewport_response(
    context: FeatureReadContextWire,
    features: Iterable[RenderFeature],
    *,
    limit: int,
    position_budget: int = MAX_VIEWPORT_POSITIONS,
    byte_budget: int = MAX_VIEWPORT_BYTES,
) -> ViewportWire:
    if type(limit) is not int or not 1 <= limit <= MAX_VIEWPORT_FEATURES:
        raise SpatialValidationError("viewport feature limit outside bounds")
    reference = get_coordinate_reference(4326)
    assert reference is not None
    if context["response_crs"] != reference.to_wire() or not context["geometry_included"]:
        raise SpatialValidationError("viewport requires explicit geographic geometry")
    result: ViewportWire = {
        "context": context,
        "items": [],
        "complete": True,
        "limitation": None,
    }
    positions = 0
    item_bytes = 0
    # Empty-array envelope includes both brackets; comma bytes are counted
    # incrementally. Reserve the largest possible limitation string.
    overhead = serialized_size({**result, "complete": False, "limitation": "position_limit"})
    previous: str | None = None
    for feature in features:
        if previous is not None and feature.feature_id <= previous:
            raise SpatialValidationError("viewport order must be strictly stable")
        previous = feature.feature_id
        if len(result["items"]) == limit:
            result.update({"complete": False, "limitation": "feature_limit"})
            break
        if (
            type(feature.positions) is not int
            or feature.positions < 1
            or positions + feature.positions > position_budget
        ):
            result.update({"complete": False, "limitation": "position_limit"})
            break
        if feature.geometry is None:
            raise SpatialValidationError("bounded geometry marker has inconsistent positions")
        vertices = validate_ready_geometry(
            feature.geometry,
            expected_srid=4326,
            output_roundoff=True,
        )
        if len(vertices) != feature.positions:
            raise SpatialValidationError("viewport geometry position count disagrees")
        if any(abs(y) > WEB_MERCATOR_MAX_LATITUDE for _, y in vertices):
            raise SpatialValidationError("geometry cannot be presented in map display domain")
        wire: RenderFeatureWire = {
            "feature_id": feature.feature_id,
            "geometry": {
                "type": feature.geometry.type,
                "coordinates": feature.geometry.coordinates,
                "coordinate_reference": reference.to_wire(),
            },
        }
        encoded_item_size = serialized_size(wire) + (1 if result["items"] else 0)
        if overhead + item_bytes + encoded_item_size > byte_budget:
            result.update({"complete": False, "limitation": "byte_limit"})
            break
        result["items"].append(wire)
        positions += feature.positions
        item_bytes += encoded_item_size
    if serialized_size(result) > byte_budget:
        raise MapReadUnavailableError("viewport envelope exceeds response budget")
    return result
