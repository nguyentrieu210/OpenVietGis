"""GeoPackage REAL/decimal values import as plain base-10 strings (no exponent notation)."""

from __future__ import annotations

import pytest

from openvietgis_api.adapters.formats.geopackage import _coerce_gpkg_value
from openvietgis_api.domains.spatial.domain import FieldType


@pytest.mark.parametrize(
    "raw,expected",
    [(20.0, "20"), (100, "100"), (12.5, "12.5"), (1e-7, "0.0000001"), ("12.50", "12.5"), (0.0, "0")],
)
def test_decimal_values_never_use_exponent_notation(raw: object, expected: str) -> None:
    assert _coerce_gpkg_value(FieldType.DECIMAL, raw) == expected
