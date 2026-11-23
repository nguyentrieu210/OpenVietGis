"""R027 three-way reconciliation contract tests with synthetic GIS features."""

from __future__ import annotations

from dataclasses import replace

import pytest

from openvietgis_api.domains.integration_hub.bidirectional import (
    FeatureSnapshot,
    is_echo_event,
    reconcile_three_way,
)


def feat(key: str, *, name: str = "A", rank: int = 1, x: float = 0.0) -> FeatureSnapshot:
    return FeatureSnapshot(
        key, {"name": name, "rank": rank, "private": "NEVER_RETURN"},
        {"type": "Point", "coordinates": [x, 5.0]},
    )


def preview(base, ovg, ext, fields=frozenset({"name", "rank"})):
    return reconcile_three_way(
        baseline=base, openvietgis=ovg, external=ext,
        allowed_fields=fields,
        baseline_revision="base-r1",
        openvietgis_revision="ovg-r2",
        external_revision="qgis-r3",
    )


def test_bidirectional_independent_fields_converge_without_leaking_private_columns():
    baseline = [feat("001", name="Old", rank=1)]
    local = [feat("001", name="OVG", rank=1)]
    remote = [feat("001", name="Old", rank=9)]
    result = preview(baseline, local, remote)
    assert result.conflicts == ()
    assert len(result.actions) == 2
    by_side = {a.target_side: a for a in result.actions}
    assert by_side["external"].attributes == {"name": "OVG", "rank": 9}
    assert by_side["external"].changed_fields == ("name",)
    assert by_side["openvietgis"].attributes == {"name": "OVG", "rank": 9}
    assert by_side["openvietgis"].changed_fields == ("rank",)
    assert all("private" not in a.attributes for a in result.actions)

    merged = feat("001", name="OVG", rank=9)
    converged = preview(baseline, [merged], [merged])
    assert converged.actions == ()
    assert converged.conflicts == ()
    assert converged.unchanged_count == 1


def test_only_qgis_geometry_changed_is_proposed_for_openvietgis():
    base = [feat("lot-1", x=0)]
    out = preview(base, base, [feat("lot-1", x=1.75)])
    assert len(out.actions) == 1
    op = out.actions[0]
    assert op.target_side == "openvietgis"
    assert op.action == "update"
    assert op.geometry_changed
    assert op.changed_fields == ()
    assert op.geometry_geojson["coordinates"] == [1.75, 5.0]


def test_parallel_geometry_edits_are_conflict_not_last_writer_wins():
    base = [feat("lot-1", x=0)]
    result = preview(base, [feat("lot-1", name="new", x=2)], [feat("lot-1", x=3)])
    assert result.actions == ()
    assert len(result.conflicts) == 1
    assert result.conflicts[0].geometry_conflict
    assert result.conflicts[0].reason == "concurrent_edit"


def test_overlapping_field_edit_reports_conflicting_field_and_no_write():
    base = [feat("k", name="before")]
    result = preview(base, [feat("k", name="from OVG")], [feat("k", name="from QGIS")])
    assert result.actions == ()
    assert result.conflicts[0].fields == ("name",)


def test_matching_parallel_edit_needs_no_operation():
    base = [feat("lot")]
    same = [feat("lot", name="Both changed to same")]
    out = preview(base, same, same)
    assert out.actions == ()
    assert out.conflicts == ()
    assert out.unchanged_count == 1


def test_missing_on_either_side_is_a_delete_candidate_never_autodelete():
    base = [feat("1"), feat("2")]
    out = preview(base, [feat("1")], [feat("1"), feat("2")])
    assert out.actions == ()
    assert [x.feature_id for x in out.conflicts] == ["2"]
    assert out.conflicts[0].reason == "delete_candidate"


def test_new_records_go_to_other_side_but_concurrent_create_needs_review():
    out = preview([], [feat("L")], [feat("R")])
    assert [(a.target_side, a.action, a.feature_id) for a in out.actions] == [
        ("external", "create", "L"), ("openvietgis", "create", "R")
    ]
    both = preview([], [feat("same")], [feat("same")])
    assert both.actions == ()
    assert both.conflicts[0].reason == "concurrent_create"


def test_order_does_not_change_diff_digest_and_revisions_do():
    base = []
    one = preview(base, [feat("z"), feat("a")], [])
    two = preview(base, [feat("a"), feat("z")], [])
    assert one.digest == two.digest
    assert [a.feature_id for a in one.actions] == ["a", "z"]
    new_revision = reconcile_three_way(
        baseline=base, openvietgis=[feat("z"), feat("a")], external=[],
        allowed_fields=frozenset({"name", "rank"}),
        baseline_revision="base-r1", openvietgis_revision="ovg-r4",
        external_revision="qgis-r3",
    )
    assert one.digest != new_revision.digest


@pytest.mark.parametrize("invalid", [
    [feat("same"), feat("same")],
    [replace(feat("good"), feature_id=" ")],
    [replace(feat("good"), attributes={"name": "A"})],
    [replace(feat("good"), geometry_geojson={"type": "Point", "coordinates": [float("nan"), 1]})],
])
def test_invalid_or_incomplete_snapshots_fail_closed(invalid):
    with pytest.raises(ValueError):
        preview([], invalid, [])


def test_unsafe_input_limits_or_revisions_fail():
    with pytest.raises(ValueError):
        preview([], [], [], frozenset())
    with pytest.raises(ValueError):
        reconcile_three_way(
            baseline=[], openvietgis=[feat("x"), feat("y")], external=[],
            allowed_fields=frozenset({"name"}),
            baseline_revision="base", openvietgis_revision="ovg", external_revision="remote",
            max_actions=1,
        )


def test_origin_chain_prevents_echo_and_rejects_invalid_chain():
    assert is_echo_event(origin_chain=("qgis", "binding-1"), target_binding_id="binding-1")
    assert not is_echo_event(origin_chain=("qgis",), target_binding_id="binding-2")
    with pytest.raises(ValueError):
        is_echo_event(origin_chain=("replay",)*17, target_binding_id="binding-2")