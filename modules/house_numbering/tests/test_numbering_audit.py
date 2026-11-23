from modules.house_numbering.batch_numbering import build_route_plan
from modules.house_numbering.domain import Rule
from modules.house_numbering.numbering_audit import (
    audit_route_numbering,
    summarize_audit,
)

ROUTE = {"id": "route-1", "srid": 3857, "prefix": "C19"}
VERTICES = [(0, 0), (100, 0)]
RULE = Rule("review", max_distance=40)


def point(bid, x, y, number=None, kind=None):
    return {
        "id": bid,
        "x": x,
        "y": y,
        "number_id": "n-" + bid if number else None,
        "number_text": number,
        "kind": kind,
    }


def num(bid, text, side="right", kind="approved"):
    return {
        "id": "n-" + bid,
        "building_id": bid,
        "number_text": text,
        "side": side,
        "measure_m": 5,
        "kind": kind,
    }


def audit(points=(), numbers=(), changes=()):
    return audit_route_numbering(
        route=ROUTE,
        vertices=VERTICES,
        rule=RULE,
        points=list(points),
        numbers=list(numbers),
        changes=list(changes),
    )


def test_gap_is_only_information_and_reservations_count_as_occupied():
    points = [
        point("a", 10, -2, "C19/2", "approved"),
        point("b", 30, -2, "C19/8", "approved"),
    ]
    nums = [
        num("a", "C19/2"),
        num("b", "C19/8"),
        {
            "id": "reserve",
            "building_id": None,
            "number_text": "C19/4",
            "side": None,
            "measure_m": None,
            "kind": "reserved",
        },
    ]
    a = audit(points, nums)
    assert a["blocking_issues"] == 0 and a["gap_count"] == 1
    assert a["gaps"] == [
        {
            "side": "right",
            "from_number": "C19/2",
            "to_number": "C19/8",
            "unused_slots": 1,
        }
    ]
    assert summarize_audit([a])["can_save"]


def test_collision_and_wrong_parity_are_blocking():
    points = [
        point("a", 10, -2, "C19/3", "approved"),
        point("b", 20, -2, "C19/3", "survey"),
    ]
    nums = [num("a", "C19/3"), num("b", "C19/3", kind="survey")]
    a = audit(points, nums)
    assert a["duplicate_count"] == 1 and a["parity_error_count"] == 2
    assert a["blocking_issues"] >= 3 and not summarize_audit([a])["can_save"]


def test_wrong_order_and_tied_chainage_are_blocking():
    a = audit(
        [
            point("a", 10, -2, "C19/8", "approved"),
            point("b", 20, -2, "C19/2", "approved"),
        ],
        [num("a", "C19/8"), num("b", "C19/2")],
    )
    assert a["ordering_error_count"] == 1
    a = audit(
        [
            point("a", 10, -2, "C19/2", "approved"),
            point("b", 10, -3, "C19/4", "approved"),
        ],
        [num("a", "C19/2"), num("b", "C19/4")],
    )
    assert a["ordering_error_count"] == 1


def test_wrong_prefix_and_new_candidate_occupied_are_blocking():
    a = audit(
        numbers=[num("a", "B12/2")],
        changes=[
            {"building_id": "b", "number": "B12/2", "side": "right", "measure": 20}
        ],
    )
    assert a["invalid_format_count"] == 2 and a["duplicate_count"] == 1


def test_generated_values_are_counted_and_historical_number_is_untouched():
    points = [
        point("anchor", 40, -3, "C19/10", "approved"),
        point("survey", 20, -3, "C19/6", "survey"),
        point("new", 30, -3),
    ]
    nums = [num("anchor", "C19/10"), num("survey", "C19/6", kind="survey")]
    changes, warnings = build_route_plan(
        route=ROUTE,
        vertices=VERTICES,
        rule=RULE,
        points=points,
        numbers=nums,
        targets={
            x: {"state": "active", "eligibility": "eligible"} for x in ("survey", "new")
        },
    )
    assert len(changes) == 1 and changes[0]["number"] == "C19/8"
    assert "building survey: existing_number_requires_review" in warnings
    a = audit(points, nums, changes)
    assert (
        a["blocking_issues"] == 0 and a["gap_count"] == 0
    )  # C19/6,8,10 are consecutive even slots


def test_identical_full_address_is_duplicate_only_within_same_street_tree():
    from modules.house_numbering.numbering_audit import find_tree_collisions

    parents = {
        "street-a": None,
        "alley-a1": "street-a",
        "alley-a2": "street-a",
        "street-b": None,
    }
    records = [
        {"route_id": r, "number_text": numtext, "building_id": str(i), "origin": kind}
        for i, (r, numtext, kind) in enumerate(
            [
                ("alley-a1", "C19/2", "existing"),
                ("alley-a2", "C19/2", "proposal"),
                ("street-b", "C19/2", "existing"),
                ("street-a", "2", "existing"),
            ]
        )
    ]
    issues = find_tree_collisions(parents, records)
    assert len(issues) == 1 and issues[0]["root_route_id"] == "street-a"
    assert issues[0]["number"] == "C19/2" and len(issues[0]["occupants"]) == 2


def test_parity_uses_actual_geometry_not_just_outdated_stored_side():
    a = audit(
        [point("a", 10, 3, "C19/2", "approved")], [num("a", "C19/2", side="right")]
    )
    assert a["parity_error_count"] == 1
    assert a["parity_errors"][0]["origin"] == "geometry"
