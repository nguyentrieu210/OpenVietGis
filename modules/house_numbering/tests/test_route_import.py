from modules.house_numbering.route_import import plan_routes


def row(code, kind="street", parent="", prefix="", *, feature_id=None):
    return {
        "key": f"s:{feature_id or code}",
        "source_id": "s",
        "feature_id": feature_id or code,
        "code": code,
        "name": code,
        "route_type": kind,
        "parent_code": parent,
        "prefix": prefix,
        "direction_state": "forward",
        "errors": [],
    }


def test_multilevel_out_of_order_and_local_prefix():
    plan = plan_routes(
        [row("C", "alley", "B", "6"), row("B", "alley", "A", "155"), row("A")], []
    )
    assert [r["code"] for r in plan] == ["A", "B", "C"]
    assert [r.get("resolved_prefix") for r in plan] == [None, "155", "155/6"]
    assert all(r["state"] == "ready" for r in plan)


def test_full_prefix_parent_and_duplicate_checks():
    plan = plan_routes(
        [
            row("A"),
            row("B", "alley", "A", "155"),
            row("C", "alley", "B", "155/3"),
            row("D", "alley", "B", "3"),
            row("E", "alley", "B", "157/4"),
        ],
        [],
    )
    by_code = {r["code"]: r for r in plan}
    assert by_code["C"]["resolved_prefix"] == "155/3"
    assert "duplicate_sibling_prefix" in by_code["D"]["errors"]
    assert "prefix_does_not_follow_parent" in by_code["E"]["errors"]


def test_cycle_or_missing_parent_block():
    plan = plan_routes(
        [
            row("A", "alley", "B", "1"),
            row("B", "alley", "A", "2"),
            row("C", "alley", "missing", "4"),
        ],
        [],
    )
    assert all(r["state"] == "error" for r in plan)
    assert "cycle" in next(r for r in plan if r["code"] == "A")["errors"]


def test_existing_parent_by_saved_code_and_idempotency():
    old = {
        "id": "old-id",
        "name": "A",
        "route_type": "street",
        "prefix": None,
        "parent_route_id": None,
        "linked_source_id": "s",
        "linked_feature_id": "A",
        "direction_state": "forward",
        "source_label": "route-tree-import:A",
        "revision": 1,
    }
    plan = plan_routes([row("A"), row("B", "alley", "A", "155")], [old])
    assert plan[0]["state"] == "existing"
    assert plan[1]["state"] == "ready" and plan[1]["parent_id"] == "old-id"
    next_plan = plan_routes([row("C", "alley", "A", "9")], [old])
    assert next_plan[0]["state"] == "ready" and next_plan[0]["parent_id"] == "old-id"
