import pytest
from openvietgis_api.domains.platform_errors import PlatformValidationError

from modules.house_numbering.route_topology import apply_merges, parent_tasks, stable_id


def route(code, name, prefix, kind="alley", parent=""):
    return {
        "key": "s:" + code,
        "source_id": "s",
        "feature_id": code,
        "code": code,
        "name": name,
        "prefix": prefix,
        "route_type": kind,
        "parent_code": parent,
        "direction_state": "forward",
        "review_reason": "legacy",
        "errors": [],
    }


def test_missing_ancestor_levels_are_distinct_and_aggregated():
    rows = [
        route("R", "Đường A", "", "street"),
        route("A", "Đường A hẻm C19/30A/6", "C19/30A/6"),
        route("B", "Đường A hẻm C19/30A/2", "C19/30A/2"),
    ]
    tasks = parent_tasks(rows, [])
    assert [t["prefix"] for t in tasks] == ["C19", "C19/30A"]
    assert [t["children"] for t in tasks] == [2, 2]
    assert tasks[0]["id"] == stable_id("a", "C19")


def test_explicit_merge_preserves_source_codes_and_disputed_parent():
    rows = [
        route("A", "Đường A hẻm C19", "C19", parent="X"),
        route("B", "Đường A hẻm C19", "C19", parent="Y"),
    ]
    item = {
        "id": "recovery-" + stable_id("s:A", "s:B"),
        "name": rows[0]["name"],
        "prefix": "C19",
        "members": [{"key": r["key"]} for r in rows],
        "status": "connected",
    }
    apply_merges(rows, [], [item], [item["id"]])
    assert [r["code"] for r in rows] == ["A", "B"]
    assert all(
        r["logical_code"] == item["id"]
        and r["parent_code"] == ""
        and r["legacy_parent_codes"] == ["X", "Y"]
        for r in rows
    )
    with pytest.raises(PlatformValidationError):
        apply_merges(rows, [], [{**item, "status": "disconnected"}], [item["id"]])
