"""Bounded 2D control-point similarity placement (grid metres, EPSG:3857 only)."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any
from uuid import UUID

from openvietgis_api.domains.cad.domain import UNIT_TO_METERS, CadUnit
from openvietgis_api.domains.platform_errors import PlatformValidationError

Point = tuple[float, float]
Control = tuple[Point, Point]
GRID_LIMIT = 20037508.342789244


def _fail(reason: str) -> None:
    raise PlatformValidationError(reason, code="CAD_GEOREFERENCE_INVALID")


def _valid(p: Point, limit: float = 1e9) -> bool:
    return len(p) == 2 and all(math.isfinite(x) and abs(x) <= limit for x in p)


@dataclass(frozen=True, slots=True)
class CadPlacement:
    document_id: UUID
    version: int
    controls: tuple[Control, ...]
    units: CadUnit
    origin_m: Point
    translation: Point
    scale: float
    rotation: float
    tolerance_m: float
    max_scale_deviation: float
    residuals: tuple[float, ...]

    def transform_m(self, p: Point) -> Point:
        if not _valid(p):
            _fail("CAD coordinate outside bounded input")
        x = (p[0] - self.origin_m[0]) * self.scale
        y = (p[1] - self.origin_m[1]) * self.scale
        c, s = math.cos(self.rotation), math.sin(self.rotation)
        result = (self.translation[0] + c * x - s * y, self.translation[1] + s * x + c * y)
        if not _valid(result, GRID_LIMIT):
            _fail("Placed geometry outside EPSG:3857 grid")
        return result

    def metadata(self) -> dict[str, Any]:
        return {
            "algorithm": "similarity-grid-2d-v1",
            "cad_document_id": str(self.document_id),
            "cad_document_version": self.version,
            "crs_code": 3857,
            "units": self.units.value,
            "controls": self.controls,
            "origin_m": self.origin_m,
            "translation": self.translation,
            "scale": self.scale,
            "rotation_radians": self.rotation,
            "tolerance_grid_m": self.tolerance_m,
            "max_scale_deviation": self.max_scale_deviation,
            "rmse_grid_m": math.sqrt(sum(x * x for x in self.residuals) / len(self.residuals)),
            "max_residual_grid_m": max(self.residuals),
            "residuals_grid_m": self.residuals,
            "survey_grade": False,
        }


def fit_placement(
    *,
    document_id: UUID,
    version: int,
    crs_code: int,
    units: CadUnit,
    controls: tuple[Control, ...],
    tolerance_m: float,
    max_scale_deviation: float,
) -> CadPlacement:
    if crs_code != 3857 or not 1 <= version <= 20:
        _fail("Initial placement supports explicit EPSG:3857 grid metres only")
    if not 3 <= len(controls) <= 100 or any(
        not _valid(d) or not _valid(p, GRID_LIMIT) for d, p in controls
    ):
        _fail("3–100 finite control point pairs required")
    if not math.isfinite(tolerance_m) or not 1e-6 <= tolerance_m <= 1000:
        _fail("Control residual tolerance must be between 0.000001 and 1000 grid metres")
    if not math.isfinite(max_scale_deviation) or not 0 <= max_scale_deviation <= 0.2:
        _fail("Scale deviation must be between 0 and 0.2")
    u = UNIT_TO_METERS[units]
    n = len(controls)
    ox = sum(d[0] * u for d, _ in controls) / n
    oy = sum(d[1] * u for d, _ in controls) / n
    tx = sum(p[0] for _, p in controls) / n
    ty = sum(p[1] for _, p in controls) / n
    xx = yy = xy = a = b = 0.0
    for d, p in controls:
        x, y = d[0] * u - ox, d[1] * u - oy
        X, Y = p[0] - tx, p[1] - ty
        xx += x * x
        yy += y * y
        xy += x * y
        a += x * X + y * Y
        b += x * Y - y * X
    norm = xx + yy
    if norm < 1e-12 or xx * yy - xy * xy <= 1e-10 * norm * norm:
        _fail("Control points are collinear or degenerate")
    scale = math.hypot(a, b) / norm
    rotation = math.atan2(b, a)
    if not math.isfinite(scale) or scale <= 0 or abs(scale - 1) > max_scale_deviation + 1e-9:
        _fail("Scale is inconsistent with declared drawing units")
    provisional = CadPlacement(
        document_id,
        version,
        controls,
        units,
        (ox, oy),
        (tx, ty),
        scale,
        rotation,
        tolerance_m,
        max_scale_deviation,
        (0.0,),
    )
    residuals = tuple(
        math.dist(provisional.transform_m((d[0] * u, d[1] * u)), p) for d, p in controls
    )
    if max(residuals) > tolerance_m:
        _fail("Control point residual exceeds tolerance")
    return CadPlacement(
        document_id,
        version,
        controls,
        units,
        (ox, oy),
        (tx, ty),
        scale,
        rotation,
        tolerance_m,
        max_scale_deviation,
        residuals,
    )


def place_features(features: list[dict[str, Any]], placement: CadPlacement) -> dict[str, float]:
    bounds = [math.inf, math.inf, -math.inf, -math.inf]

    def point(p: list[float]) -> list[float]:
        x, y = placement.transform_m((p[0], p[1]))
        bounds[0], bounds[1] = min(bounds[0], x), min(bounds[1], y)
        bounds[2], bounds[3] = max(bounds[2], x), max(bounds[3], y)
        return [x, y]

    for feature in features:
        geometry = feature["geometry"]
        coords = geometry["coordinates"]
        if geometry["type"] == "Point":
            geometry["coordinates"] = point(coords)
        elif geometry["type"] == "LineString":
            geometry["coordinates"] = [point(p) for p in coords]
        elif geometry["type"] == "Polygon":
            geometry["coordinates"] = [[point(p) for p in ring] for ring in coords]
        else:
            _fail("Unsupported geometry type")
    if not all(math.isfinite(v) for v in bounds):
        _fail("No placed geometry")
    return dict(zip(("min_x", "min_y", "max_x", "max_y"), bounds, strict=True))
