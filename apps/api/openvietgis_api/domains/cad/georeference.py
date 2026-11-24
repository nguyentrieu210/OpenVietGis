"""Bounded local 2D control placement; no inferred datum transformation."""

from __future__ import annotations

import math
from copy import deepcopy
from dataclasses import dataclass, replace
from typing import Any, NoReturn
from uuid import UUID

from openvietgis_api.domains.cad.domain import UNIT_TO_METERS, CadUnit
from openvietgis_api.domains.editing.domain import validate_geometry_payload
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError
from openvietgis_api.domains.spatial.domain import MAX_POSITIONS, GeometryFamily

Point = tuple[float, float]
Control = tuple[Point, Point]
GRID_LIMIT = 20037508.342789244
LOCAL_RADIUS_M = 10000.0
EARTH_RADIUS_M = 6378137.0


def _fail(reason: str) -> NoReturn:
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
    crs_code: int = 3857
    native_reference: CadPlacement | None = None
    local_radius_m: float | None = None

    def transform_m(self, p: Point) -> Point:
        if not _valid(p):
            _fail("CAD coordinate outside bounded input")
        x = (p[0] - self.origin_m[0]) * self.scale
        y = (p[1] - self.origin_m[1]) * self.scale
        c, s = math.cos(self.rotation), math.sin(self.rotation)
        result = (self.translation[0] + c * x - s * y, self.translation[1] + s * x + c * y)
        if not _grid_valid(result, self.crs_code):
            _fail("Placed geometry outside declared grid bounds")
        if (
            self.local_radius_m is not None
            and math.dist(result, self.translation) > self.local_radius_m
        ):
            _fail("Local VN-2000 placement exceeds supported control-centre radius")
        return result

    def inverse_m(self, p: Point) -> Point:
        """Invert this exact calibrated grid placement; return CAD-local metres."""
        if not _grid_valid(p, self.crs_code):
            _fail("GIS coordinate outside declared grid bounds")
        if self.local_radius_m is not None and math.dist(p, self.translation) > self.local_radius_m:
            _fail("Local VN-2000 placement exceeds supported control-centre radius")
        if not math.isfinite(self.scale) or self.scale <= 0:
            _fail("Invalid placement scale")
        x, y = p[0] - self.translation[0], p[1] - self.translation[1]
        c, s = math.cos(self.rotation), math.sin(self.rotation)
        result = (
            self.origin_m[0] + (c * x + s * y) / self.scale,
            self.origin_m[1] + (-s * x + c * y) / self.scale,
        )
        if not _valid(result):
            _fail("Inverse CAD coordinate outside bounded input")
        if math.dist(self.transform_m(result), p) > self.tolerance_m:
            _fail("Inverse placement exceeds declared numeric tolerance")
        return result

    def metadata(self) -> dict[str, Any]:
        result = {
            "algorithm": "similarity-grid-2d-v1",
            "cad_document_id": str(self.document_id),
            "cad_document_version": self.version,
            "crs_code": self.crs_code,
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
        if self.local_radius_m is not None:
            result["local_radius_grid_m"] = self.local_radius_m
        if self.native_reference is not None:
            result["native_reference"] = self.native_reference.metadata()
            result["reference_method"] = "independent-paired-control-fits"
        return result


def _grid_valid(p: Point, crs_code: int) -> bool:
    if crs_code == 3857:
        return _valid(p, GRID_LIMIT)
    # A broad numeric sanity envelope, not proof of EPSG area or datum accuracy.
    return crs_code == 5899 and _valid(p) and 100000 <= p[0] <= 900000 and 0 <= p[1] <= 3000000


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
    if crs_code not in (3857, 5899) or not 1 <= version <= 20:
        _fail("Unsupported explicit grid profile")
    if not 3 <= len(controls) <= 100 or any(
        not _valid(d) or not _grid_valid(p, crs_code) for d, p in controls
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
        crs_code=crs_code,
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
        crs_code=crs_code,
    )


def fit_web_placement(
    *,
    document_id: UUID,
    version: int,
    crs_code: int,
    units: CadUnit,
    controls: tuple[Control, ...],
    web_controls: tuple[Control, ...] | None,
    tolerance_m: float,
    max_scale_deviation: float,
) -> CadPlacement:
    base = fit_placement(
        document_id=document_id,
        version=version,
        crs_code=crs_code,
        units=units,
        controls=controls,
        tolerance_m=tolerance_m,
        max_scale_deviation=max_scale_deviation,
    )
    if crs_code == 3857:
        if web_controls is not None:
            _fail("Redundant web controls for EPSG:3857")
        return base
    if web_controls is None or tuple(d for d, _ in controls) != tuple(d for d, _ in web_controls):
        _fail("VN-2000 requires matching surveyed/reference web control pairs")
    web = fit_placement(
        document_id=document_id,
        version=version,
        crs_code=3857,
        units=units,
        controls=web_controls,
        tolerance_m=tolerance_m,
        max_scale_deviation=max_scale_deviation,
    )
    # Coarse location sanity check only. It rejects grossly unrelated control grids,
    # never derives a datum transform or validates survey-grade accuracy.
    for (_, (e, n)), (_, (web_e, web_n)) in zip(controls, web_controls, strict=True):
        lat = math.atan(math.sinh(web_n / EARTH_RADIUS_M))
        lon = web_e / EARTH_RADIUS_M
        approximate_e = 500000 + EARTH_RADIUS_M * (lon - math.radians(107.75)) * math.cos(lat)
        approximate_n = EARTH_RADIUS_M * lat
        if abs(approximate_e - e) > 150000 or abs(approximate_n - n) > 150000:
            _fail("VN-2000 and web control positions are grossly inconsistent")
    # Independent local placements only; bound every placed vertex and control.
    for fitted in (base, web):
        if any(math.dist(p, fitted.translation) > LOCAL_RADIUS_M for _, p in fitted.controls):
            _fail("Local VN-2000 control span exceeds the 10 km radius")
    return replace(
        web,
        native_reference=replace(base, local_radius_m=LOCAL_RADIUS_M),
        local_radius_m=LOCAL_RADIUS_M,
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


def restore_features(
    features: list[dict[str, Any]],
    placement: CadPlacement,
    *,
    document_id: UUID,
    version: int,
    source_crs_code: int,
) -> list[dict[str, Any]]:
    """Prepare an isolated local-XY preview, not an authoritative CAD write.

    Input must already be authorized and read at a pinned GIS revision by the
    caller. Output remains in local metres for the existing DXF writer. There
    is deliberately no inferred CRS transform, native entity rewrite or save.
    """
    if (document_id, version) != (placement.document_id, placement.version):
        raise PlatformConflictError("CAD placement revision changed", code="STALE_CAD_REVISION")
    if source_crs_code != placement.crs_code:
        _fail("GIS source CRS does not match the calibrated grid")
    if not 1 <= len(features) <= 10_000:
        _fail("Inverse preview requires 1–10000 features")
    result = []
    total_positions = 0
    for feature in features:
        geom = feature.get("geometry")
        if not isinstance(geom, dict) or geom.get("type") not in ("Point", "LineString", "Polygon"):
            _fail("Unsupported inverse geometry type")
        kind, coords = validate_geometry_payload(geom, GeometryFamily(geom["type"]))
        positions = (
            [coords]
            if kind == "Point"
            else (coords if kind == "LineString" else [p for ring in coords for p in ring])
        )
        total_positions += len(positions)
        if len(positions) > MAX_POSITIONS or total_positions > 200_000:
            _fail("Inverse preview position limit exceeded")

        def point(p: list[float]) -> list[float]:
            local = placement.inverse_m((float(p[0]), float(p[1])))
            # Enforce the independently calibrated native domain too.
            if placement.native_reference is not None:
                placement.native_reference.transform_m(local)
            return list(local)

        new_geometry = {
            "type": kind,
            "coordinates": (
                point(coords)
                if kind == "Point"
                else (
                    [point(p) for p in coords]
                    if kind == "LineString"
                    else [[point(p) for p in ring] for ring in coords]
                )
            ),
        }
        validate_geometry_payload(new_geometry, GeometryFamily(kind))
        result.append({**deepcopy(feature), "geometry": new_geometry})
    return result
