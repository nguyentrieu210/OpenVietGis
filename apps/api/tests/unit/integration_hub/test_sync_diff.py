"""Pure domain tests for the R026 deletion-free GeoPackage sync diff planner."""

from __future__ import annotations

import re

import pytest

from openvietgis_api.domains.integration_hub.sync_diff import (
    MAX_SYNC_OPERATIONS,
    ServerFeatureSnapshot,
    UploadedFeatureSnapshot,
    compute_sync_diff,
)

_BASE_CONTENT_REV = "a" * 64
_BASE_SCHEMA_REV = "b" * 64
_SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def test_compute_sync_diff_creates_and_updates_and_unchanged() -> None:
    server = [
        ServerFeatureSnapshot(
            feature_id="F-UNCHANGED",
            attributes={"fld_name": "Same", "fld_rank": 1, "fld_active": True},
            geometry_geojson={"type": "Point", "coordinates": (438442.69, 2291039.83)},
        ),
        ServerFeatureSnapshot(
            feature_id="F-GEOM-ONLY",
            attributes={"fld_name": "MoveMe", "fld_rank": 2, "fld_active": False},
            geometry_geojson={"type": "Point", "coordinates": [438000.0, 2291000.0]},
        ),
        ServerFeatureSnapshot(
            feature_id="F-ATTR-ONLY",
            attributes={"fld_name": "OldName", "fld_rank": 3, "fld_active": True},
            geometry_geojson={"type": "Point", "coordinates": [438100.0, 2291100.0]},
        ),
        ServerFeatureSnapshot(
            feature_id="F-BOTH",
            attributes={"fld_name": "Before", "fld_rank": 4, "fld_active": False},
            geometry_geojson={"type": "Point", "coordinates": [438200.0, 2291200.0]},
        ),
    ]
    uploaded = [
        UploadedFeatureSnapshot(
            feature_id="F-UNCHANGED",
            attributes={"fld_name": "Same", "fld_rank": 1.0, "fld_active": True},
            geometry_geojson={"type": "Point", "coordinates": [438442.69, 2291039.83]},
        ),
        UploadedFeatureSnapshot(
            feature_id="F-GEOM-ONLY",
            attributes={"fld_name": "MoveMe", "fld_rank": 2, "fld_active": False},
            geometry_geojson={"type": "Point", "coordinates": [438001.5, 2291002.5]},
        ),
        UploadedFeatureSnapshot(
            feature_id="F-ATTR-ONLY",
            attributes={"fld_name": "NewName", "fld_rank": 3, "fld_active": True},
            geometry_geojson={"type": "Point", "coordinates": (438100, 2291100)},
        ),
        UploadedFeatureSnapshot(
            feature_id="F-BOTH",
            attributes={"fld_name": "After", "fld_rank": 40, "fld_active": True},
            geometry_geojson={"type": "Point", "coordinates": [438250.0, 2291250.0]},
        ),
        UploadedFeatureSnapshot(
            feature_id="F-CREATE",
            attributes={"fld_name": "Created", "fld_rank": 5, "fld_active": None},
            geometry_geojson={"type": "Point", "coordinates": (438300, 2291300)},
        ),
    ]

    plan = compute_sync_diff(
        server_features=server,
        uploaded_features=uploaded,
        allowed_field_ids=frozenset({"fld_name", "fld_rank", "fld_active"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )

    assert plan.create_count == 1
    assert plan.update_count == 3
    assert plan.unchanged_count == 1
    assert plan.omitted_server_feature_ids == ()
    assert _SHA256_RE.fullmatch(plan.diff_sha256)

    by_id = {op.feature_id: op for op in plan.operations}
    assert set(by_id) == {"F-ATTR-ONLY", "F-BOTH", "F-CREATE", "F-GEOM-ONLY"}
    assert [op.feature_id for op in plan.operations] == [
        "F-ATTR-ONLY",
        "F-BOTH",
        "F-CREATE",
        "F-GEOM-ONLY",
    ]

    # Create
    op_create = by_id["F-CREATE"]
    assert op_create.op_type == "create"
    assert op_create.geometry == {"type": "Point", "coordinates": [438300.0, 2291300.0]}
    assert op_create.geometry_changed is True
    assert op_create.changed_fields == ("fld_name", "fld_rank")
    assert op_create.properties == {"fld_active": None, "fld_name": "Created", "fld_rank": 5}

    # Geometry-only update
    op_geom = by_id["F-GEOM-ONLY"]
    assert op_geom.op_type == "update"
    assert op_geom.geometry_changed is True
    assert op_geom.changed_fields == ()
    assert op_geom.geometry == {"type": "Point", "coordinates": [438001.5, 2291002.5]}

    # Attribute-only update
    op_attr = by_id["F-ATTR-ONLY"]
    assert op_attr.op_type == "update"
    assert op_attr.geometry_changed is False
    assert op_attr.changed_fields == ("fld_name",)
    assert op_attr.properties["fld_name"] == "NewName"

    # Both geometry and attribute update
    op_both = by_id["F-BOTH"]
    assert op_both.op_type == "update"
    assert op_both.geometry_changed is True
    assert op_both.changed_fields == ("fld_active", "fld_name", "fld_rank")


def test_unchanged_detection_handles_tuple_vs_list_floats_and_signed_zero() -> None:
    server = [
        ServerFeatureSnapshot(
            feature_id="LINE-1",
            attributes={"fld_rank": 10, "fld_active": True, "fld_note": None},
            geometry_geojson={
                "type": "LineString",
                "coordinates": ((0, -0.0), (438442.5, 2291039.25)),
            },
        ),
        ServerFeatureSnapshot(
            feature_id="POLY-1",
            attributes={"fld_rank": 1, "fld_active": False, "fld_note": "ok"},
            geometry_geojson={
                "type": "Polygon",
                "coordinates": (((0.0, 0.0), (10, 0), (10, 10), (0, 0)),),
            },
        ),
    ]
    uploaded = [
        UploadedFeatureSnapshot(
            feature_id="LINE-1",
            attributes={"fld_rank": 10.0, "fld_active": True},
            geometry_geojson={
                "type": "LineString",
                "coordinates": [[0.0, 0.0], [438442.5, 2291039.25]],
            },
        ),
        UploadedFeatureSnapshot(
            feature_id="POLY-1",
            attributes={"fld_rank": 1, "fld_active": False, "fld_note": "ok"},
            geometry_geojson={
                "type": "Polygon",
                "coordinates": [[[0, 0], [10.0, 0.0], [10.0, 10.0], [-0.0, 0.0]]],
            },
        ),
    ]

    plan = compute_sync_diff(
        server_features=server,
        uploaded_features=uploaded,
        allowed_field_ids=frozenset({"fld_rank", "fld_active", "fld_note"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )

    assert plan.operations == ()
    assert plan.create_count == 0
    assert plan.update_count == 0
    assert plan.unchanged_count == 2


def test_bool_vs_int_is_detected_as_attribute_change() -> None:
    plan = compute_sync_diff(
        server_features=[
            ServerFeatureSnapshot(
                feature_id="F-1",
                attributes={"fld_flag": True},
                geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
            )
        ],
        uploaded_features=[
            UploadedFeatureSnapshot(
                feature_id="F-1",
                attributes={"fld_flag": 1},
                geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
            )
        ],
        allowed_field_ids=frozenset({"fld_flag"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan.update_count == 1
    assert plan.operations[0].changed_fields == ("fld_flag",)


def test_strict_filtering_against_allowed_field_ids_and_ignores_gpkg_system_columns() -> None:
    server = [
        ServerFeatureSnapshot(
            feature_id="F-1",
            attributes={
                "fld_name": "Alpha",
                "ovg_feature_id": "server-internal",
                "fid": 1,
                "secret_unlisted": "keep-me",
            },
            geometry_geojson={"type": "Point", "coordinates": [10.0, 20.0]},
        )
    ]
    # Only disallowed columns + ovg_feature_id/fid differ -> must be treated as unchanged.
    uploaded_unchanged = [
        UploadedFeatureSnapshot(
            feature_id="F-1",
            attributes={
                "fld_name": "Alpha",
                "ovg_feature_id": "DIFFERENT",
                "fid": 999,
                "secret_unlisted": "INJECTED",
                "extra_qgis_col": "ignored",
            },
            geometry_geojson={"type": "Point", "coordinates": [10.0, 20.0]},
        )
    ]
    plan_unchanged = compute_sync_diff(
        server_features=server,
        uploaded_features=uploaded_unchanged,
        # Even if ovg_feature_id or fid somehow appeared in allowed_field_ids, they are stripped.
        allowed_field_ids=frozenset({"fld_name", "ovg_feature_id", "fid"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan_unchanged.operations == ()
    assert plan_unchanged.unchanged_count == 1

    # When an allowed field changes, emitted properties contain ONLY allowed non-ignored fields.
    uploaded_changed = [
        UploadedFeatureSnapshot(
            feature_id="F-1",
            attributes={
                "fld_name": "Beta",
                "ovg_feature_id": "DIFFERENT",
                "fid": 999,
                "extra_qgis_col": "injected",
            },
            geometry_geojson={"type": "Point", "coordinates": [10.0, 20.0]},
        )
    ]
    plan_changed = compute_sync_diff(
        server_features=server,
        uploaded_features=uploaded_changed,
        allowed_field_ids=frozenset({"fld_name", "ovg_feature_id", "fid"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan_changed.update_count == 1
    op = plan_changed.operations[0]
    assert op.properties == {"fld_name": "Beta"}
    assert "ovg_feature_id" not in op.properties
    assert "fid" not in op.properties
    assert "extra_qgis_col" not in op.properties


def test_duplicate_or_blank_uploaded_feature_id_raises_value_error() -> None:
    with pytest.raises(ValueError, match="Duplicate feature_id"):
        compute_sync_diff(
            server_features=[],
            uploaded_features=[
                UploadedFeatureSnapshot(
                    feature_id="DUP-1",
                    attributes={"fld_name": "A"},
                    geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
                ),
                UploadedFeatureSnapshot(
                    feature_id="DUP-1",
                    attributes={"fld_name": "B"},
                    geometry_geojson={"type": "Point", "coordinates": [3.0, 4.0]},
                ),
            ],
            allowed_field_ids=frozenset({"fld_name"}),
            base_content_revision=_BASE_CONTENT_REV,
            base_schema_revision=_BASE_SCHEMA_REV,
        )

    with pytest.raises(ValueError, match="non-empty"):
        compute_sync_diff(
            server_features=[],
            uploaded_features=[
                UploadedFeatureSnapshot(
                    feature_id="   ",
                    attributes={},
                    geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
                )
            ],
            allowed_field_ids=frozenset(),
            base_content_revision=_BASE_CONTENT_REV,
            base_schema_revision=_BASE_SCHEMA_REV,
        )


def test_max_1000_operations_cap_raises_value_error() -> None:
    assert MAX_SYNC_OPERATIONS == 1000
    uploaded_at_limit = [
        UploadedFeatureSnapshot(
            feature_id=f"F-{i:04d}",
            attributes={"fld_name": f"N-{i}"},
            geometry_geojson={"type": "Point", "coordinates": [float(i), 1.0]},
        )
        for i in range(MAX_SYNC_OPERATIONS)
    ]
    plan_ok = compute_sync_diff(
        server_features=[],
        uploaded_features=uploaded_at_limit,
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan_ok.create_count == MAX_SYNC_OPERATIONS

    uploaded_over_limit = [
        *uploaded_at_limit,
        UploadedFeatureSnapshot(
            feature_id="F-OVER",
            attributes={"fld_name": "Over"},
            geometry_geojson={"type": "Point", "coordinates": [9999.0, 1.0]},
        ),
    ]
    with pytest.raises(ValueError, match="maximum 1,000"):
        compute_sync_diff(
            server_features=[],
            uploaded_features=uploaded_over_limit,
            allowed_field_ids=frozenset({"fld_name"}),
            base_content_revision=_BASE_CONTENT_REV,
            base_schema_revision=_BASE_SCHEMA_REV,
        )


def test_never_emits_delete_operations_and_reports_sorted_omitted_server_ids() -> None:
    server = [
        ServerFeatureSnapshot(
            feature_id="HVS-QN4",
            attributes={"fld_name": "Four"},
            geometry_geojson={"type": "Point", "coordinates": [439365.92, 2292185.83]},
        ),
        ServerFeatureSnapshot(
            feature_id="HVS-QN1",
            attributes={"fld_name": "One"},
            geometry_geojson={"type": "Point", "coordinates": [438442.69, 2291039.83]},
        ),
        ServerFeatureSnapshot(
            feature_id="HVS-QN3",
            attributes={"fld_name": "Three"},
            geometry_geojson={"type": "Point", "coordinates": [438900.0, 2291500.0]},
        ),
        ServerFeatureSnapshot(
            feature_id="HVS-QN2",
            attributes={"fld_name": "Two"},
            geometry_geojson={"type": "Point", "coordinates": [437957.55, 2291878.33]},
        ),
    ]
    # Upload only HVS-QN2 (updated) and omit QN1, QN3, QN4.
    uploaded = [
        UploadedFeatureSnapshot(
            feature_id="HVS-QN2",
            attributes={"fld_name": "Two-Edited"},
            geometry_geojson={"type": "Point", "coordinates": [437957.55, 2291878.33]},
        )
    ]

    plan = compute_sync_diff(
        server_features=server,
        uploaded_features=uploaded,
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )

    assert plan.create_count == 0
    assert plan.update_count == 1
    assert plan.unchanged_count == 0
    assert all(op.op_type in ("create", "update") for op in plan.operations)
    assert plan.omitted_server_feature_ids == ("HVS-QN1", "HVS-QN3", "HVS-QN4")

    # Even with an empty upload, zero delete operations are emitted.
    empty_plan = compute_sync_diff(
        server_features=server,
        uploaded_features=[],
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert empty_plan.operations == ()
    assert empty_plan.omitted_server_feature_ids == ("HVS-QN1", "HVS-QN2", "HVS-QN3", "HVS-QN4")


def test_diff_sha256_is_deterministic_order_invariant_and_sensitive_to_revisions_and_ops() -> None:
    server = [
        ServerFeatureSnapshot(
            feature_id="F-1",
            attributes={"fld_name": "A"},
            geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
        )
    ]
    up_1 = UploadedFeatureSnapshot(
        feature_id="F-1",
        attributes={"fld_name": "A-edited"},
        geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
    )
    up_2 = UploadedFeatureSnapshot(
        feature_id="F-2",
        attributes={"fld_name": "B-new"},
        geometry_geojson={"type": "Point", "coordinates": [3.0, 4.0]},
    )

    plan_a = compute_sync_diff(
        server_features=server,
        uploaded_features=[up_1, up_2],
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    plan_reordered = compute_sync_diff(
        server_features=server,
        uploaded_features=[up_2, up_1],
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan_a.diff_sha256 == plan_reordered.diff_sha256
    assert _SHA256_RE.fullmatch(plan_a.diff_sha256)

    plan_diff_content_rev = compute_sync_diff(
        server_features=server,
        uploaded_features=[up_1, up_2],
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision="c" * 64,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan_diff_content_rev.diff_sha256 != plan_a.diff_sha256

    plan_diff_schema_rev = compute_sync_diff(
        server_features=server,
        uploaded_features=[up_1, up_2],
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision="d" * 64,
    )
    assert plan_diff_schema_rev.diff_sha256 != plan_a.diff_sha256

    plan_diff_op = compute_sync_diff(
        server_features=server,
        uploaded_features=[
            UploadedFeatureSnapshot(
                feature_id="F-1",
                attributes={"fld_name": "A-edited-differently"},
                geometry_geojson={"type": "Point", "coordinates": [1.0, 2.0]},
            ),
            up_2,
        ],
        allowed_field_ids=frozenset({"fld_name"}),
        base_content_revision=_BASE_CONTENT_REV,
        base_schema_revision=_BASE_SCHEMA_REV,
    )
    assert plan_diff_op.diff_sha256 != plan_a.diff_sha256
