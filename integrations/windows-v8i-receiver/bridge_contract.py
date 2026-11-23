"""Portable read-only V8i exchange contract. No DGN parsing, server writes or implicit CRS transforms."""

from __future__ import annotations

import hashlib
import json
import math
import re
from collections.abc import Mapping
from itertools import pairwise
from typing import Any

SCHEMA_VERSION = "ovg.v8i.snapshot.v1"
MAX_ELEMENTS = 5000
MAX_PROPERTIES = 64
CRS_PATTERN = re.compile(r"^EPSG:[1-9][0-9]{2,5}$")
ELEMENT_ID_PATTERN = re.compile(r"^[A-Za-z0-9_.:-]{1,128}$")
ALLOWED_SNAPSHOT_FIELDS = frozenset({
    "schema_version", "drawing_id", "model_id", "source_crs", "elements", "unsupported_count"
})
ALLOWED_ELEMENT_FIELDS = frozenset({"element_id", "geometry", "properties"})


class ContractError(ValueError):
    """Snapshot cannot safely be interpreted as a lossless source-CRS data record."""


def _require_fields(value: Any, allowed: frozenset[str], required: set[str], label: str) -> Mapping[str, Any]:
    if not isinstance(value, dict) or set(value) - allowed or required - set(value):
        raise ContractError(f"{label}: invalid/missing fields (allowed: {', '.join(sorted(allowed))})")
    return value


def _identifier(value: Any, label: str) -> str:
    if not isinstance(value, str) or not ELEMENT_ID_PATTERN.fullmatch(value):
        raise ContractError(f"{label}: expected 1-128 ASCII letters, digits or _ . : -")
    return value


def _number(value: Any, label: str) -> int | float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ContractError(f"{label}: expected finite number")
    if abs(value) > 1e10:
        raise ContractError(f"{label}: coordinate exceeds safety bound")
    return value


def _position(value: Any, label: str) -> list[int | float]:
    if not isinstance(value, list) or len(value) != 2:
        raise ContractError(f"{label}: expected 2D coordinate [x, y]")
    return [_number(value[0], label + ".x"), _number(value[1], label + ".y")]


def _line(value: Any, label: str, minimum: int = 2) -> list[list[int | float]]:
    if not isinstance(value, list) or not minimum <= len(value) <= 50000:
        raise ContractError(f"{label}: expected {minimum}..50000 positions")
    return [_position(p, label + "[" + str(i) + "]") for i, p in enumerate(value)]


def _polygon(value: Any, label: str) -> list[list[list[int | float]]]:
    if not isinstance(value, list) or not 1 <= len(value) <= 100:
        raise ContractError(f"{label}: expected 1..100 linear rings")
    result: list[list[list[int | float]]] = []
    for i, ring in enumerate(value):
        vertices = _line(ring, f"{label}[{i}]", minimum=4)
        if vertices[0] != vertices[-1]:
            raise ContractError(f"{label}[{i}]: ring is not closed")
        twice_area = sum(
            a[0] * b[1] - b[0] * a[1]
            for a, b in pairwise(vertices)
        )
        if not math.isfinite(twice_area) or abs(twice_area) < 1e-10:
            raise ContractError(f"{label}[{i}]: zero/invalid area")
        result.append(vertices)
    return result


def _geometry(raw: Any) -> dict[str, Any]:
    geom = _require_fields(raw, frozenset({"type", "coordinates"}), {"type", "coordinates"}, "geometry")
    kind = geom["type"]
    if kind == "Point":
        coords = _position(geom["coordinates"], "geometry.coordinates")
    elif kind == "LineString":
        coords = _line(geom["coordinates"], "geometry.coordinates")
    elif kind == "Polygon":
        coords = _polygon(geom["coordinates"], "geometry.coordinates")
    else:
        raise ContractError("geometry.type: only Point, LineString, Polygon are supported in v1")
    return {"type": kind, "coordinates": coords}


def validate_snapshot(raw: Any) -> dict[str, Any]:
    """Validate a source-CRS-native envelope and return canonical normalized fields."""
    snap = _require_fields(
        raw, ALLOWED_SNAPSHOT_FIELDS,
        {"schema_version", "drawing_id", "model_id", "source_crs", "elements", "unsupported_count"},
        "snapshot"
    )
    if snap["schema_version"] != SCHEMA_VERSION:
        raise ContractError("schema_version: unsupported bridge contract")
    drawing_id = _identifier(snap["drawing_id"], "drawing_id")
    model_id = _identifier(snap["model_id"], "model_id")
    source_crs = snap["source_crs"]
    if not isinstance(source_crs, str) or not CRS_PATTERN.fullmatch(source_crs):
        raise ContractError("source_crs: explicit EPSG code required; never guess VN-2000 zone")
    unsupported_count = snap["unsupported_count"]
    if isinstance(unsupported_count, bool) or not isinstance(unsupported_count, int) or not 0 <= unsupported_count <= 10000000:
        raise ContractError("unsupported_count: expected nonnegative integer")
    raw_elements = snap["elements"]
    if not isinstance(raw_elements, list) or len(raw_elements) > MAX_ELEMENTS:
        raise ContractError(f"elements: expected array of at most {MAX_ELEMENTS} entities")
    seen: set[str] = set()
    elements = []
    for i, entry in enumerate(raw_elements):
        element = _require_fields(entry, ALLOWED_ELEMENT_FIELDS, {"element_id", "geometry"}, f"elements[{i}]")
        eid = _identifier(element["element_id"], f"elements[{i}].element_id")
        if eid in seen:
            raise ContractError(f"elements[{i}]: duplicate element_id {eid}")
        seen.add(eid)
        props = element.get("properties", {})
        if not isinstance(props, dict) or len(props) > MAX_PROPERTIES:
            raise ContractError(f"elements[{i}].properties: expected map of at most {MAX_PROPERTIES} fields")
        clean_props: dict[str, Any] = {}
        for key, value in props.items():
            if not isinstance(key, str) or not ELEMENT_ID_PATTERN.fullmatch(key):
                raise ContractError(f"elements[{i}].properties: invalid field identifier")
            if value is not None and not isinstance(value, (str, int, float, bool)):
                raise ContractError(f"elements[{i}].properties.{key}: scalar required")
            if isinstance(value, str) and len(value) > 4096:
                raise ContractError(f"elements[{i}].properties.{key}: string too long")
            if isinstance(value, float) and not math.isfinite(value):
                raise ContractError(f"elements[{i}].properties.{key}: non-finite value")
            clean_props[key] = value
        elements.append({"element_id": eid, "geometry": _geometry(element["geometry"]), "properties": clean_props})
    return {
        "schema_version": SCHEMA_VERSION,
        "drawing_id": drawing_id,
        "model_id": model_id,
        "source_crs": source_crs,
        "unsupported_count": unsupported_count,
        "elements": sorted(elements, key=lambda obj: obj["element_id"]),
    }


def stable_digest(value: Any) -> str:
    """Hash validated canonical data; field and element insertion order cannot change the digest."""
    normalized = validate_snapshot(value)
    encoded = json.dumps(normalized, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def diff_snapshots(previous: Any, current: Any) -> dict[str, Any]:
    """Preview only: missing elements are delete candidates, never automatic deletes."""
    before = validate_snapshot(previous)
    after = validate_snapshot(current)
    for key in ("drawing_id", "model_id", "source_crs"):
        if before[key] != after[key]:
            raise ContractError(f"diff refused: {key} changed")
    if before["unsupported_count"] or after["unsupported_count"]:
        raise ContractError("diff refused: unsupported CAD entities prevent complete-source comparison")
    old = {e["element_id"]: e for e in before["elements"]}
    new = {e["element_id"]: e for e in after["elements"]}
    changes: list[dict[str, Any]] = []
    for eid in sorted(old.keys() | new.keys()):
        b, a = old.get(eid), new.get(eid)
        bd = _element_digest(b) if b is not None else None
        ad = _element_digest(a) if a is not None else None
        if b is None:
            op = "upsert"
        elif a is None:
            op = "delete_candidate"
        elif bd != ad:
            op = "upsert"
        else:
            op = "unchanged"
        changes.append({
            "operation": op, "element_id": eid, "before_digest": bd, "after_digest": ad
        })
    return {
        "schema_version": "ovg.v8i.delta.v1",
        "drawing_id": before["drawing_id"],
        "model_id": before["model_id"],
        "source_crs": before["source_crs"],
        "before_revision": stable_digest(before),
        "after_revision": stable_digest(after),
        "applied": False,
        "changes": changes,
    }


def _element_digest(element: Mapping[str, Any]) -> str:
    return hashlib.sha256(json.dumps(element, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")).hexdigest()