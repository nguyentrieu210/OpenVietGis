from modules.house_numbering.address_suggestions import rank

STAGE = {
    "key": "s",
    "origin": "staged",
    "name": "Đường A hẻm 91",
    "road": "Đường A",
    "prefix": "91",
    "status": "staged",
}
CHILD = {
    "key": "c",
    "origin": "catalog",
    "name": "Đường A hẻm 91/14",
    "road": "Đường A",
    "prefix": "91/14",
    "status": "confirmed",
}
STREET = {
    "key": "r",
    "origin": "catalog",
    "name": "Đường A",
    "road": "Đường A",
    "prefix": "",
    "status": "confirmed",
}


def row(road="Đường A", alley="", eligible=True, can_belong=True):
    return {
        "road": road,
        "alley": alley,
        "eligible": eligible,
        "can_belong": can_belong,
    }


def test_missing_prefix_is_not_street_frontage():
    p = rank(row(), [(STREET, 1), (STAGE, 5)], "Đường A", "91")
    assert p["priority"] == "partial"
    assert p["candidates"][0]["key"] == "s"
    assert not any(c["full_match"] for c in p["candidates"])
    assert "missing_prefix" in p["candidates"][1]["signals"]


def test_exact_child_is_ranked_above_staged_parent():
    p = rank(row(alley="91/14"), [(STAGE, 1), (CHILD, 15)], "Đường A", "91")
    assert p["priority"] == "strong"
    assert p["candidates"][0]["decision"] == "other_address"
    assert p["candidates"][0]["prefix"] == "91/14"


def test_wrong_road_is_conflict_even_when_close():
    p = rank(row(road="Đường B", alley="91"), [(STAGE, 0)], "Đường A", "91")
    assert (
        p["priority"] == "conflict" and "road_conflict" in p["candidates"][0]["signals"]
    )


def test_duplicate_or_draft_not_strong():
    p = rank(
        row(alley="91/14"),
        [(CHILD, 2), ({**CHILD, "key": "c2"}, 3)],
        "Đường A",
        "91",
        True,
    )
    assert p["priority"] == "conflict"
    p = rank(row(alley="91/14"), [({**CHILD, "status": "draft"}, 2)], "Đường A", "91")
    assert p["priority"] == "partial"


def test_distance_and_lock_prevent_prefill():
    p = rank(row(can_belong=False), [(STAGE, 35.001)], "Đường A", "91")
    assert p["priority"] == "unmatched" and not p["candidates"][0]["can_prefill"]
    p = rank(row(eligible=False), [(STAGE, 1)], "Đường A", "91")
    assert p == {"priority": "blocked", "candidates": []}
    p = rank(row(alley="91/14"), [(CHILD, 81)], "Đường A", "91")
    assert p["priority"] == "unmatched" and not p["candidates"][0]["can_prefill"]
