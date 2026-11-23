import pytest
from openvietgis_api.domains.platform_errors import PlatformValidationError

from modules.house_numbering.route_import import plan_routes
from modules.house_numbering.route_recovery import (
    address_label,
    expected_parent,
    selection_rows,
    survey_evidence,
)
from modules.house_numbering.tests.test_route_import import row


def test_labels_keep_full_alphanumeric_prefix_and_immediate_parent():
    assert address_label("Đường Phạm Hùng hẻm C19/30a") == ("pham hung","C19/30A")
    assert address_label("Hẻm 30A") is None
    assert address_label("QL50 hẻm 2") == ("quoc lo 50","2")
    assert expected_parent({"name":"Đường A hẻm 155/6/3","prefix":"3"}) == ("a","155/6")

def test_duplicate_paths_block_both_candidates_and_children():
    items=plan_routes([row("R"),row("A","alley","R","C19"),
        row("B","alley","R","c19"),row("C","alley","A","30A")],[])
    by={x["code"]:x for x in items}
    assert "duplicate_sibling_prefix" in by["A"]["errors"]
    assert "duplicate_sibling_prefix" in by["B"]["errors"]
    assert "parent_has_errors" in by["C"]["errors"]

def test_partial_selection_requires_ancestors_and_blocks_error_rows():
    items=plan_routes([row("R"),row("A","alley","R","C19"),row("B","alley","A","30A"),row("bad","alley","missing","1")],[])
    plan={"items":items,"ready":3,"existing":0,"errors":1}
    assert [r["code"] for r in selection_rows(plan,["s:R","s:A","s:B"])] == ["R","A","B"]
    for selected in [None,[],["s:A"],["s:B","s:R"],["s:bad"],["s:R","s:R"],["unknown"]]:
        with pytest.raises(PlatformValidationError):
            selection_rows(plan,selected)

def test_survey_conflicts_and_existing_links_block_review():
    class Result:
        def mappings(self):
            return [{"code":"A","road":"Đường A","alley":"C19","unassigned":True,"amount":2},
                    {"code":"A","road":"Đường B","alley":"C19","unassigned":True,"amount":3,"example_sources":["source-b"],"example_features":["house-b"]},
                    {"code":"A","road":"Đường A","alley":"C19","unassigned":False,"amount":1}]
    class Session:
        def execute(self, statement, params):
            assert params["codes"] == ["A"]
            return Result()
    candidate={"code":"A","name":"Đường A hẻm C19","review_reason":"legacy","state":"ready","errors":[]}
    survey_evidence(Session(),{},[candidate])
    assert candidate["survey_evidence"]=={"referenced":6,"unassigned":5,"address_conflicts":3}
    assert candidate["survey_review"]["counts"] == {"exact":3,"different_road":3}
    assert candidate["survey_review"]["groups"][0]["examples"] == [{"source_id":"source-b","feature_id":"house-b"}]
    assert candidate["state"]=="error"
    assert set(candidate["errors"])=={"review_survey_address_conflict","review_building_already_linked"}
