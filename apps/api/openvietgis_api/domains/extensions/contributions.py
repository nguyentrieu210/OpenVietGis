"""Bounded data-only project contributions; no code, URLs or attributes."""

from __future__ import annotations

import math
import re
from typing import Any

from openvietgis_api.domains.platform_errors import PlatformValidationError

_LABEL_BAD = re.compile(r"[\x00-\x1f\x7f\u202a-\u202e\u2066-\u2069]")
_DEPTH = {
    "Point": 0,
    "MultiPoint": 1,
    "LineString": 1,
    "MultiLineString": 2,
    "Polygon": 2,
    "MultiPolygon": 3,
}


def clean_label(value: Any, limit: int) -> str:
    if (
        not isinstance(value, str)
        or not value.strip()
        or len(value) > limit
        or _LABEL_BAD.search(value)
    ):
        raise PlatformValidationError("invalid contribution label")
    return value.strip()


def validate_contributions(value: dict[str, Any], capabilities: list[str]) -> dict[str, Any]:
    invalid = PlatformValidationError("invalid partner contributions")
    if set(value) != {"overlay", "tools"} or not isinstance(value["tools"], list):
        raise invalid
    tools = value["tools"]
    if len(tools) > 4 or (tools and "tool_register" not in capabilities):
        raise invalid
    clean_tools = []
    for tool in tools:
        if (
            not isinstance(tool, dict)
            or set(tool) != {"id", "label"}
            or not isinstance(tool["id"], str)
            or not re.fullmatch(r"[a-z][a-z0-9_]{0,31}", tool["id"])
        ):
            raise invalid
        clean_tools.append({"id": tool["id"], "label": clean_label(tool["label"], 40)})
    if len({tool["id"] for tool in clean_tools}) != len(clean_tools):
        raise invalid
    overlay = value["overlay"]
    if overlay is None:
        return {"overlay": None, "tools": clean_tools}
    if (
        "map_overlay" not in capabilities
        or not isinstance(overlay, dict)
        or set(overlay) != {"title", "geojson"}
    ):
        raise invalid
    title = clean_label(overlay["title"], 60)
    fc = overlay["geojson"]
    if (
        not isinstance(fc, dict)
        or set(fc) != {"type", "features"}
        or fc["type"] != "FeatureCollection"
        or not isinstance(fc["features"], list)
        or not 1 <= len(fc["features"]) <= 100
    ):
        raise invalid
    positions = 0
    nodes = 0

    def walk(value: Any, depth: int) -> Any:
        nonlocal positions, nodes
        nodes += 1
        if nodes > 8192 or not isinstance(value, list) or not 1 <= len(value) <= 2048:
            raise invalid
        if depth == 0:
            positions += 1
            if (
                positions > 4096
                or len(value) != 2
                or any(type(x) not in (int, float) for x in value)
            ):
                raise invalid
            if (
                not all(math.isfinite(x) for x in value)
                or abs(value[0]) > 180
                or abs(value[1]) > 90
            ):
                raise invalid
            return value
        return [walk(x, depth - 1) for x in value]

    def ring(v: list) -> bool:
        return len(v) >= 4 and v[0] == v[-1]

    def polygon(v: list) -> bool:
        return all(ring(r) for r in v)

    features = []
    for feature in fc["features"]:
        if (
            not isinstance(feature, dict)
            or set(feature) != {"type", "geometry", "properties"}
            or feature["type"] != "Feature"
            or feature["properties"] != {}
        ):
            raise invalid
        g = feature["geometry"]
        if (
            not isinstance(g, dict)
            or set(g) != {"type", "coordinates"}
            or not isinstance(g["type"], str)
            or g["type"] not in _DEPTH
        ):
            raise invalid
        kind = g["type"]
        coords = walk(g["coordinates"], _DEPTH[kind])
        if (
            kind == "LineString"
            and len(coords) < 2
            or kind == "MultiLineString"
            and any(len(line) < 2 for line in coords)
            or kind == "Polygon"
            and not polygon(coords)
            or kind == "MultiPolygon"
            and any(not polygon(p) for p in coords)
        ):
            raise invalid
        features.append(
            {"type": "Feature", "geometry": {"type": kind, "coordinates": coords}, "properties": {}}
        )
    return {
        "overlay": {"title": title, "geojson": {"type": "FeatureCollection", "features": features}},
        "tools": clean_tools,
    }
