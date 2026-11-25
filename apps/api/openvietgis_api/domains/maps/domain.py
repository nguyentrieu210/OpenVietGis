"""Renderer-independent, bounded map configuration and immutable save values."""

from __future__ import annotations

import hashlib
import json
import math
import re
from dataclasses import dataclass
from datetime import datetime
from typing import Literal
from uuid import UUID, uuid5

MAX_REVISION = 2**53 - 1
MAX_LAYERS = 32
MAX_LATITUDE = 85.05112878


class MapValidationError(ValueError):
    """Invalid map values; never include payload values in public messages."""


def _number(value: object, minimum: float, maximum: float) -> None:
    if (
        isinstance(value, bool)
        or not isinstance(value, int | float)
        or not math.isfinite(value)
        or not minimum <= value <= maximum
    ):
        raise MapValidationError("map number outside bounds")


def _revision(value: object, *, minimum: int = 0) -> None:
    if type(value) is not int or not minimum <= value <= MAX_REVISION:
        raise MapValidationError("invalid configuration revision")


def _id(value: object) -> None:
    if not isinstance(value, UUID):
        raise MapValidationError("logical UUID required")


def map_identity(project_id: UUID) -> UUID:
    _id(project_id)
    return uuid5(project_id, "openvietgis:map")


@dataclass(frozen=True, slots=True)
class Style:
    color: str = "#2563EB"
    opacity: float = 0.8
    width: float = 2
    radius: float = 5

    def __post_init__(self) -> None:
        if not isinstance(self.color, str) or re.fullmatch(r"#[0-9A-Fa-f]{6}", self.color) is None:
            raise MapValidationError("invalid basic color")
        object.__setattr__(self, "color", self.color.upper())
        _number(self.opacity, 0, 1)
        _number(self.width, 1, 12)
        _number(self.radius, 2, 20)
        for name in ("opacity", "width", "radius"):
            number = float(getattr(self, name))
            object.__setattr__(self, name, 0.0 if number == 0 else number)

    def to_wire(self) -> dict[str, object]:
        return {
            "color": self.color,
            "opacity": self.opacity,
            "width": self.width,
            "radius": self.radius,
        }


@dataclass(frozen=True, slots=True)
class Viewpoint:
    longitude: float
    latitude: float
    zoom: float
    bearing: float
    pitch: float
    crs: Literal[4326] = 4326

    def __post_init__(self) -> None:
        for value, low, high in (
            (self.longitude, -180, 180),
            (self.latitude, -MAX_LATITUDE, MAX_LATITUDE),
            (self.zoom, 0, 22),
            (self.bearing, -180, 180),
            (self.pitch, 0, 60),
        ):
            _number(value, low, high)
        for name in ("longitude", "latitude", "zoom", "bearing", "pitch"):
            number = float(getattr(self, name))
            object.__setattr__(self, name, 0.0 if number == 0 else number)
        if type(self.crs) is not int or self.crs != 4326:
            raise MapValidationError("explicit geographic viewpoint required")

    def to_wire(self) -> dict[str, object]:
        return {
            "longitude": self.longitude,
            "latitude": self.latitude,
            "zoom": self.zoom,
            "bearing": self.bearing,
            "pitch": self.pitch,
            "crs": self.crs,
        }


@dataclass(frozen=True, slots=True)
class LayerReference:
    layer_id: UUID
    visible: bool
    style: Style
    # Spec 056: project-shared layer group path (QGIS-style tree), e.g. "Hiện trạng/Giao thông".
    group: str | None = None

    def __post_init__(self) -> None:
        _id(self.layer_id)
        if type(self.visible) is not bool or not isinstance(self.style, Style):
            raise MapValidationError("invalid map Layer reference")
        if self.group is not None:
            parts = self.group.split("/") if isinstance(self.group, str) else []
            if (
                not parts
                or len(self.group) > 200
                or len(parts) > 4
                or any(not p.strip() or p != p.strip() for p in parts)
            ):
                raise MapValidationError("invalid map Layer group")

    def to_wire(self) -> dict[str, object]:
        wire: dict[str, object] = {
            "layer_id": str(self.layer_id),
            "visible": self.visible,
            "style": self.style.to_wire(),
        }
        # Omitted when ungrouped so existing revisions keep their digest.
        if self.group is not None:
            wire["group"] = self.group
        return wire


@dataclass(frozen=True, slots=True)
class Configuration:
    layers: tuple[LayerReference, ...] = ()
    basemap: Literal["grid"] = "grid"
    default_viewpoint: Viewpoint | None = None

    def __post_init__(self) -> None:
        layers = tuple(self.layers)
        if (
            len(layers) > MAX_LAYERS
            or any(not isinstance(item, LayerReference) for item in layers)
            or len({item.layer_id for item in layers}) != len(layers)
            or self.basemap != "grid"
            or (
                self.default_viewpoint is not None
                and not isinstance(self.default_viewpoint, Viewpoint)
            )
        ):
            raise MapValidationError("invalid map configuration")
        object.__setattr__(self, "layers", layers)

    def to_wire(self) -> dict[str, object]:
        return {
            "layers": [item.to_wire() for item in self.layers],
            "basemap": self.basemap,
            "default_viewpoint": self.default_viewpoint.to_wire()
            if self.default_viewpoint
            else None,
        }


@dataclass(frozen=True, slots=True)
class ViewpointIntent:
    mode: str
    value: Viewpoint | None = None

    def __post_init__(self) -> None:
        if (
            self.mode not in ("preserve", "clear", "set")
            or (self.mode == "set") != isinstance(self.value, Viewpoint)
            or (self.mode != "set" and self.value is not None)
        ):
            raise MapValidationError("invalid designated viewpoint intent")

    def to_wire(self) -> dict[str, object]:
        return {"mode": self.mode, **({"value": self.value.to_wire()} if self.value else {})}


@dataclass(frozen=True, slots=True)
class SaveIntent:
    operation_id: UUID
    expected_revision: int
    layers: tuple[LayerReference, ...]
    basemap: Literal["grid"]
    viewpoint: ViewpointIntent

    def __post_init__(self) -> None:
        _id(self.operation_id)
        _revision(self.expected_revision)
        validated = Configuration(self.layers, self.basemap)
        object.__setattr__(self, "layers", validated.layers)
        if not isinstance(self.viewpoint, ViewpointIntent):
            raise MapValidationError("designated viewpoint intent required")

    @property
    def fingerprint(self) -> str:
        payload = {
            "expected_revision": self.expected_revision,
            "layers": [item.to_wire() for item in self.layers],
            "basemap": self.basemap,
            "default_viewpoint": self.viewpoint.to_wire(),
        }
        return hashlib.sha256(
            json.dumps(
                payload,
                sort_keys=True,
                separators=(",", ":"),
                allow_nan=False,
            ).encode("utf-8")
        ).hexdigest()

    def apply(self, previous: Configuration) -> Configuration:
        viewpoint = (
            previous.default_viewpoint
            if self.viewpoint.mode == "preserve"
            else self.viewpoint.value
        )
        return Configuration(self.layers, self.basemap, viewpoint)


@dataclass(frozen=True, slots=True)
class MapState:
    map_id: UUID
    organization_id: UUID
    project_id: UUID
    configuration_revision: int
    configuration: Configuration
    updated_at: datetime | None = None

    def __post_init__(self) -> None:
        for value in (self.map_id, self.organization_id, self.project_id):
            _id(value)
        if self.map_id != map_identity(self.project_id):
            raise MapValidationError("map identity must belong to Project")
        _revision(self.configuration_revision)
        if not isinstance(self.configuration, Configuration):
            raise MapValidationError("map configuration required")

    def to_wire(self) -> dict[str, object]:
        return {
            "map_id": self.map_id,
            "organization_id": self.organization_id,
            "project_id": self.project_id,
            "configuration_revision": self.configuration_revision,
            "configuration": self.configuration.to_wire(),
        }


@dataclass(frozen=True, slots=True)
class MapReceipt:
    map_id: UUID
    operation_id: UUID
    configuration_revision: int

    def __post_init__(self) -> None:
        _id(self.map_id)
        _id(self.operation_id)
        _revision(self.configuration_revision, minimum=1)

    def to_wire(self) -> dict[str, object]:
        return {
            "map_id": self.map_id,
            "operation_id": self.operation_id,
            "configuration_revision": self.configuration_revision,
        }


@dataclass(frozen=True, slots=True)
class MapOperation:
    operation_id: UUID
    map_id: UUID
    actor_type: str
    actor_id: UUID
    request_hash: str
    committed_revision: int
    created_at: datetime

    @property
    def receipt(self) -> MapReceipt:
        return MapReceipt(self.map_id, self.operation_id, self.committed_revision)


@dataclass(frozen=True, slots=True)
class MapRevision:
    map_id: UUID
    operation_id: UUID
    configuration_revision: int
    prior_revision: int
    actor_type: str
    actor_id: UUID
    actor_display_snapshot: str
    created_at: datetime
    before_configuration: Configuration | None
    after_configuration: Configuration
    audit_event_id: UUID

    def to_wire(self) -> dict[str, object]:
        return {
            "map_id": self.map_id,
            "operation_id": self.operation_id,
            "configuration_revision": self.configuration_revision,
            "prior_revision": self.prior_revision,
            "actor_type": self.actor_type,
            "actor_id": self.actor_id,
            "actor_display_snapshot": self.actor_display_snapshot,
            "created_at": self.created_at,
            "before_configuration": self.before_configuration.to_wire()
            if self.before_configuration
            else None,
            "after_configuration": self.after_configuration.to_wire(),
        }
