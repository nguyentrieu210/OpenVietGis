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
