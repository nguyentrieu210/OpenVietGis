"""Trusted R003 reference definitions and scoped, pinned candidate rows.

Registry entries are immutable synthetic/control definitions, not shared tenant
records, external connections, or caller-supplied geometry. Materialization
prepares NOT_READY rows only. The native database verification in T020 must
inspect the persisted rows before the registration transaction can claim ready
or a specific independently expected validation failure.
"""

from __future__ import annotations

import json
import math
from collections.abc import Callable, Iterator, Mapping, Sequence
from dataclasses import asdict, dataclass, replace
from datetime import UTC, datetime, timedelta
from enum import StrEnum
from types import MappingProxyType
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.exc import SQLAlchemyError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.rls import require_project_context
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.spatial.domain import (
    MAX_POSITIONS,
    READABLE_CRS_CODES,
    SUPPORTED_FAMILIES,
    WEB_MERCATOR_MAX,
    AttributeValue,
    CoordinateTree,
    DataSource,
    Extent,
    ExtentState,
    FeatureCountState,
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    GeometryState,
    Lifecycle,
    NativeGeometry,
    ReferenceFeature,
    SchemaState,
    SourceField,
    SourceKind,
    SpatialValidationError,
    ValidationReason,
    ValidationReasonCode,
    ValidationState,
    canonical_attribute_value,
    canonical_manifest_digest,
    validate_ready_geometry,
)
from openvietgis_api.domains.spatial.ports import SpatialScopeError
from openvietgis_api.domains.spatial.service import (
    PinnedSource,
    derive_source_pins,
    verify_source_pins,
)

FIXTURE_VERSION = "r003-reference-v1"
_PROVISIONAL_PIN = "0" * 64
_EPOCH = datetime(2025, 1, 1, tzinfo=UTC)

type XY = tuple[float, float]
type Bounds = tuple[float, float, float, float]
type DMS = tuple[int, int, int]  # degrees, minutes, hundredths of an arc-second


class FixtureProvenance(StrEnum):
    SYNTHETIC = "synthetic"
    ANALYTIC_WEB_MERCATOR = "analytic_web_mercator"
    QUANG_NINH_CIRCULAR = "quang_ninh_circular_50_2025"
    QUANG_NINH_HVS = "quang_ninh_mae_eia_hvs"


@dataclass(frozen=True, slots=True)
class FieldDefinition:
    field_id: str
    name: str
    type: FieldType
    nullable: bool


REFERENCE_FIELDS: tuple[FieldDefinition, ...] = (
    FieldDefinition("fld_feature_code", "Feature code", FieldType.TEXT, False),
    FieldDefinition("fld_name", "Name", FieldType.TEXT, False),
    FieldDefinition("fld_rank", "Rank", FieldType.INTEGER, False),
    FieldDefinition("fld_measure", "Measure", FieldType.DECIMAL, True),
    FieldDefinition("fld_active", "Active", FieldType.BOOLEAN, False),
    FieldDefinition("fld_observed_at", "Observed at", FieldType.TIMESTAMP, True),
    FieldDefinition("fld_note", "Note", FieldType.TEXT, True),
)


@dataclass(frozen=True, slots=True)
class FixtureRecord:
    """Unscoped immutable manifest facts, never a persisted feature row."""

    feature_id: str
    attributes: tuple[tuple[str, AttributeValue], ...]
    geometry: NativeGeometry | None
    geometry_state: GeometryState


@dataclass(frozen=True, slots=True)
class ReferenceFixture:
    key: str
    declared_crs: int | None
    stored_crs: int | None
    declared_family: GeometryFamily
    declared_dimensions: GeometryDimensions
    expected_family: GeometryFamily
    expected_dimensions: GeometryDimensions
    expected_count: int
    expected_native_bounds: Bounds | None
    provenance: FixtureProvenance
    expected_reason: ValidationReasonCode | None
    _record_factory: Callable[[], Iterator[FixtureRecord]]
    # Independent known control XY, not an input axis heuristic.
    axis_control_xy: XY | None = None
    # Only the named correctable manifest has evidence for this correction.
    correctable_to_crs: int | None = None
    version: str = FIXTURE_VERSION
    fields: tuple[FieldDefinition, ...] = REFERENCE_FIELDS

    @property
    def trusted_quang_ninh(self) -> bool:
        return self.provenance in (
            FixtureProvenance.QUANG_NINH_CIRCULAR,
            FixtureProvenance.QUANG_NINH_HVS,
        )

    def records(self) -> tuple[FixtureRecord, ...]:
        records = tuple(sorted(self._record_factory(), key=lambda row: row.feature_id))
        if len(records) != self.expected_count:
            raise SpatialValidationError("reference fixture count differs from manifest")
        return records


def _canonical_cents(cents: int) -> str:
    whole, fraction = divmod(abs(cents), 100)
    sign = "-" if cents < 0 else ""
    fractional = f"{fraction:02d}".rstrip("0")
    return f"{sign}{whole}" + (f".{fractional}" if fractional else "")


def _attributes(
    sequence: int, feature_id: str, label: str
) -> tuple[tuple[str, AttributeValue], ...]:
    # All values come from the one-based sequence, never a live clock/row ID.
    cents = (sequence - 1) * 37 * (-1 if sequence % 2 == 0 else 1)
    observed = (_EPOCH + timedelta(minutes=sequence - 1)).isoformat()
    note = None if sequence % 5 == 0 else "" if sequence % 5 == 1 else f"note-{sequence:06d}"
    return (
        ("fld_feature_code", feature_id),
        ("fld_name", label),
        ("fld_rank", sequence % 1000),
        ("fld_measure", None if sequence % 7 == 0 else _canonical_cents(cents)),
        ("fld_active", sequence % 2 == 0),
        ("fld_observed_at", None if sequence % 11 == 0 else observed.removesuffix("+00:00") + "Z"),
        ("fld_note", note),
    )


def _record(
    feature_id: str,
    geometry: NativeGeometry | None,
    *,
    sequence: int = 1,
    label: str | None = None,
    state: GeometryState = GeometryState.PRESENT,
) -> FixtureRecord:
    return FixtureRecord(
        feature_id,
        _attributes(sequence, feature_id, label or f"Reference {feature_id}"),
        geometry,
        state,
    )


def _microdegrees(x: int, y: int) -> XY:
    return x / 1_000_000, y / 1_000_000


def _primary_records(
    family: GeometryFamily, prefix: str, label: str, count: int
) -> Iterator[FixtureRecord]:
    # Deterministic Vietnam test grid using integer microdegrees, not repeated
    # floating point addition or projected data generated by the tested engine.
    for sequence in range(1, count + 1):
        row, column = divmod(sequence - 1, 100)
        x = 106_600_000 + 1_000 * column
        y = 10_700_000 + 1_000 * row
        if family == GeometryFamily.POINT:
            geometry = NativeGeometry(family.value, _microdegrees(x, y), 4326)
        elif family == GeometryFamily.LINE_STRING:
            geometry = NativeGeometry(
                family.value,
                (
                    _microdegrees(x, y),
                    _microdegrees(x + 300, y + 200),
                    _microdegrees(x + 600, y + 100),
                ),
                4326,
            )
        elif family == GeometryFamily.POLYGON:
            geometry = NativeGeometry(
                family.value,
                (
                    (
                        _microdegrees(x, y),
                        _microdegrees(x + 600, y),
                        _microdegrees(x + 600, y + 400),
                        _microdegrees(x, y + 400),
                        _microdegrees(x, y),
                    ),
                ),
                4326,
            )
        else:
            raise SpatialValidationError("unexpected primary reference geometry family")
        yield _record(
            f"{prefix}-{sequence:06d}",
            geometry,
            sequence=sequence,
            label=f"{label} {sequence:06d}",
        )


def _primary(
    key: str,
    family: GeometryFamily,
    prefix: str,
    label: str,
    count: int,
    bounds: Bounds,
) -> ReferenceFixture:
    return ReferenceFixture(
        key=key,
        declared_crs=4326,
        stored_crs=4326,
        declared_family=family,
        declared_dimensions=GeometryDimensions.XY,
        expected_family=family,
        expected_dimensions=GeometryDimensions.XY,
        expected_count=count,
        expected_native_bounds=bounds,
        provenance=FixtureProvenance.SYNTHETIC,
        expected_reason=None,
        _record_factory=lambda: _primary_records(family, prefix, label, count),
    )


def _fixed(
    key: str,
    records: tuple[FixtureRecord, ...],
    *,
    declared_crs: int | None = 4326,
    stored_crs: int | None = 4326,
    declared_family: GeometryFamily = GeometryFamily.POINT,
    declared_dimensions: GeometryDimensions = GeometryDimensions.XY,
    expected_family: GeometryFamily = GeometryFamily.POINT,
    expected_dimensions: GeometryDimensions = GeometryDimensions.XY,
    expected_native_bounds: Bounds | None = None,
    provenance: FixtureProvenance = FixtureProvenance.SYNTHETIC,
    expected_reason: ValidationReasonCode | None = None,
    axis_control_xy: XY | None = None,
    correctable_to_crs: int | None = None,
) -> ReferenceFixture:
    return ReferenceFixture(
        key=key,
        declared_crs=declared_crs,
        stored_crs=stored_crs,
        declared_family=declared_family,
        declared_dimensions=declared_dimensions,
        expected_family=expected_family,
        expected_dimensions=expected_dimensions,
        expected_count=len(records),
        expected_native_bounds=expected_native_bounds,
        provenance=provenance,
        expected_reason=expected_reason,
        _record_factory=lambda: iter(records),
        axis_control_xy=axis_control_xy,
        correctable_to_crs=correctable_to_crs,
    )


def _point(xy: XY, srid: int | None = 4326) -> NativeGeometry:
    return NativeGeometry("Point", xy, srid)


def _point_fixture(
    key: str, srid: int, controls: tuple[tuple[str, XY], ...], provenance: FixtureProvenance
) -> ReferenceFixture:
    records = tuple(
        _record(feature_id, _point(xy, srid), sequence=sequence)
        for sequence, (feature_id, xy) in enumerate(controls, start=1)
    )
    return _fixed(
        key,
        records,
        declared_crs=srid,
        stored_crs=srid,
        expected_native_bounds=(
            min(xy[0] for _, xy in controls),
            min(xy[1] for _, xy in controls),
            max(xy[0] for _, xy in controls),
            max(xy[1] for _, xy in controls),
        ),
        provenance=provenance,
    )


def _dms(value: DMS) -> float:
    degrees, minutes, hundredths = value
    return (degrees * 360_000 + minutes * 6_000 + hundredths) / 360_000


def _lonlat(lon: DMS, lat: DMS) -> XY:
    return _dms(lon), _dms(lat)


# Published independent coordinates, not results of PostGIS/PROJ.
# Circular 50/2025/TT-BXD appendix: five VN-2000/WGS84 pairs.
# These source fields are longitude first in the application, even though the
# source document displays latitude first; sec values below are hundredths.
_CIRCULAR: tuple[tuple[str, DMS, DMS, DMS, DMS], ...] = (
    ("VH1", (108, 1, 1000), (21, 24, 4000), (108, 1, 1680), (21, 24, 3640)),
    ("VH2", (108, 3, 3530), (21, 24, 5210), (108, 3, 4210), (21, 24, 4850)),
    ("VH10", (107, 56, 4600), (21, 14, 1050), (107, 56, 5280), (21, 14, 690)),
    ("CT1", (107, 45, 4080), (20, 58, 1430), (107, 45, 4760), (20, 58, 1070)),
    ("QY1", (106, 52, 5920), (20, 50, 780), (106, 53, 600), (20, 50, 420)),
)
# MAE EIA (July 2025), printed p.45 Table 1.4 and QH-04 drawing:
# X=northing, Y=easting in source; stored API XY = (easting, northing).
_HVS: tuple[tuple[str, XY, DMS, DMS, DMS, DMS], ...] = (
    (
        "HVS-QN1",
        (438442.69, 2291039.83),
        (107, 9, 3244),
        (20, 42, 4209),
        (107, 9, 3919),
        (20, 42, 3848),
    ),
    (
        "HVS-QN2",
        (437957.55, 2291878.33),
        (107, 9, 1556),
        (20, 43, 930),
        (107, 9, 2231),
        (20, 43, 569),
    ),
    (
        "HVS-QN3",
        (438891.96, 2292185.83),
        (107, 9, 4782),
        (20, 43, 1941),
        (107, 9, 5457),
        (20, 43, 1580),
    ),
    (
        "HVS-QN4",
        (439365.92, 2291378.72),
        (107, 10, 430),
        (20, 42, 5322),
        (107, 10, 1105),
        (20, 42, 4961),
    ),
)
_HVS_WGS = tuple((name, _lonlat(wlon, wlat)) for name, _, _, _, wlon, wlat in _HVS)
_HVS_QN1_WGS = _HVS_WGS[0][1]
_GOOD_POINT = _point((106.6, 10.7))
_GOOD_LINE = NativeGeometry("LineString", ((106.6, 10.7), (106.601, 10.701)), 4326)

# For this fixture-only registry, negative expected reasons are evidence
# predicates used by T020, not a claim that the database has verified the rows.
_FIXTURES: tuple[ReferenceFixture, ...] = (
    _primary(
        "reference_points_4326",
        GeometryFamily.POINT,
        "point",
        "Point",
        4000,
        (106.6, 10.7, 106.699, 10.739),
    ),
    _primary(
        "reference_lines_4326",
        GeometryFamily.LINE_STRING,
        "line",
        "Line",
        3000,
        (106.6, 10.7, 106.6996, 10.7292),
    ),
    _primary(
        "reference_polygons_4326",
        GeometryFamily.POLYGON,
        "polygon",
        "Polygon",
        3000,
        (106.6, 10.7, 106.6996, 10.7294),
    ),
    # Independent Pseudo-Mercator formula evaluated at WGS controls
    # (106.6,10.7), (106.6996,10.7294), (107.157,20.715).
    _point_fixture(
        "reference_points_3857",
        3857,
        (
            ("merc-000001", (11866657.718562961, 1198103.040560261)),
            ("merc-000002", (11877745.139845973, 1201433.906852360)),
            ("merc-000003", (11928662.674934816, 2357927.646019978)),
        ),
        FixtureProvenance.ANALYTIC_WEB_MERCATOR,
    ),
    _point_fixture(
        "reference_points_4756",
        4756,
        tuple((name, _lonlat(vlon, vlat)) for name, vlon, vlat, _, _ in _CIRCULAR),
        FixtureProvenance.QUANG_NINH_CIRCULAR,
    ),
    _point_fixture(
        "reference_points_4326_quang_ninh",
        4326,
        tuple((name, _lonlat(wlon, wlat)) for name, _, _, wlon, wlat in _CIRCULAR),
        FixtureProvenance.QUANG_NINH_CIRCULAR,
    ),
    _point_fixture(
        "reference_points_4756_hvs",
        4756,
        tuple((name, _lonlat(vlon, vlat)) for name, _, vlon, vlat, _, _ in _HVS),
        FixtureProvenance.QUANG_NINH_HVS,
    ),
    _point_fixture(
        "reference_points_4326_hvs",
        4326,
        _HVS_WGS,
        FixtureProvenance.QUANG_NINH_HVS,
    ),
    _point_fixture(
        "reference_points_5899",
        5899,
        tuple((name, xy) for name, xy, _, _, _, _ in _HVS),
        FixtureProvenance.QUANG_NINH_HVS,
    ),
    # Valid WGS84 native point, negative for 4326 -> 3857 (not invalid source).
    _point_fixture(
        "reference_points_4326_webmerc_out_of_area",
        4326,
        (("point-000001", (0.0, 86.0)),),
        FixtureProvenance.SYNTHETIC,
    ),
    _fixed("reference_empty_4326", ()),
    _fixed(
        "reference_unknown_crs",
        (_record("point-000001", _point((106.6, 10.7), None)),),
        declared_crs=None,
        stored_crs=None,
        expected_reason=ValidationReasonCode.MISSING_CRS,
    ),
    _fixed(
        "reference_conflicting_metadata",
        (_record("line-000001", _GOOD_LINE),),
        expected_family=GeometryFamily.LINE_STRING,
        expected_reason=ValidationReasonCode.GEOMETRY_FAMILY_MISMATCH,
    ),
    _fixed(
        "reference_unsupported_dimensions",
        (
            _record(
                "point-000001",
                NativeGeometry("Point", (106.6, 10.7, 5.0), 4326, GeometryDimensions.XYZ),
                state=GeometryState.UNSUPPORTED,
            ),
        ),
        declared_dimensions=GeometryDimensions.XYZ,
        expected_dimensions=GeometryDimensions.XYZ,
        expected_reason=ValidationReasonCode.UNSUPPORTED_DIMENSIONS,
    ),
    _fixed(
        "reference_unsupported_dimensions_m",
        (
            _record(
                "point-000001",
                NativeGeometry("Point", (106.6, 10.7, 7.0), 4326, GeometryDimensions.XYM),
                state=GeometryState.UNSUPPORTED,
            ),
        ),
        declared_dimensions=GeometryDimensions.XYM,
        expected_dimensions=GeometryDimensions.XYM,
        expected_reason=ValidationReasonCode.UNSUPPORTED_DIMENSIONS,
    ),
    _fixed(
        "reference_mixed_geometry",
        (_record("line-000001", _GOOD_LINE), _record("point-000001", _GOOD_POINT, sequence=2)),
        declared_family=GeometryFamily.MIXED,
        expected_family=GeometryFamily.MIXED,
        expected_reason=ValidationReasonCode.UNSUPPORTED_GEOMETRY,
    ),
    _fixed(
        "reference_unsupported_family",
        (
            _record(
                "multipoint-000001",
                NativeGeometry("MultiPoint", ((106.6, 10.7), (106.601, 10.701)), 4326),
                state=GeometryState.UNSUPPORTED,
            ),
        ),
        declared_family=GeometryFamily.UNSUPPORTED,
        expected_family=GeometryFamily.UNSUPPORTED,
        expected_reason=ValidationReasonCode.UNSUPPORTED_GEOMETRY,
    ),
    _fixed(
        "reference_null_geometry",
        (_record("point-000001", None, state=GeometryState.ABSENT),),
        stored_crs=None,
        expected_family=GeometryFamily.UNKNOWN,
        expected_dimensions=GeometryDimensions.UNKNOWN,
        expected_reason=ValidationReasonCode.NULL_GEOMETRY,
    ),
    _fixed(
        "reference_empty_geometry",
        (_record("point-000001", NativeGeometry("Point", (), 4326), state=GeometryState.EMPTY),),
        expected_reason=ValidationReasonCode.EMPTY_GEOMETRY,
    ),
    _fixed(
        "reference_invalid_geometry",
        (
            _record(
                "polygon-000001",
                NativeGeometry(
                    "Polygon",
                    (
                        (
                            (106.6, 10.7),
                            (106.601, 10.701),
                            (106.6, 10.701),
                            (106.601, 10.7),
                            (106.6, 10.7),
                        ),
                    ),
                    4326,
                ),
                state=GeometryState.INVALID,
            ),
        ),
        declared_family=GeometryFamily.POLYGON,
        expected_family=GeometryFamily.POLYGON,
        expected_reason=ValidationReasonCode.INVALID_GEOMETRY,
    ),
    _fixed(
        "reference_axis_conflict",
        (
            _record(
                "HVS-QN1", _point((_HVS_QN1_WGS[1], _HVS_QN1_WGS[0])), state=GeometryState.INVALID
            ),
        ),
        provenance=FixtureProvenance.QUANG_NINH_HVS,
        expected_reason=ValidationReasonCode.AXIS_CONFLICT,
        axis_control_xy=_HVS_QN1_WGS,
    ),
    _fixed(
        "reference_out_of_domain",
        (_record("point-000001", _point((106.6, 91.0)), state=GeometryState.INVALID),),
        expected_reason=ValidationReasonCode.COORDINATE_OUT_OF_DOMAIN,
    ),
    _fixed(
        "reference_definition_only_crs",
        (_record("point-000001", _point((500000.0, 1200000.0), 3405)),),
        declared_crs=3405,
        stored_crs=3405,
        expected_reason=ValidationReasonCode.UNSUPPORTED_CRS,
    ),
    _fixed(
        "reference_wrapping_extent",
        (
            _record(
                "line-000001", NativeGeometry("LineString", ((179.9, 10.0), (-179.9, 10.0)), 4326)
            ),
        ),
        declared_family=GeometryFamily.LINE_STRING,
        expected_family=GeometryFamily.LINE_STRING,
        expected_reason=ValidationReasonCode.EXTENT_WRAP_UNSUPPORTED,
    ),
    _fixed(
        "reference_correctable_crs",
        (_record("HVS-QN1", _point(_HVS_QN1_WGS)),),
        declared_crs=3857,
        stored_crs=4326,
        expected_native_bounds=(*_HVS_QN1_WGS, *_HVS_QN1_WGS),
        provenance=FixtureProvenance.QUANG_NINH_HVS,
        expected_reason=ValidationReasonCode.CRS_MISMATCH,
        axis_control_xy=_HVS_QN1_WGS,
        correctable_to_crs=4326,
    ),
)

REFERENCE_FIXTURES: Mapping[str, ReferenceFixture] = MappingProxyType(
    {fixture.key: fixture for fixture in _FIXTURES}
)


def get_reference_fixture(key: str) -> ReferenceFixture | None:
    """Exact allowlisted logical alias; never interpret it as a storage locator."""
    if not isinstance(key, str):
        return None
    return REFERENCE_FIXTURES.get(key)


def materialize_reference_fixture(
    key: str,
    *,
    organization_id: UUID,
    project_id: UUID,
    source_id: UUID,
    name: str,
    now: datetime,
) -> PinnedSource:
    """Bind new candidate rows to the server-selected scope and source identity.

    The caller authorizes the project, allocates the source UUID and performs
    insertion/actual native verification/audit in one transaction. No SQL
    execution, external input or commit happens here; metadata is provisional.
    """
    fixture = get_reference_fixture(key)
    if fixture is None:
        raise SpatialValidationError("unknown reference fixture")
    fields = tuple(
        SourceField(
            organization_id,
            project_id,
            source_id,
            field.field_id,
            field.name,
            ordinal,
            field.type,
            field.nullable,
            _PROVISIONAL_PIN,
        )
        for ordinal, field in enumerate(fixture.fields)
    )
    field_by_id = {field.field_id: field for field in fields}
    features: list[ReferenceFeature] = []
    for record in fixture.records():
        values = dict(record.attributes)
        if len(values) != len(record.attributes) or set(values) != set(field_by_id):
            raise SpatialValidationError("reference fixture attributes differ from schema")
        attributes = {
            field_id: canonical_attribute_value(field_by_id[field_id], value)
            for field_id, value in values.items()
        }
        features.append(
            ReferenceFeature(
                organization_id,
                project_id,
                source_id,
                record.feature_id,
                attributes,
                record.geometry,
                record.geometry_state,
                _PROVISIONAL_PIN,
                _PROVISIONAL_PIN,
            )
        )
    source = DataSource(
        id=source_id,
        organization_id=organization_id,
        project_id=project_id,
        name=name,
        kind=SourceKind.REFERENCE_FIXTURE,
        reference_key=fixture.key,
        fixture_version=fixture.version,
        manifest_digest=_PROVISIONAL_PIN,
        source_asset_id=None,
        lifecycle=Lifecycle.ACTIVE,
        validation_state=ValidationState.NOT_READY,
        validation_reason=ValidationReason(
            ValidationReasonCode.FIXTURE_UNAVAILABLE,
            "Reference fixture awaits native verification.",
        ),
        source_crs=fixture.declared_crs,
        stored_crs=fixture.stored_crs,
        schema_state=SchemaState.KNOWN,
        geometry_family=fixture.expected_family,
        dimensions=fixture.expected_dimensions,
        feature_count_state=FeatureCountState.KNOWN,
        feature_count=len(features),
        extent_state=ExtentState.UNAVAILABLE,
        extent=None,
        catalog_revision=1,
        content_revision=_PROVISIONAL_PIN,
        schema_revision=_PROVISIONAL_PIN,
        created_at=now,
        updated_at=now,
        retired_at=None,
    )
    pinned = derive_source_pins(source, fields, features)
    # Include canonical geometry/attribute/schema pins rather than merely a
    # generator label. The immutable original manifest also pins declaration,
    # provenance, independent expected metadata and correction/axis evidence.
    manifest_digest = canonical_manifest_digest(
        {
            "manifest_type": "r003-reference-fixture-v1",
            "key": fixture.key,
            "version": fixture.version,
            "declared_crs": fixture.declared_crs,
            "stored_crs": fixture.stored_crs,
            "declared_family": fixture.declared_family.value,
            "declared_dimensions": fixture.declared_dimensions.value,
            "expected_family": fixture.expected_family.value,
            "expected_dimensions": fixture.expected_dimensions.value,
            "expected_count": fixture.expected_count,
            "expected_native_bounds": fixture.expected_native_bounds,
            "provenance": fixture.provenance.value,
            "expected_reason": fixture.expected_reason.value if fixture.expected_reason else None,
            "axis_control_xy": fixture.axis_control_xy,
            "correctable_to_crs": fixture.correctable_to_crs,
            "schema_revision": pinned.source.schema_revision,
            "content_revision": pinned.source.content_revision,
        }
    )
    return PinnedSource(
        replace(pinned.source, manifest_digest=manifest_digest), pinned.fields, pinned.features
    )


class ReferenceFixtureIntegrityError(SpatialValidationError):
    """Unexpected registry or storage discrepancy; registration must roll back."""


class ReferenceFixtureQueryError(ReferenceFixtureIntegrityError):
    """Unexpected SQL/engine failure; never represent as unavailable evidence."""


class ReferenceCorrectionUnverifiedError(SpatialValidationError):
    """A proposed declaration is not supported by the immutable control evidence."""


def _integrity() -> ReferenceFixtureIntegrityError:
    return ReferenceFixtureIntegrityError("Reference fixture native verification failed.")


def _unverified() -> ReferenceCorrectionUnverifiedError:
    return ReferenceCorrectionUnverifiedError("Reference CRS correction could not be verified.")


def _scope_params(
    session: Session, context: AuthorizationContext, source: DataSource
) -> dict[str, UUID]:
    require_project_context(session, context, permission=Permission.PROJECT_EDIT)
    if (source.organization_id, source.project_id) != (context.organization_id, context.project_id):
        raise SpatialScopeError("reference fixture scope mismatch")
    return {
        "org": source.organization_id,
        "project": source.project_id,
        "source": source.id,
    }


def _checked_manifest(fixture: ReferenceFixture, pinned: PinnedSource) -> PinnedSource:
    """Prove actual pins against the original trusted binding, not caller fields."""
    source = pinned.source
    if get_reference_fixture(fixture.key) is not fixture:
        raise _integrity()
    original = materialize_reference_fixture(
        fixture.key,
        organization_id=source.organization_id,
        project_id=source.project_id,
        source_id=source.id,
        name=source.name,
        now=source.created_at,
    )
    if (
        replace(
            source,
            validation_state=original.source.validation_state,
            validation_reason=original.source.validation_reason,
            extent_state=original.source.extent_state,
            extent=original.source.extent,
            catalog_revision=1,
            updated_at=source.created_at,
        )
        != original.source
        or pinned.fields != original.fields
        or pinned.features != original.features
    ):
        raise _integrity()
    try:
        verify_source_pins(source, pinned.fields, pinned.features)
    except SpatialValidationError:
        raise _integrity() from None
    return original


def native_geometry_parameters(geometry: NativeGeometry | None) -> tuple[str | None, int | None]:
    """Exact SQL insertion transport for the finite trusted fixture vocabulary."""
    if geometry is None:
        return None, None
    if (
        geometry.dimensions
        not in (
            GeometryDimensions.XY,
            GeometryDimensions.XYZ,
            GeometryDimensions.XYM,
            GeometryDimensions.XYZM,
        )
        or geometry.members
        or geometry.type
        not in (
            "Point",
            "LineString",
            "Polygon",
            "MultiPoint",
        )
    ):
        raise _integrity()
    try:
        return geometry.to_wkt(), geometry.srid
    except SpatialValidationError:
        raise _integrity() from None


def _native_input(pinned: PinnedSource) -> str:
    return json.dumps(
        [
            {
                "feature_id": row.feature_id,
                "attributes": dict(row.attributes),
                "geometry_state": row.geometry_state.value,
                "content_revision": row.content_revision,
                "schema_revision": row.schema_revision,
                "wkt": wkt,
                "srid": srid,
            }
            for row in pinned.features
            for wkt, srid in (native_geometry_parameters(row.geometry),)
        ],
        allow_nan=False,
    )


# The geometry construction is exactly the insertion transport, without any
# geography cast, coordinate conversion, repair, guessed native CRS or transform.
_EXPECTED_SQL = """
    WITH expected AS (
        SELECT r.feature_id, r.attributes, r.geometry_state,
               r.content_revision, r.schema_revision,
               CASE WHEN r.wkt IS NULL THEN NULL
                    ELSE ST_GeomFromText(r.wkt, COALESCE(r.srid, 0)) END AS geometry
        FROM jsonb_to_recordset(CAST(:records AS jsonb)) AS r(
            feature_id text, attributes jsonb, geometry_state text,
            content_revision text, schema_revision text, wkt text, srid integer
        )
    )
"""

# PostgreSQL float8 arrays retain actual binary64 ordinates (no GeoJSON/WKT
# round-trip rounding). ST_Zmflag, unlike ST_NDims alone, distinguishes M and Z.
# EWKB comparison also preserves component/ring grouping and raw native SRID.
_OBSERVE_SQL = """
    SELECT a.feature_id, a.attributes, a.geometry_state, a.content_revision, a.schema_revision,
           a.geometry IS NULL AS absent, ST_IsEmpty(a.geometry) AS empty,
           ST_GeometryType(a.geometry) AS family, ST_SRID(a.geometry) AS srid,
           ST_Zmflag(a.geometry) AS zm, ST_NDims(a.geometry) AS ndims,
           ST_IsValid(a.geometry, 0) AS valid, ST_NPoints(a.geometry) AS point_count,
           ST_AsEWKB(a.geometry) AS native_ewkb,
           (
               SELECT array_agg(
                   ARRAY[ST_X(dp.geom), ST_Y(dp.geom), ST_Z(dp.geom), ST_M(dp.geom)]
                   ORDER BY dp.path
               )
               FROM ST_DumpPoints(a.geometry) AS dp
           ) AS positions
"""

_CANDIDATE_SQL = (
    _EXPECTED_SQL
    + _OBSERVE_SQL
    + """
    FROM expected AS a
    ORDER BY a.feature_id COLLATE "C"
"""
)
# Avoid a join between an unanalysed freshly populated table and a JSON
# recordset (estimated at 100 rows), which can choose a quadratic nested loop.
# Read both ordered streams in the same transaction; compare exact native EWKB
# and feature IDs, including NULL and empty geometries, before validation.
_EXPECTED_EWKB_SQL = (
    _EXPECTED_SQL
    + """
    SELECT feature_id, ST_AsEWKB(geometry) AS native_ewkb
    FROM expected ORDER BY feature_id COLLATE "C"
"""
)
_PERSISTED_SQL = (
    _OBSERVE_SQL
    + """
    FROM spatial.reference_features AS a
    WHERE a.organization_id=:org AND a.project_id=:project AND a.source_id=:source
    ORDER BY a.feature_id COLLATE "C"
"""
)


def _observe(
    session: Session, pinned: PinnedSource, scope: Mapping[str, UUID] | None = None
) -> Sequence[RowMapping]:
    try:
        records = {"records": _native_input(pinned)}
        if scope is None:
            return session.execute(text(_CANDIDATE_SQL), records).mappings().all()
        expected = session.execute(text(_EXPECTED_EWKB_SQL), records).all()
        actual = session.execute(text(_PERSISTED_SQL), dict(scope)).mappings().all()
        if len(actual) != len(expected) or any(
            (row["feature_id"], row["native_ewkb"]) != tuple(original)
            for row, original in zip(actual, expected, strict=True)
        ):
            raise _integrity()
        return actual
    except SQLAlchemyError:
        # Unexpected engine/parser/SQL faults are not an accepted negative
        # fixture. Caller owns transaction rollback; never expose raw SQL.
        raise ReferenceFixtureQueryError("Reference native verification unavailable.") from None


def _verify_persisted_metadata(
    session: Session, pinned: PinnedSource, scope: Mapping[str, UUID]
) -> None:
    """Read actual current metadata, fields and pins, under the verified RLS scope."""
    try:
        source = (
            session.execute(
                text("""
                SELECT * FROM spatial.data_sources
                WHERE organization_id=:org AND project_id=:project AND id=:source
            """),
                scope,
            )
            .mappings()
            .one_or_none()
        )
        fields = (
            session.execute(
                text("""
                SELECT * FROM spatial.source_fields
                WHERE organization_id=:org AND project_id=:project AND source_id=:source
                ORDER BY ordinal
            """),
                scope,
            )
            .mappings()
            .all()
        )
    except SQLAlchemyError:
        raise ReferenceFixtureQueryError("Reference native verification unavailable.") from None
    if (
        source is None
        or dict(source) != asdict(pinned.source)
        or [dict(field) for field in fields] != [asdict(field) for field in pinned.fields]
    ):
        raise _integrity()


def _flatten(tree: CoordinateTree) -> Iterator[tuple[float, ...]]:
    if not tree:
        return
    if isinstance(tree[0], float):
        if not all(isinstance(value, float) for value in tree):
            raise _integrity()
        yield tuple(value for value in tree if isinstance(value, float))
        return
    for item in tree:
        if not isinstance(item, tuple):
            raise _integrity()
        yield from _flatten(item)


def _raw_positions(geometry: NativeGeometry) -> tuple[tuple[float | None, ...], ...]:
    result: list[tuple[float | None, ...]] = []
    for position in _flatten(geometry.coordinates):
        match geometry.dimensions:
            case GeometryDimensions.XY:
                if len(position) != 2:
                    raise _integrity()
                result.append((*position, None, None))
            case GeometryDimensions.XYZ:
                if len(position) != 3:
                    raise _integrity()
                result.append((*position, None))
            case GeometryDimensions.XYM:
                if len(position) != 3:
                    raise _integrity()
                result.append((position[0], position[1], None, position[2]))
            case GeometryDimensions.XYZM:
                if len(position) != 4:
                    raise _integrity()
                result.append(position)
            case _:
                raise _integrity()
    if geometry.members:
        raise _integrity()  # No nested collection exists in the trusted fixture manifest.
    return tuple(result)


_ZM_DIMENSIONS = {
    0: GeometryDimensions.XY,
    1: GeometryDimensions.XYM,
    2: GeometryDimensions.XYZ,
    3: GeometryDimensions.XYZM,
}


@dataclass(frozen=True, slots=True)
class _NativeObservation:
    geometry: NativeGeometry | None
    family: GeometryFamily
    dimensions: GeometryDimensions
    srid: int | None
    positions: tuple[XY, ...]
    empty: bool
    valid: bool
    state: GeometryState


def _coordinate_out_of_domain(x: float, y: float, srid: int | None) -> bool:
    if not (math.isfinite(x) and math.isfinite(y)):
        return True
    if srid in (4326, 4756):
        return not (-180 <= x <= 180 and -90 <= y <= 90)
    if srid == 3857:
        return abs(x) > WEB_MERCATOR_MAX or abs(y) > WEB_MERCATOR_MAX
    return False


def _verified_observations(
    pinned: PinnedSource, rows: Sequence[RowMapping]
) -> tuple[_NativeObservation, ...]:
    if len(rows) != len(pinned.features):
        raise _integrity()
    result: list[_NativeObservation] = []
    field_by_id = {field.field_id: field for field in pinned.fields}
    for feature, row in zip(pinned.features, rows, strict=True):
        attrs = row["attributes"]
        if (
            row["feature_id"] != feature.feature_id
            or row["geometry_state"] != feature.geometry_state.value
            or row["content_revision"] != feature.content_revision
            or row["schema_revision"] != feature.schema_revision
            or not isinstance(attrs, dict)
            or set(attrs) != set(field_by_id)
        ):
            raise _integrity()
        for key, value in attrs.items():
            try:
                canonical = canonical_attribute_value(field_by_id[key], value)
            except SpatialValidationError:
                raise _integrity() from None
            expected = feature.attributes[key]
            if type(canonical) is not type(value) or type(expected) is not type(value):
                raise _integrity()
            if canonical != value or expected != value:
                raise _integrity()
        geometry = feature.geometry
        if geometry is None:
            if (
                row["absent"] is not True
                or any(
                    row[key] is not None
                    for key in (
                        "family",
                        "srid",
                        "zm",
                        "ndims",
                        "empty",
                        "valid",
                        "point_count",
                        "positions",
                    )
                )
                or feature.geometry_state != GeometryState.ABSENT
            ):
                raise _integrity()
            result.append(
                _NativeObservation(
                    None,
                    GeometryFamily.UNKNOWN,
                    GeometryDimensions.UNKNOWN,
                    None,
                    (),
                    False,
                    False,
                    GeometryState.ABSENT,
                )
            )
            continue
        raw = tuple(tuple(position) for position in row["positions"] or ())
        dims = _ZM_DIMENSIONS.get(row["zm"])
        srid = row["srid"] or None
        if (
            row["absent"] is not False
            or row["family"] != "ST_" + geometry.type
            or dims is None
            or dims != geometry.dimensions
            or srid != geometry.srid
            or row["ndims"]
            != {
                GeometryDimensions.XY: 2,
                GeometryDimensions.XYZ: 3,
                GeometryDimensions.XYM: 3,
                GeometryDimensions.XYZM: 4,
            }[dims]
            or row["empty"] != geometry.is_empty
            or row["point_count"] != len(raw)
            or raw != _raw_positions(geometry)
            or type(row["valid"]) is not bool
        ):
            raise _integrity()
        family = (
            GeometryFamily(geometry.type)
            if geometry.type in SUPPORTED_FAMILIES
            else GeometryFamily.UNSUPPORTED
        )
        xy = tuple((point[0], point[1]) for point in raw)
        if row["empty"]:
            state = GeometryState.EMPTY
        elif family not in SUPPORTED_FAMILIES or dims != GeometryDimensions.XY:
            state = GeometryState.UNSUPPORTED
        elif not row["valid"] or any(_coordinate_out_of_domain(x, y, srid) for x, y in xy):
            state = GeometryState.INVALID
        else:
            state = GeometryState.PRESENT
        if state != feature.geometry_state:
            raise _integrity()
        result.append(
            _NativeObservation(geometry, family, dims, srid, xy, row["empty"], row["valid"], state)
        )
    return tuple(result)


def _wrapping(geometry: NativeGeometry) -> bool:
    if geometry.srid not in (4326, 4756) or geometry.type == "Point":
        return False
    parts = (geometry.coordinates,) if geometry.type == "LineString" else geometry.coordinates
    for part in parts:
        if not isinstance(part, tuple):
            raise _integrity()
        positions = tuple(_flatten(part))
        if any(abs(a[0] - b[0]) > 180 for a, b in zip(positions, positions[1:], strict=False)):
            return True
    return False


def _validation_reason(
    fixture: ReferenceFixture,
    observations: Sequence[_NativeObservation],
    family: GeometryFamily,
    dimensions: GeometryDimensions,
    stored_crs: int | None,
    declared_crs: int | None,
) -> ValidationReasonCode | None:
    code = ValidationReasonCode
    if any(row.geometry is None for row in observations):
        return code.NULL_GEOMETRY
    if any(row.empty for row in observations):
        return code.EMPTY_GEOMETRY
    if family not in SUPPORTED_FAMILIES:
        return code.UNSUPPORTED_GEOMETRY
    if dimensions != GeometryDimensions.XY:
        return code.UNSUPPORTED_DIMENSIONS
    if stored_crs is None or declared_crs is None:
        return code.MISSING_CRS
    if stored_crs not in READABLE_CRS_CODES or declared_crs not in READABLE_CRS_CODES:
        return code.UNSUPPORTED_CRS
    if stored_crs != declared_crs:
        return code.CRS_MISMATCH
    if family != fixture.declared_family:
        return code.GEOMETRY_FAMILY_MISMATCH
    if any(not row.valid for row in observations):
        return code.INVALID_GEOMETRY
    if fixture.axis_control_xy is not None:
        # Only independently published known point controls justify an axis
        # conclusion; no swapping or inference for merely plausible numbers.
        if len(observations) != 1 or observations[0].positions != (fixture.axis_control_xy,):
            return code.AXIS_CONFLICT
    if any(
        _coordinate_out_of_domain(x, y, stored_crs)
        for row in observations
        for x, y in row.positions
    ):
        return code.COORDINATE_OUT_OF_DOMAIN
    if any(row.geometry is not None and _wrapping(row.geometry) for row in observations):
        return code.EXTENT_WRAP_UNSUPPORTED
    if any(len(row.positions) > MAX_POSITIONS for row in observations):
        return code.BOUNDS_EXCEEDED
    if stored_crs == 5899 and fixture.provenance != FixtureProvenance.QUANG_NINH_HVS:
        return code.UNSUPPORTED_CRS
    # Domain shape and finite-domain invariants augment native topology checks.
    try:
        for row in observations:
            assert row.geometry is not None
            validate_ready_geometry(row.geometry, family, stored_crs)
    except SpatialValidationError:
        return code.INVALID_GEOMETRY
    return None


_REASON_MESSAGES: Mapping[ValidationReasonCode, str] = MappingProxyType(
    {
        ValidationReasonCode.NULL_GEOMETRY: "The reference contains absent geometry.",
        ValidationReasonCode.EMPTY_GEOMETRY: "The reference contains an empty geometry.",
        ValidationReasonCode.MISSING_CRS: "Native CRS metadata is missing.",
        ValidationReasonCode.UNSUPPORTED_CRS: "The native CRS is not readable in this release.",
        ValidationReasonCode.CRS_MISMATCH: "Declared CRS differs from verified native CRS.",
        ValidationReasonCode.UNSUPPORTED_GEOMETRY: "The inspected geometry family is unsupported.",
        ValidationReasonCode.UNSUPPORTED_DIMENSIONS: "Only XY geometry is readable.",
        ValidationReasonCode.GEOMETRY_FAMILY_MISMATCH: "Declared and inspected families differ.",
        ValidationReasonCode.INVALID_GEOMETRY: "Native geometry failed topology validation.",
        ValidationReasonCode.AXIS_CONFLICT: "Native axes conflict with the independent control.",
        ValidationReasonCode.COORDINATE_OUT_OF_DOMAIN: "Native coordinates outside CRS domain.",
        ValidationReasonCode.EXTENT_WRAP_UNSUPPORTED: "Wrapped geographic extents are unsupported.",
        ValidationReasonCode.BOUNDS_EXCEEDED: "Native geometry exceeds the position limit.",
    }
)


def _verified_metadata(
    fixture: ReferenceFixture,
    source: DataSource,
    observations: Sequence[_NativeObservation],
    *,
    corrected: bool = False,
) -> DataSource:
    nonnull = [row for row in observations if row.geometry is not None]
    if not observations:
        # Zero rows are independently expected by this declared schema, not an
        # unknown empty geometry/SRID. The manifest is already verified.
        family, dimensions = fixture.declared_family, fixture.declared_dimensions
        stored_crs = fixture.stored_crs
    elif not nonnull:
        family, dimensions, stored_crs = GeometryFamily.UNKNOWN, GeometryDimensions.UNKNOWN, None
    else:
        families = {row.family for row in nonnull}
        dims = {row.dimensions for row in nonnull}
        srids = {row.srid for row in nonnull}
        family = next(iter(families)) if len(families) == 1 else GeometryFamily.MIXED
        dimensions = next(iter(dims)) if len(dims) == 1 else GeometryDimensions.MIXED
        if len(srids) != 1:
            raise _integrity()
        stored_crs = next(iter(srids))
    if (
        len(observations) != fixture.expected_count
        or family != fixture.expected_family
        or dimensions != fixture.expected_dimensions
        or stored_crs != fixture.stored_crs
    ):
        raise _integrity()
    reason = _validation_reason(
        fixture, observations, family, dimensions, stored_crs, source.source_crs
    )
    if reason != (None if corrected else fixture.expected_reason):
        # A fixture expectation is an independent check, never the classifier.
        raise _unverified() if corrected else _integrity()
    positions = tuple(point for row in observations for point in row.positions)
    bounds: Bounds | None = None
    if positions:
        xs, ys = zip(*positions, strict=True)
        bounds = (min(xs), min(ys), max(xs), max(ys))
    if fixture.expected_native_bounds is not None and bounds != fixture.expected_native_bounds:
        raise _integrity()
    extent: Extent | None = None
    extent_state = ExtentState.UNAVAILABLE
    if reason is None:
        if bounds is not None:
            if stored_crs is None:
                raise _integrity()
            extent = Extent(*bounds, stored_crs)
            extent_state = ExtentState.AVAILABLE
        elif len(observations) == 0:
            extent_state = ExtentState.EMPTY
        else:
            raise _integrity()
    return replace(
        source,
        schema_state=SchemaState.KNOWN,
        geometry_family=family,
        dimensions=dimensions,
        stored_crs=stored_crs,
        feature_count_state=FeatureCountState.KNOWN,
        feature_count=len(observations),
        validation_state=(
            ValidationState.READY
            if reason is None
            else ValidationState.NOT_READY
            if reason == ValidationReasonCode.MISSING_CRS
            else ValidationState.INVALID
        ),
        validation_reason=(
            ValidationReason(reason, _REASON_MESSAGES[reason]) if reason is not None else None
        ),
        extent_state=extent_state,
        extent=extent,
    )


def _verify_native(
    session: Session,
    fixture: ReferenceFixture,
    pinned: PinnedSource,
    scope: Mapping[str, UUID] | None,
) -> tuple[PinnedSource, tuple[_NativeObservation, ...]]:
    observations = _verified_observations(pinned, _observe(session, pinned, scope))
    result = PinnedSource(
        _verified_metadata(fixture, pinned.source, observations), pinned.fields, pinned.features
    )
    try:
        verify_source_pins(result.source, result.fields, result.features)
    except SpatialValidationError:
        raise _integrity() from None
    return result, observations


def validate_reference_candidate(
    session: Session, fixture: ReferenceFixture, pinned: PinnedSource
) -> PinnedSource:
    """Inspect actual native SQL values before insertion; not yet publication."""
    original = _checked_manifest(fixture, pinned)
    if pinned != original:
        raise _integrity()
    return _verify_native(session, fixture, pinned, None)[0]


def validate_persisted_reference_source(
    session: Session,
    fixture: ReferenceFixture,
    pinned: PinnedSource,
    *,
    context: AuthorizationContext,
) -> PinnedSource:
    """Reverify actual scoped storage in the same outer registration transaction.

    Supports provisional or already verified initial source metadata, but never
    commits or publishes anything. Any discrepancy fails closed.
    """
    scope = _scope_params(session, context, pinned.source)
    original = _checked_manifest(fixture, pinned)
    _verify_persisted_metadata(session, pinned, scope)
    verified, _ = _verify_native(session, fixture, pinned, scope)
    if pinned.source not in (original.source, verified.source):
        raise _integrity()
    return verified


def validate_reference_correction(
    session: Session,
    source: DataSource,
    *,
    context: AuthorizationContext,
    declared_crs: int,
    now: datetime,
) -> DataSource:
    """Return corrected interpretation only after exact actual persisted proof.

    The caller owns the source lock and CAS, all writes and audit in the same
    transaction. Native EWKB, attributes, metadata schema and manifest identity
    remain unchanged; no SQL UPDATE or geometry conversion occurs here.
    """
    scope = _scope_params(session, context, source)
    fixture = get_reference_fixture(source.reference_key)
    if (
        fixture is None
        or fixture.correctable_to_crs is None
        or declared_crs != fixture.correctable_to_crs
        or type(declared_crs) is not int
        or fixture.axis_control_xy is None
        or source.lifecycle != Lifecycle.ACTIVE
        or source.validation_state != ValidationState.INVALID
        or source.validation_reason is None
        or source.validation_reason.code != ValidationReasonCode.CRS_MISMATCH
        or source.source_crs == declared_crs
    ):
        raise _unverified()
    original = materialize_reference_fixture(
        fixture.key,
        organization_id=source.organization_id,
        project_id=source.project_id,
        source_id=source.id,
        name=source.name,
        now=source.created_at,
    )
    actual = PinnedSource(source, original.fields, original.features)
    try:
        _checked_manifest(fixture, actual)
        _verify_persisted_metadata(session, actual, scope)
        verified, observations = _verify_native(session, fixture, actual, scope)
        if verified.source != source:
            raise _integrity()
    except ReferenceFixtureQueryError:
        raise
    except ReferenceFixtureIntegrityError:
        raise _unverified() from None
    if source.stored_crs != declared_crs or len(observations) != 1:
        raise _unverified()
    proposal = replace(source, source_crs=declared_crs)
    result = _verified_metadata(fixture, proposal, observations, corrected=True)
    corrected = derive_source_pins(
        replace(result, catalog_revision=source.catalog_revision + 1, updated_at=now),
        actual.fields,
        actual.features,
    )
    if (
        corrected.source.validation_state != ValidationState.READY
        or corrected.source.content_revision == source.content_revision
        or corrected.source.schema_revision != source.schema_revision
        or corrected.source.manifest_digest != source.manifest_digest
    ):
        raise _unverified()
    return corrected.source


def derive_test_fixture_refresh(
    pinned: PinnedSource, *, variant: str, now: datetime
) -> PinnedSource:
    """Finite test-only mutation; never accepts arbitrary geometry or attributes.

    Only the bounded four-record 5899 control fixture is permitted. Coordinates,
    stable IDs and field interpretation remain immutable; each variant changes
    exactly one revision domain. The adapter enforces the test environment.
    """
    source = pinned.source
    if (
        source.reference_key != "reference_points_5899"
        or not source.readable
        or len(pinned.features) != 4
        or variant not in ("content", "schema")
    ):
        raise SpatialValidationError("unsupported controlled fixture refresh")
    verify_source_pins(source, pinned.fields, pinned.features)
    original = materialize_reference_fixture(
        source.reference_key,
        organization_id=source.organization_id,
        project_id=source.project_id,
        source_id=source.id,
        name=source.name,
        now=source.created_at,
    )
    if (
        source.manifest_digest,
        source.fixture_version,
        source.source_crs,
        source.stored_crs,
    ) != (
        original.source.manifest_digest,
        original.source.fixture_version,
        original.source.source_crs,
        original.source.stored_crs,
    ):
        raise SpatialValidationError("unexpected controlled fixture binding")
    # Allow only the original trusted manifest or a prior deterministic test
    # variant. Preserve all geometry, IDs and other attribute values.
    if [row.feature_id for row in pinned.features] != [row.feature_id for row in original.features]:
        raise SpatialValidationError("unexpected controlled fixture identities")
    first = pinned.features[0]
    original_first = original.features[0]
    old_name = first.attributes["fld_name"]
    original_name = original_first.attributes["fld_name"]
    alternate_name = "Controlled test refresh"
    if old_name not in (original_name, alternate_name):
        raise SpatialValidationError("unexpected controlled fixture attribute")
    for index, (actual, trusted) in enumerate(zip(pinned.features, original.features, strict=True)):
        attrs = dict(actual.attributes)
        if index == 0:
            attrs["fld_name"] = original_name
        if (
            attrs != dict(trusted.attributes)
            or actual.geometry != trusted.geometry
            or actual.geometry_state != trusted.geometry_state
        ):
            raise SpatialValidationError("unexpected controlled fixture content")
    fields = list(pinned.fields)
    features = list(pinned.features)
    alternate_field = "Name (controlled test refresh)"
    if len(fields) != len(original.fields):
        raise SpatialValidationError("unexpected controlled fixture schema")
    for index, (actual_field, trusted_field) in enumerate(
        zip(fields, original.fields, strict=True)
    ):
        if actual_field.field_id == "fld_name":
            if actual_field.name not in (trusted_field.name, alternate_field):
                raise SpatialValidationError("unexpected controlled fixture field name")
            if variant == "schema":
                name = (
                    alternate_field
                    if actual_field.name == trusted_field.name
                    else trusted_field.name
                )
                fields[index] = replace(actual_field, name=name)
        elif actual_field.name != trusted_field.name:
            raise SpatialValidationError("unexpected controlled fixture field name")
        if (
            replace(
                actual_field, name=trusted_field.name, schema_revision=trusted_field.schema_revision
            )
            != trusted_field
        ):
            raise SpatialValidationError("unexpected controlled fixture field interpretation")
    if variant == "content":
        new_name = alternate_name if old_name == original_name else original_name
        features[0] = replace(first, attributes=dict(first.attributes) | {"fld_name": new_name})
    return derive_source_pins(
        replace(source, catalog_revision=source.catalog_revision + 1, updated_at=now),
        fields,
        features,
    )
