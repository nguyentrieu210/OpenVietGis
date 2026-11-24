from pathlib import Path

import pytest

from integrations.qgis_plugin.openvietgis_connector.client import ApiError
from integrations.qgis_plugin.openvietgis_connector.managed_client import (
    diff_managed_snapshots,
)


def feature(value):
    return {"geometry": {"type": "Point", "coordinates": [1, 2]}, "properties": {"n": value}}


def test_native_save_does_not_lose_diff():
    baseline = {"a": feature(1), "b": feature(2)}
    current = {"a": feature(3), "b": feature(2), "c": feature(4)}
    assert diff_managed_snapshots(baseline, current) == [
        {"op_type": "update", "feature_id": "a", **current["a"]},
        {"op_type": "create", "feature_id": "c", **current["c"]},
    ]
    assert diff_managed_snapshots(current, current) == []


def test_deletes_are_denied_by_default():
    baseline = {"a": feature(1)}
    with pytest.raises(ApiError):
        diff_managed_snapshots(baseline, {})
    assert diff_managed_snapshots(baseline, {}, allow_deletes=True) == [
        {"op_type": "delete", "feature_id": "a"}
    ]


def test_global_cursor_is_not_advanced_by_write_receipt():
    source = (Path(__file__).resolve().parents[2] /
              "integrations/qgis_plugin/openvietgis_connector/plugin.py").read_text()
    assert "self._managed_cursor = max(" not in source
    assert "get_managed_commit_receipt(config, operation_id=pending" in source
    assert 'return "b64:"' not in source


def test_native_geometry_wkt_preserves_projected_coordinates():
    from integrations.qgis_plugin.openvietgis_connector.managed_client import (
        native_geometry_wkt,
    )
    assert native_geometry_wkt({"type": "Point", "coordinates": [11800000.123456789, 1200000]}) == (
        "POINT (11800000.12345679 1200000)"
    )
    assert native_geometry_wkt({"type": "LineString", "coordinates": [[0, 1], [2, 3]]}) == (
        "LINESTRING (0 1, 2 3)"
    )
    assert native_geometry_wkt({
        "type": "Polygon", "coordinates": [[[0, 0], [1, 0], [1, 1], [0, 0]]],
    }) == "POLYGON ((0 0, 1 0, 1 1, 0 0))"
    with pytest.raises(ApiError):
        native_geometry_wkt({"type": "Point", "coordinates": [1, 2, 3]})


def test_initial_load_uses_large_pages_and_rejects_revision_change(monkeypatch):
    from types import SimpleNamespace

    from integrations.qgis_plugin.openvietgis_connector import managed_client as client
    calls = []
    monkeypatch.setattr(client, "create_managed_session",
                        lambda *a, **k: SimpleNamespace(id="s", permission_epoch=1))
    def bootstrap(*a, **kw):
        calls.append(kw)
        return SimpleNamespace(content_revision="a", schema_revision="b", permission_epoch=1,
                               features=[kw["offset"]], has_more=kw["offset"] == 0,
                               next_offset=5000 if kw["offset"] == 0 else None)
    monkeypatch.setattr(client, "bootstrap_managed_layer", bootstrap)
    result = client.load_managed_layers(None, ["l"], None)
    assert result[1][0][1] == [0, 5000]
    assert [c["limit"] for c in calls] == [5000, 5000]
    assert client.load_managed_layers(None, ["l"], None, cancelled=lambda: True) is None


def test_platform_error_message_is_not_discarded():
    import io
    from urllib.error import HTTPError

    from integrations.qgis_plugin.openvietgis_connector.client import (
        _extract_http_error_detail,
    )
    error = HTTPError("https://example.com", 422, "Unprocessable Content", {}, io.BytesIO(
        b'{"code":"SCHEMA_MISMATCH","message":"field fld_x does not match integer"}'
    ))
    assert _extract_http_error_detail(error, "secret") == (
        "SCHEMA_MISMATCH: field fld_x does not match integer"
    )
