"""Coordinate-changing response contract; real numeric correctness uses PostGIS controls."""

import pytest

from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.domains.spatial.crs import get_transformation_profile
from openvietgis_api.domains.spatial.domain import NativeGeometry
from tests.contract.test_spatial_catalog import response_payload
from tests.contract.test_spatial_features import (
    BASE,
    DETAIL,
    FeatureHTTP,
    feature_http,  # noqa: F401
    url,
)


@pytest.fixture
def harness(request: pytest.FixtureRequest) -> FeatureHTTP:
    return request.getfixturevalue("feature_http")


def test_transform_contract_preserves_native_attributes_pins_and_selected_identity(
    harness: FeatureHTTP,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    layer, source, pins = harness.seed("reference_points_4326")
    original = harness.feature(layer, "point-000001", **pins)
    calls = []

    def transform(self, *, context, source, features, target_code):
        calls.append((source.id, target_code, [row.feature_id for row in features]))
        return tuple(NativeGeometry("Point", (123.0, 456.0), 3857) for _ in features)

    monkeypatch.setattr(SqlSpatialRepository, "transform_features", transform, raising=False)
    query = {**pins, "response_crs": 3857}
    page = response_payload(
        harness.client.get(url(layer_id=layer), params={**query, "limit": 1}),
        "GET",
        BASE,
        200,
    )
    detail = response_payload(
        harness.client.get(url(DETAIL, layer_id=layer, feature_id="point-000001"), params=query),
        "GET",
        DETAIL,
        200,
    )
    assert page["items"] == [detail["feature"]]
    assert page["context"] == detail["context"]
    assert page["context"]["content_revision"] == source.content_revision
    assert page["context"]["schema_revision"] == source.schema_revision
    operation = get_transformation_profile(4326, 3857)
    assert operation is not None
    assert detail["context"]["transformation"] == operation.to_wire()
    assert detail["context"]["response_crs"]["code"] == 3857
    assert detail["feature"]["geometry"]["coordinates"] == [123.0, 456.0]
    assert detail["feature"]["attributes"] == original["feature"]["attributes"]
    assert detail["feature"]["feature_id"] == original["feature"]["feature_id"]
    assert len(calls) == 2
    # Reading in another response CRS does not change native records or pins.
    assert harness.feature(layer, "point-000001", **pins) == original
    assert len(calls) == 2
