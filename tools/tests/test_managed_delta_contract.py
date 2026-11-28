"""Strict cursor validation for QGIS before provider writes."""

import pytest

from integrations.qgis_plugin.openvietgis_connector.managed_client import (
    ApiError,
    validate_managed_delta,
)


def change(layer="l1", **kw):
    return {
        "cursor": 4,
        "layer_id": layer,
        "source_id": "src",
        "content_revision": "a" * 64,
        "upserts": [{"feature_id": "p1"}],
        "removals": [],
        **kw,
    }


def envelope(**kw):
    return {
        "reset_required": False,
        "has_more": False,
        "next_cursor": 4,
        "permission_epoch": 1,
        "changes": [change()],
        **kw,
    }


def test_aliases_same_cursor():
    page = validate_managed_delta(envelope(changes=[change(), change("l2")]), cursor=3)
    assert len(page.changes) == 2 and page.next_cursor == 4


@pytest.mark.parametrize(
    "bad",
    [
        {"next_cursor": 2},
        {"next_cursor": "4"},
        {"permission_epoch": True},
        {"reset_required": True},
        {"changes": [change(), change()]},
        {"changes": [change(cursor=2)]},
        {"changes": [change(cursor=5)]},
        {"changes": [change(removals=["p1"])]},
        {"changes": [change(content_revision="old")]},
        {"has_more": True, "next_cursor": 3, "changes": []},
    ],
)
def test_reject_bad_envelope(bad):
    with pytest.raises(ApiError):
        validate_managed_delta(envelope(**bad), cursor=3)


def test_reset_allows_rewind_without_changes():
    assert validate_managed_delta(
        envelope(reset_required=True, next_cursor=1, changes=[]), cursor=3
    ).reset_required


def test_delta_request_bounds_before_network(monkeypatch):
    from integrations.qgis_plugin.openvietgis_connector import managed_client as client
    monkeypatch.setattr(client, "_json_request", lambda *a, **k: (_ for _ in ()).throw(AssertionError("network")))
    for cursor, limit in [(-1, 200), (0, 0), (0, 501), (True, 200)]:
        with pytest.raises(ApiError):
            client.fetch_managed_delta(None, session_id="s", cursor=cursor, limit=limit)
