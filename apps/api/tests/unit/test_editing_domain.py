"""R006 must publish logical attributes readable by the Core feature contract."""

from uuid import uuid4

import pytest

from openvietgis_api.domains.editing.domain import validate_typed_attributes
from openvietgis_api.domains.platform_errors import PlatformValidationError
from openvietgis_api.domains.spatial.domain import FieldType, SourceField


def field(kind: FieldType, *, nullable: bool = False) -> SourceField:
    return SourceField(uuid4(), uuid4(), uuid4(), "value", "Value", 0, kind, nullable, "a" * 64)


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("1000.0000", "1000"),
        (1000.0, "1000"),
        (-0.0, "0"),
        ("0.02500", "0.025"),
        (0.0000001, "0.0000001"),
        ("12345678901234567890.123456", "12345678901234567890.123456"),
    ],
)
def test_editing_decimal_canonicalizes_before_publish(raw: object, expected: str) -> None:
    assert validate_typed_attributes({"value": raw}, [field(FieldType.DECIMAL)]) == {
        "value": expected
    }


def test_editing_timestamp_is_a_canonical_utc_string() -> None:
    assert validate_typed_attributes(
        {"value": "2026-11-21T10:00:00+07:00"}, [field(FieldType.TIMESTAMP)]
    ) == {"value": "2026-11-21T03:00:00Z"}


@pytest.mark.parametrize(
    ("kind", "raw"),
    [
        (FieldType.DECIMAL, "NaN"),
        (FieldType.DECIMAL, True),
        (FieldType.DECIMAL, "1.1234567890123456789"),
        (FieldType.INTEGER, 2**31),
        (FieldType.TIMESTAMP, "not a time"),
        (FieldType.TIMESTAMP, "2026-11-21T10:00:00"),
    ],
)
def test_invalid_logical_attribute_rejected_before_commit(kind: FieldType, raw: object) -> None:
    with pytest.raises(PlatformValidationError):
        validate_typed_attributes({"value": raw}, [field(kind)])


def test_editing_required_null_and_unknown_fields_rejected() -> None:
    with pytest.raises(PlatformValidationError):
        validate_typed_attributes({}, [field(FieldType.TEXT)])
    with pytest.raises(PlatformValidationError):
        validate_typed_attributes({"secret_unknown": "value"}, [field(FieldType.TEXT)])
    assert validate_typed_attributes({}, [field(FieldType.TEXT, nullable=True)]) == {"value": None}
