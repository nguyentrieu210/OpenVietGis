"""R003 domain invariants independent of HTTP, PostGIS, and persistence."""

from dataclasses import FrozenInstanceError, replace
from datetime import UTC, datetime, timedelta, timezone
from decimal import Decimal
from uuid import UUID, uuid4

import pytest

from openvietgis_api.domains.spatial.domain import (
    CatalogChange,
    CatalogOperation,
    ChangedValue,
    DataSource,
    Extent,
    ExtentState,
    FeatureCountState,
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    GeometryState,
    Layer,
    LayerRevision,
    Lifecycle,
    NativeGeometry,
    ReferenceFeature,
    ResourceType,
    SchemaState,
    SourceField,
    SourceKind,
    SourceRevision,
    SpatialValidationError,
    ValidationReason,
    ValidationReasonCode,
    ValidationState,
    canonical_attribute_value,
    canonical_manifest_digest,
    content_revision_digest,
    next_revision,
    schema_revision_digest,
    validate_ready_geometry,
    validate_source_contents,
)
from openvietgis_api.domains.tenancy.domain import ActorType

ORG = UUID(int=1)
PROJECT = UUID(int=2)
SOURCE = UUID(int=3)
LAYER = UUID(int=4)
AT = datetime(2026, 11, 18, tzinfo=UTC)
CONTENT = "a" * 64
SCHEMA = "b" * 64


def source(**overrides: object) -> DataSource:
    base = DataSource(
        id=SOURCE,
        organization_id=ORG,
        project_id=PROJECT,
        name="Points",
        kind=SourceKind.REFERENCE_FIXTURE,
        reference_key="reference_points_4326",
        fixture_version="1",
        manifest_digest="c" * 64,
        source_asset_id=None,
        lifecycle=Lifecycle.ACTIVE,
        validation_state=ValidationState.READY,
        validation_reason=None,
        source_crs=4326,
        stored_crs=4326,
        schema_state=SchemaState.KNOWN,
        geometry_family=GeometryFamily.POINT,
        dimensions=GeometryDimensions.XY,
        feature_count_state=FeatureCountState.KNOWN,
        feature_count=1,
        extent_state=ExtentState.AVAILABLE,
        extent=Extent(106.9, 21.1, 106.9, 21.1, 4326),
        catalog_revision=1,
        content_revision=CONTENT,
        schema_revision=SCHEMA,
        created_at=AT,
        updated_at=AT,
        retired_at=None,
    )
    return replace(base, **overrides)


def field(
    kind: FieldType = FieldType.TEXT, *, name: str = "Name", nullable: bool = True
) -> SourceField:
    return SourceField(ORG, PROJECT, SOURCE, "fld_name", name, 0, kind, nullable, SCHEMA)


def feature(**overrides: object) -> ReferenceFeature:
    base = ReferenceFeature(
        organization_id=ORG,
        project_id=PROJECT,
        source_id=SOURCE,
        feature_id="point-000001",
        attributes={"fld_name": "x"},
        geometry=NativeGeometry("Point", (106.9, 21.1), 4326),
        geometry_state=GeometryState.PRESENT,
        content_revision=CONTENT,
        schema_revision=SCHEMA,
    )
    return replace(base, **overrides)


def layer(**overrides: object) -> Layer:
    return replace(
        Layer(LAYER, ORG, PROJECT, SOURCE, "Layer", Lifecycle.ACTIVE, 1, AT, AT, None),
        **overrides,
    )


@pytest.mark.parametrize(
    "overrides",
    [
        {"name": " "},
        {"reference_key": "../fixture"},
        {"fixture_version": ""},
        {"manifest_digest": "A" * 64},
        {"source_asset_id": UUID(int=5)},
        {"catalog_revision": 0},
        {"catalog_revision": 2**53},
        {"catalog_revision": True},
        {"content_revision": "not-a-pin"},
        {"validation_reason": ValidationReason(ValidationReasonCode.MISSING_CRS, "Missing")},
        {"source_crs": None},
        {"source_crs": 3857},
        {"stored_crs": 3405},
        {"geometry_family": GeometryFamily.MIXED},
        {"dimensions": GeometryDimensions.XYZ},
        {"schema_state": SchemaState.UNAVAILABLE},
        {"feature_count": -1},
        {"feature_count": True},
        {"feature_count_state": FeatureCountState.UNAVAILABLE},
        {"extent_state": ExtentState.EMPTY},
        {"extent": Extent(106.9, 21.1, 106.9, 21.1, 4756)},
        {"retired_at": AT},
        {"updated_at": datetime(2026, 11, 18)},
    ],
)
def test_invalid_source_metadata_is_rejected(overrides: dict[str, object]) -> None:
    with pytest.raises(SpatialValidationError):
        source(**overrides)


def test_source_states_and_revision_identity() -> None:
    before = source()
    assert before.revision == SourceRevision(1, CONTENT, SCHEMA)
    renamed = replace(before, name="  Renamed  ", catalog_revision=2)
    assert renamed.name == "Renamed"
    assert renamed.revision == SourceRevision(2, CONTENT, SCHEMA)
    with pytest.raises(FrozenInstanceError):
        before.name = "changed"  # type: ignore[misc]

    empty = source(feature_count=0, extent=None, extent_state=ExtentState.EMPTY)
    assert empty.feature_count == 0 and empty.extent is None
    invalid = source(
        validation_state=ValidationState.INVALID,
        validation_reason=ValidationReason(ValidationReasonCode.MISSING_CRS, "CRS missing"),
        source_crs=None,
        stored_crs=None,
        extent_state=ExtentState.UNAVAILABLE,
        extent=None,
    )
    assert invalid.validation_reason is not None
    with pytest.raises(SpatialValidationError):
        replace(invalid, validation_reason=None)
    with pytest.raises(SpatialValidationError):
        replace(invalid, extent_state=ExtentState.EMPTY)
    assert next_revision(1) == 2
    with pytest.raises(SpatialValidationError):
        next_revision(2**53 - 1)


@pytest.mark.parametrize(
    "kwargs",
    [
        {"field_id": "0bad"},
        {"field_id": "fld.unsafe"},
        {"field_id": "a" * 65},
        {"ordinal": -1},
        {"ordinal": 100},
        {"type": "string"},
        {"nullable": 1},
        {"schema_revision": "x"},
    ],
)
def test_field_identity_and_type_constraints(kwargs: dict[str, object]) -> None:
    with pytest.raises(SpatialValidationError):
        replace(field(), **kwargs)


def test_typed_attribute_values_are_exact_and_canonical() -> None:
    assert canonical_attribute_value(field(), "") == ""
    assert canonical_attribute_value(field(), None) is None
    assert canonical_attribute_value(field(FieldType.INTEGER), 0) == 0
    assert canonical_attribute_value(field(FieldType.BOOLEAN), False) is False
    assert canonical_attribute_value(field(FieldType.DECIMAL), Decimal("12.3400")) == "12.34"
    assert canonical_attribute_value(field(FieldType.DECIMAL), "-0.0") == "0"
    assert canonical_attribute_value(field(FieldType.DECIMAL), Decimal("9" * 38)) == "9" * 38
    assert canonical_attribute_value(field(FieldType.TIMESTAMP), AT) == "2026-11-18T00:00:00Z"
    local = AT.astimezone(timezone(timedelta(hours=7)))
    assert canonical_attribute_value(field(FieldType.TIMESTAMP), local) == "2026-11-18T00:00:00Z"
    for value in (
        "2026-11-18T07:30:00+07:30",
        "2026-11-17T23:30:00-00:30",
        "2026-11-18T23:59:00+23:59",
    ):
        assert (
            canonical_attribute_value(field(FieldType.TIMESTAMP), value) == "2026-11-18T00:00:00Z"
        )


@pytest.mark.parametrize(
    ("kind", "value"),
    [
        (FieldType.INTEGER, True),
        (FieldType.INTEGER, 2**31),
        (FieldType.INTEGER, "0"),
        (FieldType.BOOLEAN, 1),
        (FieldType.TEXT, 0),
        (FieldType.TEXT, "x" * 4097),
        (FieldType.DECIMAL, 1.2),
        (FieldType.DECIMAL, "1e3"),
        (FieldType.DECIMAL, "9" * 39),
        (FieldType.DECIMAL, "0." + "0" * 18 + "1"),
        (FieldType.DECIMAL, Decimal("NaN")),
        (FieldType.TIMESTAMP, "2026-01-01"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00.1234567Z"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00+01:99"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00+00:60"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00-01:99"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00+24:00"),
        (FieldType.TIMESTAMP, "2026-01-01T00:00:00-00:00"),
    ],
)
def test_type_errors_are_not_silently_coerced(kind: FieldType, value: object) -> None:
    with pytest.raises(SpatialValidationError):
        canonical_attribute_value(field(kind), value)
    with pytest.raises(SpatialValidationError):
        canonical_attribute_value(field(kind, nullable=False), None)


def test_feature_stability_and_deep_immutability() -> None:
    attrs: dict[str, str] = {"fld_name": "original"}
    item = feature(attributes=attrs)
    attrs["fld_name"] = "new"
    assert item.attributes["fld_name"] == "original"
    with pytest.raises(TypeError):
        item.attributes["fld_name"] = "mutate"  # type: ignore[index]
    with pytest.raises(SpatialValidationError):
        replace(item, feature_id="bad/name")
    with pytest.raises(SpatialValidationError):
        replace(item, geometry_state=GeometryState.ABSENT)
    assert replace(item, geometry=None, geometry_state=GeometryState.ABSENT).geometry is None
    empty = NativeGeometry("Point", (), 4326)
    assert replace(item, geometry=empty, geometry_state=GeometryState.EMPTY).geometry == empty


def test_geometry_primitives_fail_closed() -> None:
    point = NativeGeometry("Point", (106.9, 21.1), 4326)
    validate_ready_geometry(point, GeometryFamily.POINT, 4326)
    for bad in (
        NativeGeometry("Point", (106.9, 21.1), None),
        NativeGeometry("Point", (21.1, 106.9), 4326),
        NativeGeometry("Point", (106.9, 21.1, 0), 4326, GeometryDimensions.XYZ),
        NativeGeometry("LineString", ((1, 2),), 4326),
        NativeGeometry("Polygon", (((0, 0), (0, 1), (1, 1), (1, 0)),), 4326),
        NativeGeometry("LineString", tuple((0, 0) for _ in range(4097)), 4326),
        NativeGeometry("MultiPoint", ((1, 2), (3, 4)), 4326),
    ):
        with pytest.raises(SpatialValidationError):
            validate_ready_geometry(bad, GeometryFamily.POINT if bad.type == "Point" else None)
    with pytest.raises(SpatialValidationError):
        NativeGeometry("Point", (float("nan"), 1), 4326)
    with pytest.raises(SpatialValidationError):
        Extent(1, 1, 0, 1, 4326)


def test_source_content_constraints_and_pin_consistency() -> None:
    validate_source_contents(source(), (field(),), (feature(),))
    bad_features = (
        feature(project_id=uuid4()),
        feature(source_id=uuid4()),
        feature(content_revision="d" * 64),
        feature(schema_revision="d" * 64),
        feature(attributes={"fld_other": "x"}),
        feature(attributes={"fld_name": 1}),
        feature(geometry=NativeGeometry("Point", (107, 21.1), 4326)),
        feature(geometry=None, geometry_state=GeometryState.ABSENT),
    )
    for bad in bad_features:
        with pytest.raises(SpatialValidationError):
            validate_source_contents(source(), (field(),), (bad,))
    with pytest.raises(SpatialValidationError):
        validate_source_contents(source(), (field(), field()), (feature(),))
    with pytest.raises(SpatialValidationError):
        validate_source_contents(source(), (field(),), (feature(), feature()))


def test_deterministic_manifest_tokens_separate_content_and_schema() -> None:
    f1 = field()
    f2 = replace(f1, field_id="fld_second", name="Second", ordinal=1)
    item1 = feature(attributes={"fld_name": "x", "fld_second": None})
    item2 = replace(item1, feature_id="point-000002")
    schema_pin = schema_revision_digest((f1, f2), GeometryFamily.POINT, GeometryDimensions.XY)
    assert schema_pin == schema_revision_digest(
        (f2, f1), GeometryFamily.POINT, GeometryDimensions.XY
    )
    assert schema_pin != schema_revision_digest(
        (replace(f1, name="Renamed"), f2), GeometryFamily.POINT, GeometryDimensions.XY
    )
    pin = content_revision_digest(
        (item1, item2),
        (f1, f2),
        4326,
        4326,
        geometry_family=GeometryFamily.POINT,
        dimensions=GeometryDimensions.XY,
    )
    assert pin == content_revision_digest(
        (item2, item1),
        (f2, f1),
        4326,
        4326,
        geometry_family=GeometryFamily.POINT,
        dimensions=GeometryDimensions.XY,
    )
    assert pin == content_revision_digest(
        (item1, item2),
        (replace(f1, name="New"), f2),
        4326,
        4326,
        geometry_family=GeometryFamily.POINT,
        dimensions=GeometryDimensions.XY,
    )
    assert pin != content_revision_digest(
        (item1, item2),
        (f1, f2),
        4326,
        None,
        geometry_family=GeometryFamily.POINT,
        dimensions=GeometryDimensions.XY,
    )
    assert pin != content_revision_digest(
        (replace(item1, attributes={"fld_name": "new", "fld_second": None}), item2),
        (f1, f2),
        4326,
        4326,
        geometry_family=GeometryFamily.POINT,
        dimensions=GeometryDimensions.XY,
    )
    assert canonical_manifest_digest({"a": 1, "b": 2}) == canonical_manifest_digest(
        {"b": 2, "a": 1}
    )


def test_empty_schema_and_geometry_interpretations_are_revisioned() -> None:
    known = schema_revision_digest(
        (), GeometryFamily.POINT, GeometryDimensions.XY, schema_state=SchemaState.KNOWN
    )
    unavailable = schema_revision_digest(
        (), GeometryFamily.POINT, GeometryDimensions.XY, schema_state=SchemaState.UNAVAILABLE
    )
    assert known != unavailable
    with pytest.raises(SpatialValidationError):
        schema_revision_digest(
            (field(),),
            GeometryFamily.POINT,
            GeometryDimensions.XY,
            schema_state=SchemaState.UNAVAILABLE,
        )
    with pytest.raises(TypeError):
        content_revision_digest((), (), 4326, 4326)  # type: ignore[call-arg]
    point_pin = content_revision_digest(
        (), (), 4326, 4326, geometry_family=GeometryFamily.POINT, dimensions=GeometryDimensions.XY
    )
    line_pin = content_revision_digest(
        (),
        (),
        4326,
        4326,
        geometry_family=GeometryFamily.LINE_STRING,
        dimensions=GeometryDimensions.XY,
    )
    assert point_pin != line_pin


def test_layer_and_change_revision_evidence_is_safe_and_immutable() -> None:
    assert layer().revision == LayerRevision(1)
    with pytest.raises(SpatialValidationError):
        layer(layer_revision=0)
    with pytest.raises(SpatialValidationError):
        layer(lifecycle=Lifecycle.RETIRED)
    change = CatalogChange(
        id=uuid4(),
        organization_id=ORG,
        project_id=PROJECT,
        resource_type=ResourceType.LAYER,
        resource_id=LAYER,
        source_id=None,
        layer_id=LAYER,
        actor_type=ActorType.HUMAN,
        actor_id=uuid4(),
        actor_display_snapshot="GIS actor",
        operation=CatalogOperation.RENAME_LAYER,
        created_at=AT,
        audit_event_id=uuid4(),
        prior_revision=LayerRevision(1),
        new_revision=LayerRevision(2),
        changes=(ChangedValue("name", "Layer", "Renamed"),),
    )
    for overrides in (
        {"layer_id": SOURCE},
        {"source_id": SOURCE},
        {"actor_type": ActorType.SYSTEM},
        {"prior_revision": None},
        {"new_revision": LayerRevision(3)},
        {"new_revision": SourceRevision(2, CONTENT, SCHEMA)},
        {"changes": ()},
    ):
        with pytest.raises(SpatialValidationError):
            replace(change, **overrides)
    with pytest.raises(SpatialValidationError):
        ChangedValue("password", None, "secret")
    with pytest.raises(SpatialValidationError):
        ChangedValue("name", "same", "same")
    with pytest.raises(SpatialValidationError):
        ChangedValue("name", None, {"dsn": "secret"})


@pytest.mark.parametrize(
    "operation", [CatalogOperation.EDIT_FEATURES, CatalogOperation.PROCESSING_PUBLISH]
)
def test_source_content_operations_are_valid_catalog_history(operation: CatalogOperation) -> None:
    next_content = "d" * 64
    change = CatalogChange(
        id=uuid4(),
        organization_id=ORG,
        project_id=PROJECT,
        resource_type=ResourceType.DATA_SOURCE,
        resource_id=SOURCE,
        source_id=SOURCE,
        layer_id=None,
        actor_type=ActorType.HUMAN,
        actor_id=uuid4(),
        actor_display_snapshot="GIS actor",
        operation=operation,
        created_at=AT,
        audit_event_id=uuid4(),
        prior_revision=SourceRevision(1, CONTENT, SCHEMA),
        new_revision=SourceRevision(2, next_content, SCHEMA),
        changes=(ChangedValue("content_revision", CONTENT, next_content),),
    )
    assert change.operation is operation


@pytest.mark.parametrize(
    ("geometry", "expected"),
    [
        (NativeGeometry("Point", (1, 2), 4326), "POINT (1.0 2.0)"),
        (NativeGeometry("Point", (1, 2, 3), 4326, GeometryDimensions.XYM), "POINT M (1.0 2.0 3.0)"),
        (NativeGeometry("LineString", ((1, 2), (3, 4)), 4326), "LINESTRING (1.0 2.0,3.0 4.0)"),
        (
            NativeGeometry("Polygon", (((0, 0), (1, 0), (1, 1), (0, 0)),), 4326),
            "POLYGON ((0.0 0.0,1.0 0.0,1.0 1.0,0.0 0.0))",
        ),
        (NativeGeometry("MultiPoint", ((1, 2), (3, 4)), 4326), "MULTIPOINT ((1.0 2.0),(3.0 4.0))"),
        (
            NativeGeometry("MultiLineString", (((1, 2), (3, 4)),), 4326),
            "MULTILINESTRING ((1.0 2.0,3.0 4.0))",
        ),
        (
            NativeGeometry("MultiPolygon", ((((0, 0), (1, 0), (1, 1), (0, 0)),),), 4326),
            "MULTIPOLYGON (((0.0 0.0,1.0 0.0,1.0 1.0,0.0 0.0)))",
        ),
        (NativeGeometry("Point", (), 4326), "POINT EMPTY"),
    ],
)
def test_native_wkt_exact_standard_nesting(geometry: NativeGeometry, expected: str) -> None:
    assert geometry.to_wkt() == expected
