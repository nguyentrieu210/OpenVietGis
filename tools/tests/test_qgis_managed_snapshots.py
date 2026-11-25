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
    assert "send_pending_commit(config, pending" in source
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
        offset = kw["offset"]
        return client.ManagedBootstrapPage(
            layer_id="l", source_id="src", name="fixture", geometry_family="Point",
            dimensions="XY", crs_code=4326, content_revision="a"*64, schema_revision="b"*64,
            cursor=1, permission_epoch=1, fields=(),
            features=({"feature_id": f"p{offset}"},), has_more=offset == 0,
            next_offset=1 if offset == 0 else None,
        )
    monkeypatch.setattr(client, "bootstrap_managed_layer", bootstrap)
    result = client.load_managed_layers(None, ["l"], None)
    assert result[1][0][1] == [{"feature_id":"p0"},{"feature_id":"p1"}]
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

def test_pending_retry_checks_receipt_before_replaying(monkeypatch):
    from integrations.qgis_plugin.openvietgis_connector import managed_client as client
    sent = []
    request = {"operation_id": "op", "source_id": "source", "operations": [{}]}
    pending = {"operation_id": "op", "request": request}
    monkeypatch.setattr(client, "commit_managed_changes", lambda *a, **kw: sent.append(kw) or "sent")
    monkeypatch.setattr(client, "get_managed_commit_receipt", lambda *a, **kw: "receipt")
    assert client.send_pending_commit(None, pending, recover=True) == "receipt"
    assert sent == []
    def missing(*a, **kw):
        raise ApiError("not found", status_code=404)
    monkeypatch.setattr(client, "get_managed_commit_receipt", missing)
    with pytest.raises(ApiError):
        client.send_pending_commit(None, pending, recover=True, permit_replay=False)
    assert sent == []
    assert client.send_pending_commit(None, pending, recover=True) == "sent"
    assert sent == [request]


@pytest.mark.parametrize(
    "mismatch",
    [
        {"operation_id": "other"},
        {"source_id": "other"},
        {"operation_count": 2},
        {"event_cursor": -1},
        {"resulting_content_revision": "bad"},
        {"status": "unknown"},
    ],
)
def test_unconfirmed_receipt_does_not_replay_pending_commit(monkeypatch, mismatch):
    from integrations.qgis_plugin.openvietgis_connector import managed_client as client

    wire = {
        "status": "accepted",
        "changeset_id": "changeset",
        "operation_id": "pending-op",
        "source_id": "expected-source",
        "resulting_content_revision": "a" * 64,
        "operation_count": 1,
        "event_cursor": 12,
        "audit_event_id": "audit",
    } | mismatch
    request = {"source_id": "expected-source", "operations": [{}]}
    pending = {"operation_id": "pending-op", "request": request}
    monkeypatch.setattr(client, "_managed_base", lambda _config: "https://test.invalid")

    def reply(_config, method, url):
        assert method == "GET"
        assert url.endswith("/commits/pending-op?source_id=expected-source")
        return wire

    monkeypatch.setattr(client, "_json_request", reply)
    with pytest.raises(ApiError, match="Biên nhận"):
        client.send_pending_commit(None, pending, recover=True)
    assert pending["request"] is request


def test_required_fields_preflight_preserves_draft_and_allows_valid_point():
    from integrations.qgis_plugin.openvietgis_connector.managed_client import (
        validate_managed_operations,
    )
    fields = [{"name": "id", "display_name": "Mã", "nullable": False},
              {"name": "note", "nullable": True}]
    op = {"op_type": "create", "feature_id": "new",
          "geometry": {"type": "Point", "coordinates": [106.6, 10.7]},
          "properties": {"id": None, "note": None}}
    with pytest.raises(ApiError, match="1 đối tượng thiếu trường bắt buộc: Mã"):
        validate_managed_operations([op], fields)
    assert op["properties"]["id"] is None
    op["properties"]["id"] = "valid"
    validate_managed_operations([op], fields)
    validate_managed_operations([{"op_type": "delete", "feature_id": "new"}], fields)


def test_sqlite_snapshot_requires_fresh_scope_and_revisions(tmp_path):
    from types import SimpleNamespace

    from integrations.qgis_plugin.openvietgis_connector.snapshot_cache import (
        cache_path,
        clear_snapshots,
        read_snapshot,
        write_snapshot,
    )
    c = SimpleNamespace(base_url="https://example.com", organization_id="o",
                        project_id="p", bearer_token="secret")
    p = cache_path(tmp_path, c, "l")
    assert "secret" not in p.name
    page = SimpleNamespace(layer_id="l", source_id="s", content_revision="a",
                           schema_revision="b", permission_epoch=1, crs_code=4326,
                           fields=[], cacheable=True, feature_count=1)
    features = [{"feature_id": "f"}]
    write_snapshot(p, page, features)
    assert read_snapshot(p, page) == features
    page.cacheable = False
    assert read_snapshot(p, page) is None
    page.cacheable = True
    page.permission_epoch = 2
    assert read_snapshot(p, page) is None
    page.permission_epoch = 1
    page.content_revision = "new"
    assert read_snapshot(p, page) is None
    c.bearer_token = "different"
    assert cache_path(tmp_path, c, "l") != p
    clear_snapshots(tmp_path)
    assert not p.exists()


def test_reopen_uses_one_authorized_probe_not_full_download(tmp_path, monkeypatch):
    from types import SimpleNamespace

    from integrations.qgis_plugin.openvietgis_connector import managed_client as client
    from integrations.qgis_plugin.openvietgis_connector.snapshot_cache import (
        cache_path,
        write_snapshot,
    )
    c = SimpleNamespace(base_url="https://example.com", organization_id="o",
                        project_id="p", bearer_token="secret")
    page = SimpleNamespace(layer_id="l", source_id="s", content_revision="a",
                           schema_revision="b", permission_epoch=1, crs_code=4326,
                           fields=[], cacheable=True, feature_count=2)
    features = [{"feature_id": "a"}, {"feature_id": "b"}]
    write_snapshot(cache_path(tmp_path, c, "l"), page, features)
    calls = []
    monkeypatch.setattr(client, "create_managed_session",
                        lambda *a, **k: SimpleNamespace(id="s", permission_epoch=1))
    monkeypatch.setattr(client, "bootstrap_managed_layer", lambda *a, **k: calls.append(k) or page)
    result = client.load_managed_layers(c, ["l"], None, cache_root=tmp_path)
    assert result[1][0][1] == features
    assert len(calls) == 1 and calls[0]["limit"] == 1


def test_reopen_changed_cache_uses_delta_and_revalidates(monkeypatch):
    from types import SimpleNamespace

    from integrations.qgis_plugin.openvietgis_connector import managed_client as client
    probe = SimpleNamespace(layer_id="l", source_id="s", content_revision="new",
                            schema_revision="b", permission_epoch=1, cursor=3,
                            cacheable=True, feature_count=2)
    record = {"revision": "old", "cursor": 2,
              "features": [{"feature_id": "a"}, {"feature_id": "b"}]}
    delta = SimpleNamespace(reset_required=False, permission_epoch=1, next_cursor=3,
                            has_more=False, changes=[{
                                "source_id": "s", "layer_id": "l", "content_revision": "new",
                                "removals": ["b"], "upserts": [{"feature_id": "c"}],
                            }])
    monkeypatch.setattr(client, "fetch_managed_delta", lambda *a, **kw: delta)
    monkeypatch.setattr(client, "bootstrap_managed_layer", lambda *a, **kw: probe)
    sess = SimpleNamespace(id="s")
    assert client.catch_up_cached_layer(None, sess, probe, record, lambda: False) == [
        {"feature_id": "a"}, {"feature_id": "c"},
    ]
    delta.next_cursor = 4
    assert client.catch_up_cached_layer(None, sess, probe, record, lambda: False) is None
    delta.next_cursor = 3
    delta.reset_required = True
    assert client.catch_up_cached_layer(None, sess, probe, record, lambda: False) is None
