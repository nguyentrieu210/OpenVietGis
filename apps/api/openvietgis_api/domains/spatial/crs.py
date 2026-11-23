"""Immutable R003 CRS definitions and approved directed-operation metadata.

The registry describes the plan's finite control-verified operations. It does not
execute a projection, authorize a source, or advertise a working read endpoint.
US5 must validate the exact runtime and independent controls before exposing it.
"""

from dataclasses import dataclass
from math import isfinite
from types import MappingProxyType
from typing import Literal, TypedDict

CRS_REGISTRY_VERSION = "r003-v1"
WEB_MERCATOR_MAX_LATITUDE = 85.0511287798066
WEB_MERCATOR_MAX_COORDINATE_METRE = 20037508.342789244
WEB_MERCATOR_OUTPUT_EPSILON_METRE = 1e-6

type Axis = Literal["longitude", "latitude", "easting", "northing"]
type Unit = Literal["degree", "metre"]
type Disposition = Literal["readable", "definition_only"]
type ProjectionValue = str | float
type ProjectionParameters = tuple[tuple[str, ProjectionValue], ...]


class CoordinateReferenceWire(TypedDict):
    authority: Literal["EPSG"]
    code: int
    coordinate_order: Literal["xy"]
    x_axis: Axis
    y_axis: Axis
    unit: Unit


class CRSProfileWire(CoordinateReferenceWire):
    name: str
    datum: str
    native_axes: list[Axis]
    projection_parameters: dict[str, ProjectionValue]
    area_of_use: str
    disposition: Disposition


class TransformationProfileWire(TypedDict):
    source_code: int
    target_code: int
    operation_id: str
    method: str
    validated_area: str
    tolerance: float
    tolerance_unit: Unit
    evidence_id: str


@dataclass(frozen=True, slots=True)
class CoordinateBounds:
    """Closed X/Y rectangle; units are declared by the owning profile.

    Epsilon is only for a documented output roundoff policy; normal input checks
    must use strict bounds. Checking a rectangle does not identify a datum or
    prove a source's provenance.
    """

    min_x: float
    min_y: float
    max_x: float
    max_y: float

    def contains(self, x: float, y: float, *, epsilon: float = 0.0) -> bool:
        if isinstance(x, bool) or isinstance(y, bool) or isinstance(epsilon, bool):
            return False
        try:
            if not all(isfinite(value) for value in (x, y, epsilon)) or epsilon < 0:
                return False
        except (OverflowError, TypeError):
            return False
        return (
            self.min_x - epsilon <= x <= self.max_x + epsilon
            and self.min_y - epsilon <= y <= self.max_y + epsilon
        )


GEOGRAPHIC_NATIVE_BOUNDS = CoordinateBounds(-180.0, -90.0, 180.0, 90.0)
WEB_MERCATOR_GEOGRAPHIC_BOUNDS = CoordinateBounds(
    -180.0,
    -WEB_MERCATOR_MAX_LATITUDE,
    180.0,
    WEB_MERCATOR_MAX_LATITUDE,
)
WEB_MERCATOR_PROJECTED_BOUNDS = CoordinateBounds(
    -WEB_MERCATOR_MAX_COORDINATE_METRE,
    -WEB_MERCATOR_MAX_COORDINATE_METRE,
    WEB_MERCATOR_MAX_COORDINATE_METRE,
    WEB_MERCATOR_MAX_COORDINATE_METRE,
)
QUANG_NINH_GEOGRAPHIC_BOUNDS = CoordinateBounds(106.88, 20.70, 108.07, 21.42)
QUANG_NINH_TM3_BOUNDS = CoordinateBounds(107.15, 20.70, 107.18, 20.73)


@dataclass(frozen=True, slots=True)
class CoordinateReference:
    code: int
    x_axis: Axis
    y_axis: Axis
    unit: Unit

    def to_wire(self) -> CoordinateReferenceWire:
        return {
            "authority": "EPSG",
            "code": self.code,
            "coordinate_order": "xy",
            "x_axis": self.x_axis,
            "y_axis": self.y_axis,
            "unit": self.unit,
        }


@dataclass(frozen=True, slots=True)
class CRSProfile:
    code: int
    name: str
    datum: str
    native_axes: tuple[Axis, Axis]
    x_axis: Axis
    y_axis: Axis
    unit: Unit
    projection_parameters: ProjectionParameters
    area_of_use: str
    disposition: Disposition
    # A strict native numeric domain only, not the definition's area of use.
    # Projected VN-2000 area checking needs the approved inverse projection.
    native_bounds: CoordinateBounds | None

    def coordinate_reference(self) -> CoordinateReference | None:
        if self.disposition != "readable":
            return None
        return CoordinateReference(self.code, self.x_axis, self.y_axis, self.unit)

    def to_wire(self) -> CRSProfileWire:
        return {
            "authority": "EPSG",
            "code": self.code,
            "name": self.name,
            "datum": self.datum,
            "native_axes": list(self.native_axes),
            "unit": self.unit,
            "coordinate_order": "xy",
            "x_axis": self.x_axis,
            "y_axis": self.y_axis,
            "projection_parameters": dict(self.projection_parameters),
            "area_of_use": self.area_of_use,
            "disposition": self.disposition,
        }


def _transverse_mercator_parameters(
    central_meridian: float, scale_factor: float
) -> ProjectionParameters:
    return (
        ("method", "Transverse Mercator (EPSG:9807)"),
        ("latitude_of_origin", 0.0),
        ("central_meridian", central_meridian),
        ("scale_factor", scale_factor),
        ("false_easting", 500000.0),
        ("false_northing", 0.0),
    )


# Canonical EPSG definitions, not legacy spatial_ref_sys WKT display names.
# Sources: crs-support-matrix.md and EPSG definition records 4326/3857/4756/
# 5899/3405/3406. Definition areas intentionally differ from product guard rails.
CRS_PROFILES: tuple[CRSProfile, ...] = (
    CRSProfile(
        code=4326,
        name="WGS 84",
        datum="World Geodetic System 1984 ensemble",
        native_axes=("latitude", "longitude"),
        x_axis="longitude",
        y_axis="latitude",
        unit="degree",
        projection_parameters=(),
        area_of_use="World.",
        disposition="readable",
        native_bounds=GEOGRAPHIC_NATIVE_BOUNDS,
    ),
    CRSProfile(
        code=3857,
        name="WGS 84 / Pseudo-Mercator",
        datum="World Geodetic System 1984 ensemble",
        native_axes=("easting", "northing"),
        x_axis="easting",
        y_axis="northing",
        unit="metre",
        projection_parameters=(
            ("method", "Popular Visualisation Pseudo Mercator (EPSG:1024)"),
            ("latitude_of_origin", 0.0),
            ("central_meridian", 0.0),
            ("false_easting", 0.0),
            ("false_northing", 0.0),
            ("sphere_radius", 6378137.0),
        ),
        area_of_use="World between 85.06°S and 85.06°N.",
        disposition="readable",
        native_bounds=WEB_MERCATOR_PROJECTED_BOUNDS,
    ),
    CRSProfile(
        code=4756,
        name="VN-2000",
        datum="Vietnam 2000",
        native_axes=("latitude", "longitude"),
        x_axis="longitude",
        y_axis="latitude",
        unit="degree",
        projection_parameters=(),
        area_of_use="Vietnam - onshore.",
        disposition="readable",
        native_bounds=GEOGRAPHIC_NATIVE_BOUNDS,
    ),
    CRSProfile(
        code=5899,
        name="VN-2000 / TM-3 107-45",
        datum="Vietnam 2000",
        native_axes=("easting", "northing"),
        x_axis="easting",
        y_axis="northing",
        unit="metre",
        projection_parameters=_transverse_mercator_parameters(107.75, 0.9999),
        area_of_use=(
            "Vietnam - Quang Ninh province; Da Nang municipality and Quang Nam province; "
            "Ba Ria-Vung Tau, Dong Nai and Lam Dong provinces."
        ),
        disposition="readable",
        native_bounds=None,
    ),
    CRSProfile(
        code=3405,
        name="VN-2000 / UTM zone 48N",
        datum="Vietnam 2000",
        native_axes=("easting", "northing"),
        x_axis="easting",
        y_axis="northing",
        unit="metre",
        projection_parameters=_transverse_mercator_parameters(105.0, 0.9996),
        area_of_use="Vietnam - onshore west of 108°E.",
        disposition="definition_only",
        native_bounds=None,
    ),
    CRSProfile(
        code=3406,
        name="VN-2000 / UTM zone 49N",
        datum="Vietnam 2000",
        native_axes=("easting", "northing"),
        x_axis="easting",
        y_axis="northing",
        unit="metre",
        projection_parameters=_transverse_mercator_parameters(111.0, 0.9996),
        area_of_use="Vietnam - onshore east of 108°E.",
        disposition="definition_only",
        native_bounds=None,
    ),
)
_CRS_BY_CODE = MappingProxyType({profile.code: profile for profile in CRS_PROFILES})


def get_crs_profile(code: int) -> CRSProfile | None:
    """Look up a definition; presence is not eligibility for coordinate reads."""
    if type(code) is not int:
        return None
    return _CRS_BY_CODE.get(code)


def get_readable_crs_profile(code: int) -> CRSProfile | None:
    """A readable definition still requires a validated, authorized native source."""
    profile = get_crs_profile(code)
    if profile is None or profile.disposition != "readable":
        return None
    return profile


def get_coordinate_reference(code: int) -> CoordinateReference | None:
    profile = get_readable_crs_profile(code)
    return profile.coordinate_reference() if profile is not None else None


WEB_MERCATOR_PIPELINE = (
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +ellps=WGS84"
)
EPSG_6960_PIPELINE = "urn:ogc:def:coordinateOperation:EPSG::6960"
TM3_PIPELINE = (
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad "
    "+step +proj=tmerc +lat_0=0 +lon_0=107.75 +k=0.9999 "
    "+x_0=500000 +y_0=0 +ellps=WGS84"
)


@dataclass(frozen=True, slots=True)
class PipelineStep:
    """A fixed future PostGIS step; never accept a caller-supplied pipeline.

    The adapter must check the input SRID (the function ignores it), call
    ST_TransformPipeline or ST_InverseTransformPipeline with the explicit
    target SRID, and check the returned geometry.
    """

    source_code: int
    target_code: int
    pipeline: str
    inverse: bool


@dataclass(frozen=True, slots=True)
class TransformationProfile:
    source_code: int
    target_code: int
    operation_id: str
    method: str
    validated_area: str
    tolerance: float
    tolerance_unit: Unit
    evidence_id: str
    # Source/target vertices checked geographically where applicable; if
    # projected first evaluate the approved inverse projection. This is a
    # predicate specification, not operation execution or source validation.
    geographic_bounds: CoordinateBounds
    requires_trusted_fixture: bool
    steps: tuple[PipelineStep, ...]

    def to_wire(self) -> TransformationProfileWire:
        return {
            "source_code": self.source_code,
            "target_code": self.target_code,
            "operation_id": self.operation_id,
            "method": self.method,
            "validated_area": self.validated_area,
            "tolerance": self.tolerance,
            "tolerance_unit": self.tolerance_unit,
            "evidence_id": self.evidence_id,
        }


_WEB_AREA = (
    "Longitude [-180, 180]°, latitude [-85.0511287798066, 85.0511287798066]°; "
    "projected X/Y [-20037508.342789244, 20037508.342789244] m; "
    "1e-6 m output roundoff only, no input clipping."
)
_QN_AREA = (
    "Trusted Quang Ninh reference fixtures only; longitude [106.88, 108.07]°, "
    "latitude [20.70, 21.42]°. Port-water control results, not an official boundary "
    "or extension of EPSG:6960 onshore scope. No survey-accuracy claim."
)
_TM3_AREA = (
    "Trusted Quang Ninh anchorage reference fixtures only; longitude "
    "[107.15, 107.18]°, latitude [20.70, 20.73]°. Product control rectangle, "
    "not an official boundary, extension of EPSG:6960 onshore scope, "
    "or survey-accuracy claim."
)
_WEB_EVIDENCE = "analytic-webmercator-formula"
_QN_EVIDENCE = "circular-50-2025-tt-bxd-quang-ninh"
_TM3_EVIDENCE = "mae-eia-quang-ninh-table-1-4-qh-04"

_WEB_FORWARD = PipelineStep(4326, 3857, WEB_MERCATOR_PIPELINE, False)
_WEB_INVERSE = PipelineStep(3857, 4326, WEB_MERCATOR_PIPELINE, True)
_DATUM_FORWARD = PipelineStep(4756, 4326, EPSG_6960_PIPELINE, False)
_DATUM_INVERSE = PipelineStep(4326, 4756, EPSG_6960_PIPELINE, True)
_TM3_FORWARD = PipelineStep(4756, 5899, TM3_PIPELINE, False)
_TM3_INVERSE = PipelineStep(5899, 4756, TM3_PIPELINE, True)

# A static allowlist, not a graph search. The two composite steps are the only
# approved concatenations. Identity reads are deliberately not transform edges.
TRANSFORMATION_PROFILES: tuple[TransformationProfile, ...] = (
    TransformationProfile(
        4326,
        3857,
        "epsg-1024-forward",
        "analytic Pseudo-Mercator",
        _WEB_AREA,
        0.001,
        "metre",
        _WEB_EVIDENCE,
        WEB_MERCATOR_GEOGRAPHIC_BOUNDS,
        False,
        (_WEB_FORWARD,),
    ),
    TransformationProfile(
        3857,
        4326,
        "epsg-1024-inverse",
        "analytic Pseudo-Mercator inverse",
        _WEB_AREA,
        1e-8,
        "degree",
        _WEB_EVIDENCE,
        WEB_MERCATOR_GEOGRAPHIC_BOUNDS,
        False,
        (_WEB_INVERSE,),
    ),
    TransformationProfile(
        4756,
        4326,
        "epsg-6960-forward",
        "VN-2000 to WGS 84 (2)",
        _QN_AREA,
        3.0,
        "metre",
        _QN_EVIDENCE,
        QUANG_NINH_GEOGRAPHIC_BOUNDS,
        True,
        (_DATUM_FORWARD,),
    ),
    TransformationProfile(
        4326,
        4756,
        "epsg-6960-inverse",
        "inverse EPSG:6960",
        _QN_AREA,
        3.0,
        "metre",
        _QN_EVIDENCE,
        QUANG_NINH_GEOGRAPHIC_BOUNDS,
        True,
        (_DATUM_INVERSE,),
    ),
    TransformationProfile(
        5899,
        4756,
        "epsg-5899-inverse-tm3",
        "inverse native EPSG TM-3 107-45 projection",
        _TM3_AREA,
        1.0,
        "metre",
        _TM3_EVIDENCE,
        QUANG_NINH_TM3_BOUNDS,
        True,
        (_TM3_INVERSE,),
    ),
    TransformationProfile(
        4756,
        5899,
        "epsg-5899-forward-tm3",
        "native EPSG TM-3 107-45 projection",
        _TM3_AREA,
        1.0,
        "metre",
        _TM3_EVIDENCE,
        QUANG_NINH_TM3_BOUNDS,
        True,
        (_TM3_FORWARD,),
    ),
    TransformationProfile(
        5899,
        4326,
        "epsg-5899-to-4326-via-6960",
        "explicit inverse TM-3 then EPSG:6960 approved as one registry edge",
        _TM3_AREA,
        1.0,
        "metre",
        _TM3_EVIDENCE,
        QUANG_NINH_TM3_BOUNDS,
        True,
        (_TM3_INVERSE, _DATUM_FORWARD),
    ),
    TransformationProfile(
        4326,
        5899,
        "epsg-4326-to-5899-via-6960",
        "explicit inverse EPSG:6960 then TM-3 approved as one registry edge",
        _TM3_AREA,
        1.0,
        "metre",
        _TM3_EVIDENCE,
        QUANG_NINH_TM3_BOUNDS,
        True,
        (_DATUM_INVERSE, _TM3_FORWARD),
    ),
)
_TRANSFORM_BY_EDGE = MappingProxyType(
    {(edge.source_code, edge.target_code): edge for edge in TRANSFORMATION_PROFILES}
)


def get_transformation_profile(source_code: int, target_code: int) -> TransformationProfile | None:
    """Return only a direct approved edge; never infer identities or transit."""
    if type(source_code) is not int or type(target_code) is not int:
        return None
    return _TRANSFORM_BY_EDGE.get((source_code, target_code))
