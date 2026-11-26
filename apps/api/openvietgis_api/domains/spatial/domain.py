"""Spatial catalog entities and deterministic, storage-independent value semantics."""

from __future__ import annotations

import hashlib
import json
import math
import re
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from enum import StrEnum
from types import MappingProxyType
from uuid import UUID

from openvietgis_api.domains.spatial.crs import CRS_PROFILES, WEB_MERCATOR_MAX_COORDINATE_METRE
from openvietgis_api.domains.tenancy.domain import ActorType

MAX_REVISION = 2**53 - 1
MAX_FEATURE_COUNT = 2**31 - 1
MAX_FIELDS = 100
MAX_POSITIONS = 4096
MAX_TEXT_LENGTH = 4096
KNOWN_CRS_CODES = frozenset(profile.code for profile in CRS_PROFILES)
READABLE_CRS_CODES = frozenset(
    profile.code for profile in CRS_PROFILES if profile.disposition == "readable"
)
WEB_MERCATOR_MAX = WEB_MERCATOR_MAX_COORDINATE_METRE

_FIELD_ID = re.compile(r"[A-Za-z][A-Za-z0-9_]{0,63}", re.ASCII)
_FEATURE_ID = re.compile(r"[A-Za-z0-9_-]{1,128}", re.ASCII)
_REFERENCE_KEY = re.compile(r"[a-z0-9_]{1,100}", re.ASCII)
_REVISION_TOKEN = re.compile(r"[a-f0-9]{64}", re.ASCII)
_DECIMAL = re.compile(r"-?[0-9]+(?:\.[0-9]+)?", re.ASCII)
_TIMESTAMP = re.compile(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}"
    r"(?:\.[0-9]{1,6})?(?:Z|[+-](?:[01][0-9]|2[0-3]):[0-5][0-9])",
    re.ASCII,
)


class SpatialValidationError(ValueError):
    """Invalid domain value; messages do not echo untrusted data."""


class SourceKind(StrEnum):
    REFERENCE_FIXTURE = "reference_fixture"
    IMPORTED_FILE = "imported_file"
    DERIVED_PROCESSING = "derived_processing"
    MANUAL = "manual"


class Lifecycle(StrEnum):
    ACTIVE = "active"
    RETIRED = "retired"


class ValidationState(StrEnum):
    READY = "ready"
    INVALID = "invalid"
    NOT_READY = "not_ready"


class ValidationReasonCode(StrEnum):
    MISSING_CRS = "MISSING_CRS"
    UNKNOWN_CRS = "UNKNOWN_CRS"
    UNSUPPORTED_CRS = "UNSUPPORTED_CRS"
    CRS_MISMATCH = "CRS_MISMATCH"
    GEOMETRY_FAMILY_MISMATCH = "GEOMETRY_FAMILY_MISMATCH"
    UNSUPPORTED_DIMENSIONS = "UNSUPPORTED_DIMENSIONS"
    UNSUPPORTED_GEOMETRY = "UNSUPPORTED_GEOMETRY"
    NULL_GEOMETRY = "NULL_GEOMETRY"
    EMPTY_GEOMETRY = "EMPTY_GEOMETRY"
    INVALID_GEOMETRY = "INVALID_GEOMETRY"
    AXIS_CONFLICT = "AXIS_CONFLICT"
    COORDINATE_OUT_OF_DOMAIN = "COORDINATE_OUT_OF_DOMAIN"
    EXTENT_WRAP_UNSUPPORTED = "EXTENT_WRAP_UNSUPPORTED"
    SCHEMA_MISMATCH = "SCHEMA_MISMATCH"
    FIXTURE_UNAVAILABLE = "FIXTURE_UNAVAILABLE"
    BOUNDS_EXCEEDED = "BOUNDS_EXCEEDED"


class SchemaState(StrEnum):
    KNOWN = "known"
    UNAVAILABLE = "unavailable"


class GeometryFamily(StrEnum):
    POINT = "Point"
    LINE_STRING = "LineString"
    POLYGON = "Polygon"
    MIXED = "mixed"
    UNSUPPORTED = "unsupported"
    UNKNOWN = "unknown"


class GeometryDimensions(StrEnum):
    XY = "XY"
    XYZ = "XYZ"
    XYM = "XYM"
    XYZM = "XYZM"
    MIXED = "mixed"
    UNKNOWN = "unknown"


class GeometryState(StrEnum):
    PRESENT = "present"
    ABSENT = "absent"
    EMPTY = "empty"
    INVALID = "invalid"
    UNSUPPORTED = "unsupported"


class FieldType(StrEnum):
    TEXT = "text"
    INTEGER = "integer"
    DECIMAL = "decimal"
    BOOLEAN = "boolean"
    TIMESTAMP = "timestamp"


class FeatureCountState(StrEnum):
    KNOWN = "known"
    UNAVAILABLE = "unavailable"


class ExtentState(StrEnum):
    AVAILABLE = "available"
    EMPTY = "empty"
    UNAVAILABLE = "unavailable"


class ResourceType(StrEnum):
    DATA_SOURCE = "data_source"
    LAYER = "layer"


class CatalogOperation(StrEnum):
    REGISTER_SOURCE = "register_source"
    IMPORT_SOURCE = "import_source"
    EDIT_FEATURES = "edit_features"
    PROCESSING_PUBLISH = "processing_publish"
    UPDATE_SOURCE = "update_source"
    UPDATE_SCHEMA = "update_schema"
    CORRECT_SOURCE_CRS = "correct_source_crs"
    RETIRE_SOURCE = "retire_source"
    CREATE_LAYER = "create_layer"
    RENAME_LAYER = "rename_layer"
    RETIRE_LAYER = "retire_layer"
    REFRESH_REFERENCE_FIXTURE = "refresh_reference_fixture"


class CapabilityName(StrEnum):
    METADATA_READ = "metadata_read"
    FEATURE_READ = "feature_read"
    RENDER = "render"
    EDIT = "edit"
    STYLE = "style"
    EXPORT = "export"
    ANALYSIS = "analysis"
    OFFLINE = "offline"
    QUERY = "query"


SUPPORTED_FAMILIES = frozenset(
    {GeometryFamily.POINT, GeometryFamily.LINE_STRING, GeometryFamily.POLYGON}
)


def _uuid(value: object) -> None:
    if not isinstance(value, UUID):
        raise SpatialValidationError("UUID identity required")


def _scope(organization_id: UUID, project_id: UUID) -> None:
    _uuid(organization_id)
    _uuid(project_id)


def _enum(value: object, enum_type: type[StrEnum]) -> None:
    if not isinstance(value, enum_type):
        raise SpatialValidationError("invalid domain enum")


def _text(value: object, maximum: int, *, trim: bool = False, minimum: int = 1) -> str:
    if not isinstance(value, str):
        raise SpatialValidationError("text value required")
    result = value.strip() if trim else value
    if not minimum <= len(result) <= maximum or "\x00" in result:
        raise SpatialValidationError("text length or encoding outside bounds")
    try:
        result.encode("utf-8")
    except UnicodeError:
        raise SpatialValidationError("invalid text encoding") from None
    return result


def _pattern(value: object, pattern: re.Pattern[str], reason: str) -> None:
    if not isinstance(value, str) or pattern.fullmatch(value) is None:
        raise SpatialValidationError(reason)


def _integer(value: object, minimum: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not minimum <= value <= maximum:
        raise SpatialValidationError("integer outside bounds")
    return value


def _boolean(value: object) -> bool:
    if not isinstance(value, bool):
        raise SpatialValidationError("boolean value required")
    return value


def _finite(value: object) -> float:
    if isinstance(value, bool) or not isinstance(value, int | float):
        raise SpatialValidationError("finite coordinate required")
    try:
        result = float(value)
    except (OverflowError, ValueError):
        raise SpatialValidationError("finite coordinate required") from None
    if not math.isfinite(result):
        raise SpatialValidationError("finite coordinate required")
    # Normalize negative zero only; never round or reorder coordinates.
    return 0.0 if result == 0 else result


def _utc(value: object) -> datetime:
    if not isinstance(value, datetime) or value.tzinfo is None or value.utcoffset() is None:
        raise SpatialValidationError("timezone-aware timestamp required")
    try:
        return value.astimezone(UTC)
    except (OverflowError, ValueError):
        raise SpatialValidationError("timestamp outside bounds") from None


def _lifecycle_times(
    lifecycle: Lifecycle, created: datetime, updated: datetime, retired: datetime | None
) -> tuple[datetime, datetime, datetime | None]:
    _enum(lifecycle, Lifecycle)
    created = _utc(created)
    updated = _utc(updated)
    if updated < created or (lifecycle == Lifecycle.ACTIVE) != (retired is None):
        raise SpatialValidationError("inconsistent lifecycle timestamps")
    if retired is not None:
        retired = _utc(retired)
        if not created <= retired <= updated:
            raise SpatialValidationError("inconsistent retirement timestamp")
    return created, updated, retired


def _revision(value: object) -> int:
    return _integer(value, 1, MAX_REVISION)


def validate_revision_token(value: object) -> None:
    _pattern(value, _REVISION_TOKEN, "64 lowercase hex revision token required")


def next_revision(current: int) -> int:
    return _revision(_revision(current) + 1)


def _known_crs(value: int | None) -> None:
    if value is not None and (
        isinstance(value, bool) or not isinstance(value, int) or value not in KNOWN_CRS_CODES
    ):
        raise SpatialValidationError("unknown CRS profile must not be claimed as known")


def _readable_crs(value: int | None) -> None:
    _known_crs(value)
    if value not in READABLE_CRS_CODES:
        raise SpatialValidationError("readable CRS required")


@dataclass(frozen=True, slots=True)
class ValidationReason:
    code: ValidationReasonCode
    message: str

    def __post_init__(self) -> None:
        _enum(self.code, ValidationReasonCode)
        object.__setattr__(self, "message", _text(self.message, 500, trim=True))

    def to_wire(self) -> dict[str, str]:
        return {"code": self.code.value, "message": self.message}


@dataclass(frozen=True, slots=True)
class SourceRevision:
    catalog_revision: int
    content_revision: str
    schema_revision: str

    def __post_init__(self) -> None:
        _revision(self.catalog_revision)
        validate_revision_token(self.content_revision)
        validate_revision_token(self.schema_revision)

    def to_wire(self) -> dict[str, int | str]:
        return {
            "catalog_revision": self.catalog_revision,
            "content_revision": self.content_revision,
            "schema_revision": self.schema_revision,
        }


@dataclass(frozen=True, slots=True)
class LayerRevision:
    layer_revision: int

    def __post_init__(self) -> None:
        _revision(self.layer_revision)

    def to_wire(self) -> dict[str, int]:
        return {"layer_revision": self.layer_revision}


@dataclass(frozen=True, slots=True)
class Extent:
    min_x: float
    min_y: float
    max_x: float
    max_y: float
    crs_code: int

    def __post_init__(self) -> None:
        _readable_crs(self.crs_code)
        for name in ("min_x", "min_y", "max_x", "max_y"):
            object.__setattr__(self, name, _finite(getattr(self, name)))
        if self.min_x > self.max_x or self.min_y > self.max_y:
            raise SpatialValidationError("extent minima must not exceed maxima")
        _validate_position_domain(self.min_x, self.min_y, self.crs_code)
        _validate_position_domain(self.max_x, self.max_y, self.crs_code)


@dataclass(frozen=True, slots=True)
class DataSource:
    id: UUID
    organization_id: UUID
    project_id: UUID
    name: str
    kind: SourceKind
    reference_key: str
    fixture_version: str
    manifest_digest: str
    source_asset_id: UUID | None
    lifecycle: Lifecycle
    validation_state: ValidationState
    validation_reason: ValidationReason | None
    source_crs: int | None
    stored_crs: int | None
    schema_state: SchemaState
    geometry_family: GeometryFamily
    dimensions: GeometryDimensions
    feature_count_state: FeatureCountState
    feature_count: int | None
    extent_state: ExtentState
    extent: Extent | None
    catalog_revision: int
    content_revision: str
    schema_revision: str
    created_at: datetime
    updated_at: datetime
    retired_at: datetime | None

    def __post_init__(self) -> None:
        _uuid(self.id)
        _scope(self.organization_id, self.project_id)
        object.__setattr__(self, "name", _text(self.name, 200, trim=True))
        _enum(self.kind, SourceKind)
        _pattern(self.reference_key, _REFERENCE_KEY, "invalid fixture key")
        object.__setattr__(self, "fixture_version", _text(self.fixture_version, 100))
        validate_revision_token(self.manifest_digest)
        if self.kind == SourceKind.REFERENCE_FIXTURE and self.source_asset_id is not None:
            raise SpatialValidationError("reference source cannot have a source asset")
        if self.kind == SourceKind.IMPORTED_FILE and not isinstance(self.source_asset_id, UUID):
            raise SpatialValidationError("imported source requires a source asset UUID")
        _enum(self.validation_state, ValidationState)
        _enum(self.schema_state, SchemaState)
        _enum(self.geometry_family, GeometryFamily)
        _enum(self.dimensions, GeometryDimensions)
        _enum(self.feature_count_state, FeatureCountState)
        _enum(self.extent_state, ExtentState)
        _known_crs(self.source_crs)
        _known_crs(self.stored_crs)
        SourceRevision(self.catalog_revision, self.content_revision, self.schema_revision)
        dates = _lifecycle_times(self.lifecycle, self.created_at, self.updated_at, self.retired_at)
        for name, date in zip(("created_at", "updated_at", "retired_at"), dates, strict=True):
            object.__setattr__(self, name, date)
        if self.feature_count_state == FeatureCountState.KNOWN:
            _integer(self.feature_count, 0, MAX_FEATURE_COUNT)
        elif self.feature_count is not None:
            raise SpatialValidationError("unavailable count must be null")
        if self.extent_state == ExtentState.AVAILABLE:
            if not isinstance(self.extent, Extent) or self.extent.crs_code != self.stored_crs:
                raise SpatialValidationError("available extent requires verified native CRS")
        elif self.extent is not None:
            raise SpatialValidationError("empty/unavailable extent must be null")
        if self.validation_state == ValidationState.READY:
            _readable_crs(self.source_crs)
            _readable_crs(self.stored_crs)
            if self.kind == SourceKind.REFERENCE_FIXTURE and self.source_crs != self.stored_crs:
                raise SpatialValidationError("ready declaration must match verified native CRS")
            if (
                self.validation_reason is not None
                or self.schema_state != SchemaState.KNOWN
                or self.geometry_family not in SUPPORTED_FAMILIES
                or self.dimensions != GeometryDimensions.XY
                or self.feature_count_state != FeatureCountState.KNOWN
            ):
                raise SpatialValidationError("incomplete or inconsistent ready source metadata")
            state = ExtentState.EMPTY if self.feature_count == 0 else ExtentState.AVAILABLE
            if self.extent_state != state:
                raise SpatialValidationError("ready extent state disagrees with feature count")
        elif (
            not isinstance(self.validation_reason, ValidationReason)
            or self.extent_state != ExtentState.UNAVAILABLE
        ):
            raise SpatialValidationError("non-ready source requires reason and unavailable extent")

    @property
    def revision(self) -> SourceRevision:
        return SourceRevision(self.catalog_revision, self.content_revision, self.schema_revision)

    @property
    def readable(self) -> bool:
        return self.lifecycle == Lifecycle.ACTIVE and self.validation_state == ValidationState.READY


@dataclass(frozen=True, slots=True)
class SourceField:
    organization_id: UUID
    project_id: UUID
    source_id: UUID
    field_id: str
    name: str
    ordinal: int
    type: FieldType
    nullable: bool
    schema_revision: str

    def __post_init__(self) -> None:
        _scope(self.organization_id, self.project_id)
        _uuid(self.source_id)
        _pattern(self.field_id, _FIELD_ID, "invalid stable field ID")
        object.__setattr__(self, "name", _text(self.name, 200, trim=True))
        _integer(self.ordinal, 0, MAX_FIELDS - 1)
        _enum(self.type, FieldType)
        _boolean(self.nullable)
        validate_revision_token(self.schema_revision)


type AttributeValue = str | int | bool | None
type ChangeValue = str | int | float | bool | None
type CoordinateTree = tuple[float | CoordinateTree, ...]


def _canonical_decimal(value: object) -> str:
    if isinstance(value, Decimal):
        if not value.is_finite():
            raise SpatialValidationError("finite decimal required")
        # Avoid formatting huge positive/negative exponents or applying a rounding context.
        if value.is_zero():
            return "0"
        if value.adjusted() > 37 or value.adjusted() < -18 or len(value.as_tuple().digits) > 4096:
            raise SpatialValidationError("decimal outside precision/scale bounds")
        value = format(value, "f")
    if not isinstance(value, str) or len(value) > 4096 or _DECIMAL.fullmatch(value) is None:
        raise SpatialValidationError("exact base-10 decimal required")
    negative = value.startswith("-")
    integer, _, fraction = value.removeprefix("-").partition(".")
    integer = integer.lstrip("0") or "0"
    fraction = fraction.rstrip("0")
    precision = len(integer.lstrip("0")) + len(fraction)
    if precision > 38 or len(fraction) > 18:
        raise SpatialValidationError("decimal outside precision/scale bounds")
    if integer == "0" and not fraction:
        return "0"
    sign = "-" if negative else ""
    return sign + integer + ("." + fraction if fraction else "")


def _canonical_timestamp(value: object) -> str:
    if isinstance(value, str):
        if _TIMESTAMP.fullmatch(value) is None or value.endswith("-00:00"):
            raise SpatialValidationError("RFC3339 timestamp with known timezone required")
        try:
            value = datetime.fromisoformat(value)
        except ValueError:
            raise SpatialValidationError("invalid timestamp") from None
    return _utc(value).isoformat().removesuffix("+00:00") + "Z"


def canonical_attribute_value(field: SourceField, value: object) -> AttributeValue:
    """Return the exact JSON wire representation, rejecting lossy coercions."""
    if value is None:
        if not field.nullable:
            raise SpatialValidationError("nonnullable field cannot be null")
        return None
    if field.type == FieldType.TEXT:
        return _text(value, MAX_TEXT_LENGTH, minimum=0)
    if field.type == FieldType.INTEGER:
        return _integer(value, -(2**31), 2**31 - 1)
    if field.type == FieldType.BOOLEAN:
        return _boolean(value)
    if field.type == FieldType.DECIMAL:
        return _canonical_decimal(value)
    if field.type == FieldType.TIMESTAMP:
        return _canonical_timestamp(value)
    raise SpatialValidationError("unsupported field type")


def _freeze_coordinates(value: object, depth: int = 0) -> CoordinateTree:
    if not isinstance(value, tuple | list) or depth > 5:
        raise SpatialValidationError("invalid geometry coordinate structure")
    result: list[float | CoordinateTree] = []
    for item in value:
        if isinstance(item, tuple | list):
            result.append(_freeze_coordinates(item, depth + 1))
        else:
            result.append(_finite(item))
    return tuple(result)


_NATIVE_TYPES = frozenset(
    {
        "Point",
        "LineString",
        "Polygon",
        "MultiPoint",
        "MultiLineString",
        "MultiPolygon",
        "GeometryCollection",
    }
)


@dataclass(frozen=True, slots=True)
class NativeGeometry:
    """Immutable native geometry, including inspectable unsupported/invalid fixture shapes.

    SRID None denotes unknown (PostGIS SRID 0). Readiness is separately validated;
    merely recording existing coordinates never assigns or transforms an SRID.
    """

    type: str
    coordinates: CoordinateTree
    srid: int | None
    dimensions: GeometryDimensions = GeometryDimensions.XY
    members: tuple[NativeGeometry, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.type, str) or self.type not in _NATIVE_TYPES:
            raise SpatialValidationError("unsupported native geometry representation")
        _enum(self.dimensions, GeometryDimensions)
        if self.srid is not None:
            _integer(self.srid, 1, 998999)
        object.__setattr__(self, "coordinates", _freeze_coordinates(self.coordinates))
        members = tuple(self.members)
        if any(not isinstance(member, NativeGeometry) for member in members):
            raise SpatialValidationError("invalid geometry collection member")
        if self.type == "GeometryCollection":
            if self.coordinates:
                raise SpatialValidationError("geometry collections use native members")
        elif members:
            raise SpatialValidationError("only geometry collections have members")
        object.__setattr__(self, "members", members)

    @property
    def is_empty(self) -> bool:
        return not self.coordinates and not self.members

    def to_wkt(self) -> str:
        """Serialize preserved native values for an adapter; no CRS inference."""
        marker = {
            GeometryDimensions.XYZ: " Z",
            GeometryDimensions.XYM: " M",
            GeometryDimensions.XYZM: " ZM",
        }.get(self.dimensions, "")
        head = self.type.upper() + marker
        if self.is_empty:
            return head + " EMPTY"
        if self.type == "GeometryCollection":
            return head + " (" + ",".join(member.to_wkt() for member in self.members) + ")"
        return (
            head
            + " "
            + _coordinate_wkt(self.coordinates, wrap_positions=self.type in ("Point", "MultiPoint"))
        )


def _coordinate_wkt(tree: CoordinateTree, *, wrap_positions: bool) -> str:
    if not tree:
        raise SpatialValidationError("unexpected empty coordinate component")
    if all(isinstance(item, float) for item in tree):
        position = " ".join(repr(item) for item in tree)
        return "(" + position + ")" if wrap_positions else position
    if all(isinstance(item, tuple) for item in tree):
        return (
            "("
            + ",".join(
                _coordinate_wkt(item, wrap_positions=wrap_positions)
                for item in tree
                if isinstance(item, tuple)
            )
            + ")"
        )
    raise SpatialValidationError("mixed coordinate nesting")


def _position(tree: float | CoordinateTree) -> tuple[float, float]:
    if (
        not isinstance(tree, tuple)
        or len(tree) != 2
        or not isinstance(tree[0], float)
        or not isinstance(tree[1], float)
    ):
        raise SpatialValidationError("XY position required")
    return tree[0], tree[1]


def _line(tree: float | CoordinateTree, *, ring: bool = False) -> tuple[tuple[float, float], ...]:
    if not isinstance(tree, tuple):
        raise SpatialValidationError("geometry coordinate sequence required")
    result = tuple(_position(item) for item in tree)
    if ring:
        if len(result) < 4 or result[0] != result[-1] or len(set(result[:-1])) < 3:
            raise SpatialValidationError("polygon ring must be closed and nondegenerate")
        twice_area = sum(
            x1 * y2 - x2 * y1 for (x1, y1), (x2, y2) in zip(result, result[1:], strict=False)
        )
        if not math.isfinite(twice_area) or twice_area == 0:
            raise SpatialValidationError("polygon ring must have nonzero finite area")
    elif len(result) < 2 or len(set(result)) < 2:
        raise SpatialValidationError("line requires at least two distinct positions")
    return result


def _ready_parts(geometry: NativeGeometry) -> tuple[tuple[tuple[float, float], ...], ...]:
    if geometry.dimensions != GeometryDimensions.XY or geometry.members:
        raise SpatialValidationError("only homogeneous XY geometry is readable")
    if geometry.is_empty:
        raise SpatialValidationError("empty geometry is not a readable feature")
    parts: tuple[tuple[tuple[float, float], ...], ...]
    if geometry.type == "Point":
        parts = ((_position(geometry.coordinates),),)
    elif geometry.type == "LineString":
        parts = (_line(geometry.coordinates),)
    elif geometry.type == "Polygon":
        parts = tuple(_line(ring, ring=True) for ring in geometry.coordinates)
    else:
        raise SpatialValidationError("unsupported readable geometry family")
    if sum(len(part) for part in parts) > MAX_POSITIONS:
        raise SpatialValidationError("geometry exceeds bounded position count")
    return parts


def _validate_position_domain(
    x: float, y: float, srid: int, *, output_roundoff: bool = False
) -> None:
    if srid in (4326, 4756):
        if not -180 <= x <= 180 or not -90 <= y <= 90:
            raise SpatialValidationError("geographic position outside XY domain")
    elif srid == 3857 and (
        abs(x) > WEB_MERCATOR_MAX + (1e-6 if output_roundoff else 0)
        or abs(y) > WEB_MERCATOR_MAX + (1e-6 if output_roundoff else 0)
    ):
        raise SpatialValidationError("Web Mercator position outside domain")
    # 5899 operation area/provenance are separately determined by the approved CRS pipeline.


def validate_ready_geometry(
    geometry: NativeGeometry,
    expected_family: GeometryFamily | None = None,
    expected_srid: int | None = None,
    *,
    output_roundoff: bool = False,
) -> tuple[tuple[float, float], ...]:
    """Validate shape/XY/domain/bounds; native PostGIS topology is also checked by the adapter."""
    _readable_crs(geometry.srid)
    srid = geometry.srid
    assert srid is not None
    if expected_family is not None and geometry.type != expected_family.value:
        raise SpatialValidationError("geometry family mismatch")
    if expected_srid is not None and srid != expected_srid:
        raise SpatialValidationError("native CRS mismatch")
    parts = _ready_parts(geometry)
    for part in parts:
        for x, y in part:
            _validate_position_domain(x, y, srid, output_roundoff=output_roundoff)
        if srid in (4326, 4756) and any(
            abs(previous[0] - following[0]) > 180
            for previous, following in zip(part, part[1:], strict=False)
        ):
            raise SpatialValidationError("unsupported longitude wrapping")
    return tuple(position for part in parts for position in part)


@dataclass(frozen=True, slots=True)
class ReferenceFeature:
    organization_id: UUID
    project_id: UUID
    source_id: UUID
    feature_id: str
    attributes: Mapping[str, AttributeValue]
    geometry: NativeGeometry | None
    geometry_state: GeometryState
    content_revision: str
    schema_revision: str

    def __post_init__(self) -> None:
        _scope(self.organization_id, self.project_id)
        _uuid(self.source_id)
        _pattern(self.feature_id, _FEATURE_ID, "invalid stable feature ID")
        validate_revision_token(self.content_revision)
        validate_revision_token(self.schema_revision)
        _enum(self.geometry_state, GeometryState)
        if not isinstance(self.attributes, Mapping) or len(self.attributes) > MAX_FIELDS:
            raise SpatialValidationError("invalid or unbounded logical attributes")
        attributes: dict[str, AttributeValue] = {}
        for key, value in self.attributes.items():
            _pattern(key, _FIELD_ID, "invalid stable attribute field ID")
            if isinstance(value, str):
                _text(value, MAX_TEXT_LENGTH, minimum=0)
            elif isinstance(value, bool) or value is None:
                pass
            elif isinstance(value, int):
                _integer(value, -(2**31), 2**31 - 1)
            else:
                raise SpatialValidationError("attribute value must use exact JSON representation")
            attributes[key] = value
        object.__setattr__(self, "attributes", MappingProxyType(attributes))
        if self.geometry_state == GeometryState.ABSENT:
            if self.geometry is not None:
                raise SpatialValidationError("absent geometry must be null")
            return
        if not isinstance(self.geometry, NativeGeometry):
            raise SpatialValidationError("non-absent geometry requires native value")
        if self.geometry_state == GeometryState.EMPTY:
            if not self.geometry.is_empty:
                raise SpatialValidationError("empty geometry state disagrees with native geometry")
            return
        if self.geometry.is_empty:
            raise SpatialValidationError("empty native geometry requires empty state")
        if self.geometry_state == GeometryState.PRESENT:
            _ready_parts(self.geometry)
        if (
            self.geometry_state == GeometryState.UNSUPPORTED
            and self.geometry.type in SUPPORTED_FAMILIES
            and self.geometry.dimensions == GeometryDimensions.XY
        ):
            raise SpatialValidationError("unsupported geometry state requires unsupported shape")


@dataclass(frozen=True, slots=True)
class Layer:
    id: UUID
    organization_id: UUID
    project_id: UUID
    source_id: UUID
    name: str
    lifecycle: Lifecycle
    layer_revision: int
    created_at: datetime
    updated_at: datetime
    retired_at: datetime | None

    def __post_init__(self) -> None:
        _uuid(self.id)
        _scope(self.organization_id, self.project_id)
        _uuid(self.source_id)
        object.__setattr__(self, "name", _text(self.name, 200, trim=True))
        _revision(self.layer_revision)
        dates = _lifecycle_times(self.lifecycle, self.created_at, self.updated_at, self.retired_at)
        for name, date in zip(("created_at", "updated_at", "retired_at"), dates, strict=True):
            object.__setattr__(self, name, date)

    @property
    def revision(self) -> LayerRevision:
        return LayerRevision(self.layer_revision)

    def validate_source(self, source: DataSource) -> None:
        if (source.organization_id, source.project_id, source.id) != (
            self.organization_id,
            self.project_id,
            self.source_id,
        ):
            raise SpatialValidationError("source owner/reference mismatch")


_SOURCE_CHANGE_FIELDS = frozenset(
    {
        "name",
        "kind",
        "reference_key",
        "fixture_version",
        "manifest_digest",
        "source_asset_id",
        "lifecycle",
        "validation_state",
        "validation_reason",
        "validation_reason.code",
        "validation_reason.message",
        "source_crs",
        "stored_crs",
        "schema_state",
        "geometry_family",
        "dimensions",
        "feature_count_state",
        "feature_count",
        "extent_state",
        "catalog_revision",
        "content_revision",
        "schema_revision",
    }
)
_LAYER_CHANGE_FIELDS = frozenset({"name", "source_id", "lifecycle", "layer_revision"})
_SCHEMA_FIELD_CHANGE = re.compile(
    r"fields\.[A-Za-z][A-Za-z0-9_]{0,63}\.(?:name|type|nullable|ordinal)", re.ASCII
)


def _safe_change_field(field: str, resource_type: ResourceType | None = None) -> bool:
    source_field = (
        field in _SOURCE_CHANGE_FIELDS or _SCHEMA_FIELD_CHANGE.fullmatch(field) is not None
    )
    layer_field = field in _LAYER_CHANGE_FIELDS
    if resource_type == ResourceType.DATA_SOURCE:
        return source_field
    if resource_type == ResourceType.LAYER:
        return layer_field
    return source_field or layer_field


def _safe_change_value(value: object) -> None:
    if value is None or isinstance(value, bool):
        return
    if isinstance(value, str):
        _text(value, 500, minimum=0)
    elif isinstance(value, int):
        _integer(value, -MAX_REVISION, MAX_REVISION)
    elif isinstance(value, float):
        _finite(value)
    else:
        raise SpatialValidationError("change evidence must contain only safe scalar metadata")


@dataclass(frozen=True, slots=True)
class ChangedValue:
    field: str
    old_value: ChangeValue
    new_value: ChangeValue

    def __post_init__(self) -> None:
        _text(self.field, 100)
        if not _safe_change_field(self.field):
            raise SpatialValidationError("field is not allowed in safe catalog change evidence")
        _safe_change_value(self.old_value)
        _safe_change_value(self.new_value)
        if type(self.old_value) is type(self.new_value) and self.old_value == self.new_value:
            raise SpatialValidationError("change evidence must record actual changed value")

    def to_wire(self) -> dict[str, ChangeValue]:
        return {"field": self.field, "old_value": self.old_value, "new_value": self.new_value}


_SOURCE_OPERATIONS = frozenset(
    {
        CatalogOperation.REGISTER_SOURCE,
        CatalogOperation.IMPORT_SOURCE,
        CatalogOperation.EDIT_FEATURES,
        CatalogOperation.PROCESSING_PUBLISH,
        CatalogOperation.UPDATE_SOURCE,
        CatalogOperation.UPDATE_SCHEMA,
        CatalogOperation.CORRECT_SOURCE_CRS,
        CatalogOperation.RETIRE_SOURCE,
        CatalogOperation.REFRESH_REFERENCE_FIXTURE,
    }
)


@dataclass(frozen=True, slots=True)
class CatalogChange:
    id: UUID
    organization_id: UUID
    project_id: UUID
    resource_type: ResourceType
    resource_id: UUID
    source_id: UUID | None
    layer_id: UUID | None
    actor_type: ActorType
    actor_id: UUID
    actor_display_snapshot: str
    operation: CatalogOperation
    created_at: datetime
    audit_event_id: UUID
    prior_revision: SourceRevision | LayerRevision | None
    new_revision: SourceRevision | LayerRevision
    changes: tuple[ChangedValue, ...]

    def __post_init__(self) -> None:
        _uuid(self.id)
        _scope(self.organization_id, self.project_id)
        _uuid(self.resource_id)
        _uuid(self.actor_id)
        _uuid(self.audit_event_id)
        _enum(self.resource_type, ResourceType)
        _enum(self.operation, CatalogOperation)
        _enum(self.actor_type, ActorType)
        if self.actor_type not in (ActorType.HUMAN, ActorType.SERVICE_PRINCIPAL):
            raise SpatialValidationError("catalog change requires authenticated actor")
        object.__setattr__(
            self, "actor_display_snapshot", _text(self.actor_display_snapshot, 200, trim=True)
        )
        object.__setattr__(self, "created_at", _utc(self.created_at))
        is_source = self.resource_type == ResourceType.DATA_SOURCE
        if is_source:
            if self.source_id != self.resource_id or self.layer_id is not None:
                raise SpatialValidationError("catalog change source reference mismatch")
        elif self.layer_id != self.resource_id or self.source_id is not None:
            raise SpatialValidationError("catalog change layer reference mismatch")
        if is_source != (self.operation in _SOURCE_OPERATIONS):
            raise SpatialValidationError("operation/resource type mismatch")
        create = self.operation in (
            CatalogOperation.REGISTER_SOURCE,
            CatalogOperation.IMPORT_SOURCE,
            CatalogOperation.CREATE_LAYER,
        )
        revision_type = SourceRevision if is_source else LayerRevision
        if not isinstance(self.new_revision, revision_type):
            raise SpatialValidationError("incorrect new revision domain")
        if create != (self.prior_revision is None) or (
            self.prior_revision is not None and not isinstance(self.prior_revision, revision_type)
        ):
            raise SpatialValidationError("incorrect prior revision domain")
        if isinstance(self.new_revision, SourceRevision):
            assert self.prior_revision is None or isinstance(self.prior_revision, SourceRevision)
            expected = (
                1
                if self.prior_revision is None
                else next_revision(self.prior_revision.catalog_revision)
            )
            if self.new_revision.catalog_revision != expected:
                raise SpatialValidationError("source revision must advance exactly once")
            if (
                self.prior_revision is not None
                and self.operation
                in (CatalogOperation.UPDATE_SOURCE, CatalogOperation.RETIRE_SOURCE)
                and (self.new_revision.content_revision, self.new_revision.schema_revision)
                != (self.prior_revision.content_revision, self.prior_revision.schema_revision)
            ):
                raise SpatialValidationError("metadata-only change must preserve read pins")
        else:
            assert isinstance(self.new_revision, LayerRevision)
            assert self.prior_revision is None or isinstance(self.prior_revision, LayerRevision)
            expected = (
                1
                if self.prior_revision is None
                else next_revision(self.prior_revision.layer_revision)
            )
            if self.new_revision.layer_revision != expected:
                raise SpatialValidationError("Layer revision must advance exactly once")
        changes = tuple(self.changes)
        if not 1 <= len(changes) <= 100:
            raise SpatialValidationError("change evidence must be bounded and nonempty")
        seen: set[str] = set()
        for change in changes:
            if (
                not isinstance(change, ChangedValue)
                or not _safe_change_field(change.field, self.resource_type)
                or change.field in seen
            ):
                raise SpatialValidationError("invalid or duplicate change evidence field")
            if create and change.old_value is not None:
                raise SpatialValidationError("creation change has no prior value")
            seen.add(change.field)
        object.__setattr__(self, "changes", changes)


def validate_source_contents(
    source: DataSource, fields: Sequence[SourceField], features: Sequence[ReferenceFeature]
) -> None:
    """Verify matching ownership/pins and full typed ready-source metadata.

    Invalid fixtures retain truthful metadata and bounded raw values without
    claiming readiness. The adapter separately verifies actual database geometry
    topology and independent CRS provenance/axis applicability.
    """
    scope = (source.organization_id, source.project_id, source.id)
    if len(fields) > MAX_FIELDS or len(features) > MAX_FEATURE_COUNT:
        raise SpatialValidationError("source content exceeds documented bounds")
    if source.schema_state == SchemaState.UNAVAILABLE and fields:
        raise SpatialValidationError("unavailable schema cannot expose fields")
    by_id: dict[str, SourceField] = {}
    ordinals: set[int] = set()
    for field in fields:
        if (
            (field.organization_id, field.project_id, field.source_id) != scope
            or field.schema_revision != source.schema_revision
            or field.field_id in by_id
            or field.ordinal in ordinals
        ):
            raise SpatialValidationError("field ownership, revision, or uniqueness mismatch")
        by_id[field.field_id] = field
        ordinals.add(field.ordinal)
    if source.feature_count_state == FeatureCountState.KNOWN and source.feature_count != len(
        features
    ):
        raise SpatialValidationError("known feature count mismatch")
    seen: set[str] = set()
    bounds: tuple[float, float, float, float] | None = None
    for feature in features:
        if (
            (feature.organization_id, feature.project_id, feature.source_id) != scope
            or feature.content_revision != source.content_revision
            or feature.schema_revision != source.schema_revision
            or feature.feature_id in seen
        ):
            raise SpatialValidationError("feature ownership, revision, or uniqueness mismatch")
        seen.add(feature.feature_id)
        if source.validation_state != ValidationState.READY:
            continue
        if set(feature.attributes) != set(by_id):
            raise SpatialValidationError("attribute keys do not match pinned schema")
        for key, value in feature.attributes.items():
            canonical = canonical_attribute_value(by_id[key], value)
            if type(canonical) is not type(value) or canonical != value:
                raise SpatialValidationError("noncanonical stored attribute value")
        if feature.geometry is None or feature.geometry_state != GeometryState.PRESENT:
            raise SpatialValidationError("ready features require present native geometry")
        positions = validate_ready_geometry(
            feature.geometry, source.geometry_family, source.stored_crs
        )
        for x, y in positions:
            if bounds is None:
                bounds = (x, y, x, y)
            else:
                bounds = (
                    min(bounds[0], x),
                    min(bounds[1], y),
                    max(bounds[2], x),
                    max(bounds[3], y),
                )
    if source.validation_state == ValidationState.READY and bounds is not None:
        assert source.stored_crs is not None
        if source.extent != Extent(*bounds, source.stored_crs):
            raise SpatialValidationError("reported extent does not match native feature positions")


def canonical_manifest_digest(manifest: object) -> str:
    """Hash canonical JSON (explicit key order/encoding, no lossy defaults or timestamps)."""
    try:
        payload = json.dumps(
            manifest, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False
        ).encode("ascii")
    except (TypeError, ValueError, UnicodeError, OverflowError):
        raise SpatialValidationError("manifest contains noncanonical values") from None
    return hashlib.sha256(payload).hexdigest()


def _ordered_fields(fields: Sequence[SourceField]) -> tuple[SourceField, ...]:
    if len(fields) > MAX_FIELDS:
        raise SpatialValidationError("schema bounds exceeded")
    if len({field.field_id for field in fields}) != len(fields):
        raise SpatialValidationError("duplicate stable field IDs")
    if len({field.ordinal for field in fields}) != len(fields):
        raise SpatialValidationError("duplicate field ordinals")
    return tuple(sorted(fields, key=lambda field: field.field_id))


def schema_revision_digest(
    fields: Sequence[SourceField],
    geometry_family: GeometryFamily,
    dimensions: GeometryDimensions,
    *,
    schema_state: SchemaState = SchemaState.KNOWN,
) -> str:
    _enum(geometry_family, GeometryFamily)
    _enum(dimensions, GeometryDimensions)
    _enum(schema_state, SchemaState)
    if schema_state == SchemaState.UNAVAILABLE and fields:
        raise SpatialValidationError("unavailable schema cannot expose fields")
    return canonical_manifest_digest(
        {
            "manifest_type": "r003-schema-v1",
            "schema_state": schema_state.value,
            "geometry_family": geometry_family.value,
            "dimensions": dimensions.value,
            "fields": [
                {
                    "field_id": field.field_id,
                    "name": field.name,
                    "ordinal": field.ordinal,
                    "type": field.type.value,
                    "nullable": field.nullable,
                }
                for field in _ordered_fields(fields)
            ],
        }
    )


def _geometry_manifest(geometry: NativeGeometry | None) -> object:
    if geometry is None:
        return None
    return {
        "type": geometry.type,
        "dimensions": geometry.dimensions.value,
        "srid": geometry.srid,
        "coordinates": geometry.coordinates,
        "members": [_geometry_manifest(member) for member in geometry.members],
    }


def content_revision_digest(
    features: Sequence[ReferenceFeature],
    fields: Sequence[SourceField],
    stored_crs: int | None,
    source_crs: int | None,
    *,
    geometry_family: GeometryFamily,
    dimensions: GeometryDimensions,
) -> str:
    """Content pins cover values, native geometry and schema/CRS interpretation only."""
    _known_crs(stored_crs)
    _known_crs(source_crs)
    _enum(geometry_family, GeometryFamily)
    _enum(dimensions, GeometryDimensions)
    if len({feature.feature_id for feature in features}) != len(features):
        raise SpatialValidationError("duplicate stable feature IDs")
    return canonical_manifest_digest(
        {
            "manifest_type": "r003-content-v1",
            "source_crs": source_crs,
            "stored_crs": stored_crs,
            "geometry_family": geometry_family.value,
            "dimensions": dimensions.value,
            "field_interpretations": [
                {"field_id": field.field_id, "type": field.type.value, "nullable": field.nullable}
                for field in _ordered_fields(fields)
            ],
            "features": [
                {
                    "feature_id": feature.feature_id,
                    "attributes": dict(feature.attributes),
                    "geometry": _geometry_manifest(feature.geometry),
                    "geometry_state": feature.geometry_state.value,
                }
                for feature in sorted(features, key=lambda feature: feature.feature_id)
            ],
        }
    )
