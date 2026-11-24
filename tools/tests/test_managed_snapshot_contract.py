from dataclasses import replace

import pytest

from integrations.qgis_plugin.openvietgis_connector import managed_client as client


def fixture_page(**changes):
    page = client.ManagedBootstrapPage(
        layer_id="layer",
        source_id="source",
        name="Test",
        geometry_family="Point",
        dimensions="XY",
        crs_code=5899,
        content_revision="a" * 64,
        schema_revision="b" * 64,
        cursor=1,
        permission_epoch=2,
        fields=({"name": "label", "data_type": "text"},),
        features=(
            {
                "feature_id": "p1",
                "geometry": {"type": "Point", "coordinates": [500000, 2300000]},
            },
        ),
        has_more=True,
        next_offset=1,
    )
    return replace(page, **changes)


def read(monkeypatch, *pages):
    stream = iter(pages)
    monkeypatch.setattr(
        client, "bootstrap_managed_layer", lambda *a, **kw: next(stream)
    )
    return client.load_managed_snapshot(
        None, layer_id="layer", session_id="session", permission_epoch=2, limit=1
    )


def test_native_projected_coordinates_and_identity_preserved(monkeypatch):
    first = fixture_page()
    final = fixture_page(
        features=({"feature_id": "p2"},), has_more=False, next_offset=None
    )
    pin, features = read(monkeypatch, first, final)
    assert pin.crs_code == 5899
    assert features[0]["geometry"]["coordinates"] == [500000, 2300000]
    assert [f["feature_id"] for f in features] == ["p1", "p2"]


@pytest.mark.parametrize(
    "changes",
    [
        {"layer_id": "other"},
        {"source_id": "other"},
        {"crs_code": 4326},
        {"dimensions": "XYZ"},
        {"schema_revision": "c" * 64},
        {"content_revision": "d" * 64},
        {"permission_epoch": 3},
        {"fields": ({"name": "other"},)},
        {"geometry_family": "Polygon"},
        {"features": ({"feature_id": "p1"},)},
    ],
)
def test_reject_mixed_snapshot(monkeypatch, changes):
    final = fixture_page(
        features=({"feature_id": "p2"},), has_more=False, next_offset=None
    )
    with pytest.raises(client.ApiError):
        read(monkeypatch, fixture_page(), replace(final, **changes))


@pytest.mark.parametrize(
    "changes",
    [
        {"next_offset": 0},
        {"next_offset": None},
        {"features": ()},
        {"crs_code": 3405},
        {"content_revision": "bad"},
        {"fields": ({"name": "ovg_feature_id"},)},
        {"features": ({"feature_id": ""},)},
    ],
)
def test_reject_invalid_initial_page(monkeypatch, changes):
    with pytest.raises(client.ApiError):
        read(monkeypatch, fixture_page(**changes))
