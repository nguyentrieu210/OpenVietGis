"""US5 independent CRS acceptance on real restricted-runtime HTTP and PostGIS.

Published target coordinates and analytic controls come only from the independent
test fixture. SQL observation proves explicit pipeline invocation; it never
implements the product transform. Temporary synthetic fixtures exercise boundary
conditions without editing the checked-in production allowlist or native rows.
"""

from __future__ import annotations

import json
import math
import os
import re
from collections import defaultdict
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from types import MappingProxyType
from uuid import uuid4

import pytest
from sqlalchemy import event, text

from openvietgis_api.adapters.persistence import spatial_fixtures
from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.adapters.persistence.spatial_fixtures import FixtureProvenance
from openvietgis_api.domains.spatial.domain import GeometryFamily, NativeGeometry
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.fixtures.spatial_controls import (
    DIRECTED_CONTROL_SETS,
    OUT_OF_DOMAIN_CONTROLS,
    CoordinateControl,
    DirectedControlSet,
    web_mercator_forward,
)
from tests.integration.spatial.test_features import (
    _context,
    _coordinate_reference,
    _coordinates,
    _read,
    _setup,
)
from tests.integration.spatial.test_layers import _counts
from tests.integration.spatial.test_layers import _path as _layer_path
from tests.integration.spatial.test_sources import _client, _error, _get, _register, _rows
from tests.integration.spatial.test_sources import _path as _source_path

# Independent frozen research.md pipeline identities, not imported from runtime.
WEB = (
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +ellps=WGS84"
)
DATUM = "urn:ogc:def:coordinateOperation:EPSG::6960"
TM3 = (
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad "
    "+step +proj=tmerc +lat_0=0 +lon_0=107.75 +k=0.9999 "
    "+x_0=500000 +y_0=0 +ellps=WGS84"
)
FORWARD = "st_transformpipeline"
INVERSE = "st_inversetransformpipeline"

# Each row: native CRS -> target, approved function, exact pipeline, explicit output SRID.
STEPS = {
    (4326, 3857): ((FORWARD, WEB, 3857),),
    (3857, 4326): ((INVERSE, WEB, 4326),),
    (4756, 4326): ((FORWARD, DATUM, 4326),),
    (4326, 4756): ((INVERSE, DATUM, 4756),),
    (5899, 4756): ((INVERSE, TM3, 4756),),
    (4756, 5899): ((FORWARD, TM3, 5899),),
    (5899, 4326): ((INVERSE, TM3, 4756), (FORWARD, DATUM, 4326)),
    (4326, 5899): ((INVERSE, DATUM, 4756), (FORWARD, TM3, 5899)),
}
PROFILE_KEYS = {
    "source_code",
    "target_code",
    "operation_id",
    "method",
    "validated_area",
    "tolerance",
    "tolerance_unit",
    "evidence_id",
}

EDGE_CASES = [
    pytest.param(edge, id=f"{edge.source_code}-to-{edge.target_code}")
    for edge in DIRECTED_CONTROL_SETS
]


@contextmanager
def _observe_runtime_sql() -> Iterator[list[tuple[str, str]]]:
    """Observe the actual restricted engine, never wrap/replace a transformation."""
    engine = get_engine()
    calls: list[tuple[str, str]] = []

    def before_execute(conn, cursor, statement, parameters, context, executemany):
        if re.search(r"st_(?:inverse)?transform(?:pipeline)?\s*\(", statement, re.I):
            calls.append((statement.lower(), f"{statement} {parameters!r}"))

    event.listen(engine, "before_cursor_execute", before_execute)
    try:
        yield calls
    finally:
        event.remove(engine, "before_cursor_execute", before_execute)


def _assert_pipeline(calls, edge: DirectedControlSet) -> None:
    assert calls, "no SQL transformation was executed by the actual runtime"
    assert all(not re.search(r"\bst_transform\s*\(", query) for query, _ in calls)
    for function, pipeline, output_code in STEPS[(edge.source_code, edge.target_code)]:
        matching = [payload for query, payload in calls if re.search(rf"\b{function}\s*\(", query)]
        assert matching, f"missing approved explicit function {function}"
        assert any(
            pipeline in payload and re.search(rf"\b{output_code}\b", payload)
            for payload in matching
        ), f"missing exact operation and output SRID for {edge.operation_id}"


def _registry(client, world) -> dict:
    path = (
        f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
        "/spatial/crs-profiles"
    )
    result = _read(client, path, {})
    assert set(result) == {"organization_id", "project_id", "profiles", "transformations"}
    assert result["organization_id"] == str(world.organization_id)
    assert result["project_id"] == str(world.project_id)
    profiles = {item["code"]: item for item in result["profiles"]}
    assert set(profiles) == {4326, 3857, 4756, 5899, 3405, 3406}
    assert len(result["profiles"]) == 6
    for code, profile in profiles.items():
        assert profile["authority"] == "EPSG"
        geographic = code in (4326, 4756)
        assert profile["native_axes"] == (
            ["latitude", "longitude"] if geographic else ["easting", "northing"]
        )
        assert profile["coordinate_order"] == "xy"
        expected = _coordinate_reference(code)
        for field in ("x_axis", "y_axis", "unit"):
            assert profile[field] == expected[field]
        assert profile["disposition"] == ("definition_only" if code in (3405, 3406) else "readable")
    edges = {(edge["source_code"], edge["target_code"]): edge for edge in result["transformations"]}
    assert set(edges) == set(STEPS)
    assert len(result["transformations"]) == 8
    for expected in DIRECTED_CONTROL_SETS:
        edge = edges[(expected.source_code, expected.target_code)]
        assert set(edge) == PROFILE_KEYS
        assert edge["operation_id"] == expected.operation_id
        assert edge["tolerance"] == expected.tolerance
        assert edge["tolerance_unit"] == expected.tolerance_unit
        assert edge["evidence_id"] == expected.evidence_id
        assert edge["method"] and edge["validated_area"]
    return result


def _edge_wire(registry: dict, edge: DirectedControlSet) -> dict:
    return next(
        profile
        for profile in registry["transformations"]
        if (profile["source_code"], profile["target_code"]) == (edge.source_code, edge.target_code)
    )


def _residual(actual, target, edge: DirectedControlSet) -> float:
    assert len(actual) == len(target) == 2
    assert all(type(value) in (int, float) and math.isfinite(value) for value in actual)
    if edge.tolerance_unit == "degree":
        return max(abs(a - b) for a, b in zip(actual, target, strict=True))
    if edge.target_code in (3857, 5899):
        return math.dist(actual, target)
    # Independent same-datum geodesic residual, NOT transformation or inverse roundtrip.
    with get_admin_engine().begin() as connection:
        return connection.execute(
            text("""
            SELECT ST_Distance(
                ST_SetSRID(ST_MakePoint(:actual_x,:actual_y),:srid)::geography,
                ST_SetSRID(ST_MakePoint(:target_x,:target_y),:srid)::geography
            )
            """),
            {
                "actual_x": actual[0],
                "actual_y": actual[1],
                "target_x": target[0],
                "target_y": target[1],
                "srid": edge.target_code,
            },
        ).scalar_one()


_MEASURED_CONTROLS: list[dict] = []


@pytest.fixture(autouse=True)
def _record_control_measurements(capsys):
    start = len(_MEASURED_CONTROLS)
    yield
    measured = _MEASURED_CONTROLS[start:]
    if not measured:
        return
    maxima = {}
    for row in _MEASURED_CONTROLS:
        key = row["operation_id"]
        maxima[key] = max(maxima.get(key, 0.0), row["residual"])
    with get_engine().connect() as connection:
        runtime = connection.execute(text("SELECT version(),PostGIS_Full_Version()")).one()
    report = {
        "runtime": list(runtime),
        "observed_image_digest": os.environ.get("OPENVIETGIS_TEST_DATABASE_IMAGE_DIGEST"),
        "max_residual_by_operation": maxima,
        "controls": _MEASURED_CONTROLS,
    }
    destination = Path("dist/performance/r003-crs-controls.json")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2), encoding="utf-8")
    with capsys.disabled():
        print(
            "R003_CRS_CONTROLS "
            + json.dumps(
                {
                    "max_residual_by_operation": maxima,
                    "control_observations": len(_MEASURED_CONTROLS),
                    "runtime": list(runtime),
                    "observed_image_digest": report["observed_image_digest"],
                }
            )
        )


def _assert_target(actual, expected, edge: DirectedControlSet, point_id: str) -> None:
    residual = _residual(actual, expected, edge)
    _MEASURED_CONTROLS.append(
        {
            "operation_id": edge.operation_id,
            "point_id": point_id,
            "actual": list(actual),
            "expected": list(expected),
            "residual": residual,
            "tolerance": edge.tolerance,
            "unit": edge.tolerance_unit,
        }
    )
    assert residual <= edge.tolerance, (
        f"{edge.operation_id}/{point_id}: {residual:.12g} {edge.tolerance_unit} "
        f"> {edge.tolerance:.12g}"
    )


def _add_synthetic_points(monkeypatch, edge: DirectedControlSet, controls):
    """Test-only independent points not present in the immutable product fixture set.

    Each gets a separate finite manifest to avoid unsupported wrapped extents at
    opposite antimeridian boundaries. Real HTTP registration still validates
    actual stored geometry and computes ordinary metadata/pins/evidence.
    """
    additions = {}
    for index, control in enumerate(controls):
        key = f"test_crs_{edge.source_code}_{edge.target_code}_{index}"
        additions[key] = spatial_fixtures._point_fixture(
            key,
            edge.source_code,
            ((control.point_id, control.source_xy),),
            FixtureProvenance.ANALYTIC_WEB_MERCATOR,
        )
    monkeypatch.setattr(
        spatial_fixtures,
        "REFERENCE_FIXTURES",
        MappingProxyType({**spatial_fixtures.REFERENCE_FIXTURES, **additions}),
    )
    return zip(additions, controls, strict=True)


def _check_native_and_target(
    client, world, registry, edge: DirectedControlSet, key: str, controls: list[CoordinateControl]
) -> None:
    path, source, layer_id, pins = _setup(client, world, key)
    assert source["validation_state"] == "ready"
    profile = next(item for item in registry["profiles"] if item["code"] == edge.source_code)
    assert source["source_crs"] == source["stored_crs"] == profile
    before = _rows(source["id"])
    counts = _counts(world)
    native_context = _context(world, source, layer_id, pins, edge.source_code)
    target_context = {
        **_context(world, source, layer_id, pins, edge.target_code),
        "transformation": _edge_wire(registry, edge),
    }
    # Real page, continuation and detail must agree on context and coordinates.
    query = {**pins, "response_crs": edge.target_code, "limit": 2}
    with _observe_runtime_sql() as calls:
        page = _read(client, path, query)
        assert page["context"] == target_context
        assert page["items"]
        details = {}
        for control in controls:
            detail = _read(
                client,
                f"{path}/{control.point_id}",
                {**pins, "response_crs": edge.target_code},
            )
            assert detail["context"] == target_context
            details[control.point_id] = detail["feature"]
        cursor = page["next_after_feature_id"]
        while cursor:
            next_page = _read(client, path, {**query, "after_feature_id": cursor})
            assert next_page["context"] == target_context
            assert all(item["feature_id"] > cursor for item in next_page["items"])
            page["items"] += next_page["items"]
            cursor = next_page["next_after_feature_id"]
            # Bound the independent large primary point oracle to two pages;
            # detail still checks the independently frozen last point.
            if source["feature_count"] > 10:
                break
        if source["feature_count"] <= 10:
            assert len(page["items"]) == source["feature_count"]
            assert cursor is None
    _assert_pipeline(calls, edge)
    by_id = {item["feature_id"]: item for item in page["items"]}
    for control in controls:
        native = _read(client, f"{path}/{control.point_id}", pins)
        identity = _read(
            client,
            f"{path}/{control.point_id}",
            {**pins, "response_crs": edge.source_code},
        )
        assert identity == native
        assert native["context"] == native_context
        original = native["feature"]
        assert original["geometry"]["coordinate_reference"] == _coordinate_reference(
            edge.source_code
        )
        assert original["geometry"]["type"] == "Point"
        assert original["geometry"]["coordinates"] == pytest.approx(
            control.source_xy, rel=0, abs=1e-8 if edge.source_code in (4326, 4756) else 1e-6
        )
        transformed = details[control.point_id]
        if control.point_id in by_id:
            assert by_id[control.point_id] == transformed
        assert {k: v for k, v in transformed.items() if k != "geometry"} == {
            k: v for k, v in original.items() if k != "geometry"
        }
        geometry = transformed["geometry"]
        assert set(geometry) == {"type", "coordinates", "coordinate_reference"}
        assert geometry["type"] == "Point"
        assert geometry["coordinate_reference"] == _coordinate_reference(edge.target_code)
        _assert_target(geometry["coordinates"], control.expected_xy, edge, control.point_id)
    assert _rows(source["id"]) == before
    assert _get(client, _source_path(world), source["id"]) == source
    assert _counts(world) == counts


@pytest.mark.parametrize("edge", EDGE_CASES)
def test_all_approved_directions_against_independent_controls(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
    edge: DirectedControlSet,
) -> None:
    world = seed_access_world()
    with get_engine().connect() as connection:
        assert tuple(
            connection.execute(
                text(
                    "SELECT current_user,rolsuper,rolbypassrls FROM pg_roles "
                    "WHERE rolname=current_user"
                )
            ).one()
        ) == ("openvietgis_runtime", False, False)
    with _client(world.owner_user_id) as client:
        registry = _registry(client, world)
        grouped = defaultdict(list)
        direct_only = []
        for control in edge.controls:
            if control.reference_key is None:
                direct_only.append(control)
            elif control.reference_key not in ("reference_lines_4326", "reference_polygons_4326"):
                # Line/Polygon control vertices are exercised as complete geometry below.
                grouped[control.reference_key].append(control)
        assert grouped
        for key, controls in grouped.items():
            _check_native_and_target(client, world, registry, edge, key, controls)
        if direct_only:
            assert (edge.source_code, edge.target_code) in ((4326, 3857), (3857, 4326))
            for key, control in _add_synthetic_points(monkeypatch, edge, direct_only):
                _check_native_and_target(client, world, registry, edge, key, [control])


@pytest.mark.parametrize(
    ("key", "prefix", "family"),
    [
        ("reference_lines_4326", "line", "LineString"),
        ("reference_polygons_4326", "polygon", "Polygon"),
    ],
)
def test_webmercator_transforms_each_vertex_and_preserves_geometry_shape(
    migrated_database: None,
    key: str,
    prefix: str,
    family: str,
) -> None:
    world = seed_access_world()
    edge = next(
        case
        for case in DIRECTED_CONTROL_SETS
        if (case.source_code, case.target_code) == (4326, 3857)
    )
    with _client(world.owner_user_id) as client:
        registry = _registry(client, world)
        path, source, layer_id, pins = _setup(client, world, key)
        before, counts = _rows(source["id"]), _counts(world)
        params = {**pins, "response_crs": 3857}
        target = {
            **_context(world, source, layer_id, pins, 3857),
            "transformation": _edge_wire(registry, edge),
        }
        with _observe_runtime_sql() as calls:
            page = _read(client, path, {**params, "limit": 2})
            assert page["context"] == target
            assert len(page["items"]) == 2
            for sequence, item in enumerate(page["items"], 1):
                assert item["feature_id"] == f"{prefix}-{sequence:06d}"
                assert _read(client, f"{path}/{item['feature_id']}", params) == {
                    "context": target,
                    "feature": item,
                }
                geometry = item["geometry"]
                assert geometry["type"] == family
                assert geometry["coordinate_reference"] == _coordinate_reference(3857)
                native = _coordinates(family, sequence)
                actual = geometry["coordinates"]
                if family == "Polygon":
                    assert len(actual) == len(native) == 1
                    actual, native = actual[0], native[0]
                    assert actual[0] == actual[-1]
                assert len(actual) == len(native)
                for xy, independent in zip(actual, native, strict=True):
                    _assert_target(
                        xy, web_mercator_forward(tuple(independent)), edge, item["feature_id"]
                    )
        _assert_pipeline(calls, edge)
        assert _rows(source["id"]) == before
        assert _get(client, _source_path(world), source["id"]) == source
        assert _counts(world) == counts


@pytest.mark.parametrize(
    ("key", "feature_id", "codes"),
    [
        ("reference_points_3857", "merc-000001", (4756, 5899)),
        ("reference_points_4756", "VH1", (3857,)),
        ("reference_points_5899", "HVS-QN1", (3857,)),
    ],
)
def test_reject_unapproved_direct_edges_and_definition_only_crs(
    migrated_database: None,
    key: str,
    feature_id: str,
    codes: tuple[int, ...],
) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        path, source, _, pins = _setup(client, world, key)
        before, counts = _rows(source["id"]), _counts(world)
        for suffix in ("", f"/{feature_id}"):
            for code in codes:
                _error(
                    client.get(path + suffix, params={**pins, "response_crs": code}),
                    422,
                    "UNSUPPORTED_TRANSFORMATION",
                )
            for code in (3405, 3406, 9999):
                _error(
                    client.get(path + suffix, params={**pins, "response_crs": code}),
                    422,
                    "UNSUPPORTED_CRS",
                )
            response = client.get(
                path + suffix, params={**pins, "include_geometry": False, "response_crs": 4326}
            )
            assert response.status_code == 422
            assert response.json() == {
                "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
            }
        assert _rows(source["id"]) == before
        assert _counts(world) == counts


@pytest.mark.parametrize(
    ("key", "feature_id", "target"),
    [
        ("reference_points_4326_webmerc_out_of_area", "point-000001", 3857),
        ("reference_points_4326_quang_ninh", "VH1", 5899),
        ("reference_points_4756", "VH1", 5899),
    ],
)
def test_outside_approved_area_fails_closed_for_page_and_detail(
    migrated_database: None,
    key: str,
    feature_id: str,
    target: int,
) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        path, source, _, pins = _setup(client, world, key)
        before, counts = _rows(source["id"]), _counts(world)
        for suffix in ("", f"/{feature_id}"):
            failed = client.get(path + suffix, params={**pins, "response_crs": target})
            _error(failed, 422, "TRANSFORMATION_OUT_OF_AREA")
            assert set(failed.json()) == {"code", "detail"}
            assert _read(client, path + suffix, pins)["context"]["transformation"] is None
        assert _rows(source["id"]) == before
        assert _counts(world) == counts


def test_untrusted_provenance_and_any_invalid_vertex_or_row_fail_whole_read(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = seed_access_world()
    # Test-only source records exercise valid storage but invalid transform domain.
    # This is not a product provenance override or custom public CRS endpoint.
    untrusted = spatial_fixtures._point_fixture(
        "test_crs_untrusted_qn",
        4326,
        (("near-hvs", (107.16, 20.71)),),
        FixtureProvenance.SYNTHETIC,
    )
    both = spatial_fixtures._point_fixture(
        "test_crs_mixed_area",
        4326,
        (("a-valid", (0.0, 10.0)), ("b-outside", (0.0, 86.0))),
        FixtureProvenance.SYNTHETIC,
    )
    line = spatial_fixtures._fixed(
        "test_crs_line_vertex",
        (
            spatial_fixtures._record(
                "line-000001",
                NativeGeometry(
                    "LineString",
                    ((0.0, 10.0), (1.0, 86.0), (2.0, 11.0)),
                    4326,
                ),
            ),
        ),
        declared_family=GeometryFamily.LINE_STRING,
        expected_family=GeometryFamily.LINE_STRING,
        expected_native_bounds=(0.0, 10.0, 2.0, 86.0),
    )
    monkeypatch.setattr(
        spatial_fixtures,
        "REFERENCE_FIXTURES",
        MappingProxyType(
            {
                **spatial_fixtures.REFERENCE_FIXTURES,
                untrusted.key: untrusted,
                both.key: both,
                line.key: line,
            }
        ),
    )
    with _client(world.owner_user_id) as client:
        for key, feature_id, target in (
            (untrusted.key, "near-hvs", 4756),
            (both.key, "b-outside", 3857),
            (line.key, "line-000001", 3857),
        ):
            path, source, _, pins = _setup(client, world, key)
            assert source["validation_state"] == "ready"
            before, counts = _rows(source["id"]), _counts(world)
            for suffix in ("", f"/{feature_id}"):
                failure = client.get(path + suffix, params={**pins, "response_crs": target})
                _error(failure, 422, "TRANSFORMATION_OUT_OF_AREA")
                assert set(failure.json()) == {"detail", "code"}
            if key == both.key:
                assert (
                    _read(
                        client,
                        f"{path}/a-valid",
                        {
                            **pins,
                            "response_crs": target,
                        },
                    )["feature"]["feature_id"]
                    == "a-valid"
                )
            assert _rows(source["id"]) == before
            assert _counts(world) == counts


@pytest.mark.parametrize(
    ("key", "reason"),
    [
        ("reference_axis_conflict", "AXIS_CONFLICT"),
        ("reference_out_of_domain", "COORDINATE_OUT_OF_DOMAIN"),
        ("reference_definition_only_crs", "UNSUPPORTED_CRS"),
        ("reference_mixed_geometry", "UNSUPPORTED_GEOMETRY"),
        ("reference_unsupported_family", "UNSUPPORTED_GEOMETRY"),
        ("reference_unsupported_dimensions", "UNSUPPORTED_DIMENSIONS"),
        ("reference_unsupported_dimensions_m", "UNSUPPORTED_DIMENSIONS"),
        ("reference_null_geometry", "NULL_GEOMETRY"),
        ("reference_empty_geometry", "EMPTY_GEOMETRY"),
        ("reference_invalid_geometry", "INVALID_GEOMETRY"),
    ],
)
def test_unreadable_native_geometry_cannot_be_attached_or_transformed(
    migrated_database: None,
    key: str,
    reason: str,
) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as client:
        source_id = _register(client, _source_path(world), key, key)["resource_id"]
        source = _get(client, _source_path(world), source_id)
        assert source["validation_state"] != "ready"
        assert source["validation_reason"]["code"] == reason
        before, counts = _rows(source_id), _counts(world)
        _error(
            client.post(_layer_path(world), json={"source_id": source_id, "name": key}),
            409,
            "SOURCE_NOT_READY",
        )
        assert _rows(source_id) == before
        assert _counts(world) == counts


def test_discovery_scope_current_viewer_grant_lifecycle_and_org_status(
    migrated_database: None,
) -> None:
    world, foreign = seed_access_world(), seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}"
    project = f"{root}/projects/{world.project_id}"
    path = project + "/spatial/crs-profiles"
    foreign_root = f"/api/v1/organizations/{foreign.organization_id}"
    foreign_project = f"{foreign_root}/projects/{foreign.project_id}"
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        registry = _registry(owner, world)
        assert member.get(path).status_code == 403
        grant_payload = {
            "subject_kind": "membership",
            "subject_id": str(world.member_membership_id),
            "scope_kind": "project",
            "project_id": str(world.project_id),
            "role": "viewer",
        }
        granted = owner.post(f"{root}/grants", json=grant_payload)
        assert granted.status_code == 201, granted.text
        grant = granted.json()["id"]
        assert _registry(member, world) == registry
        for inaccessible, missing in (
            (
                foreign_project + "/spatial/crs-profiles",
                f"/api/v1/organizations/{uuid4()}/projects/{uuid4()}/spatial/crs-profiles",
            ),
            (
                f"{root}/projects/{foreign.project_id}/spatial/crs-profiles",
                f"{root}/projects/{uuid4()}/spatial/crs-profiles",
            ),
        ):
            denied, absent = member.get(inaccessible), member.get(missing)
            assert denied.status_code == absent.status_code == 404
            assert denied.json() == absent.json()
        assert owner.post(f"{root}/grants/{grant}/revoke").status_code == 204
        denied = member.get(path)
        assert denied.status_code == 403
        assert set(denied.json()) == {"detail"}
        assert owner.post(f"{root}/grants", json=grant_payload).status_code == 201

        before = _counts(world)
        response = owner.patch(project, json={"status": "archived"})
        assert response.status_code == 200, response.text
        assert _registry(member, world) == registry
        assert _registry(owner, world) == registry
        assert owner.patch(root, json={"status": "suspended"}).status_code == 200
        for actor in (owner, member):
            denied = actor.get(path)
            assert denied.status_code == 403
            assert set(denied.json()) == {"detail"}
        assert owner.patch(root, json={"status": "active"}).status_code == 200
        assert _registry(member, world) == registry
        assert owner.patch(project, json={"status": "deleted"}).status_code == 200
        for actor in (owner, member):
            missing = actor.get(f"{root}/projects/{uuid4()}/spatial/crs-profiles")
            deleted = actor.get(path)
            assert deleted.status_code == missing.status_code == 404
            assert deleted.json() == missing.json()
        assert _counts(world) == before


@pytest.mark.parametrize(
    "control",
    [
        pytest.param(control, id=control.control_id)
        for control in OUT_OF_DOMAIN_CONTROLS
        if control.valid_native
        and control.source_code == 4326
        and control.target_code == 3857
        and control.reference_key is None
    ],
)
def test_web_mercator_strict_input_boundary_rejects_just_outside(
    migrated_database: None,
    monkeypatch: pytest.MonkeyPatch,
    control,
) -> None:
    world = seed_access_world()
    key = "test_crs_beyond_web_limit"
    fixture = spatial_fixtures._point_fixture(
        key,
        control.source_code,
        (("point-000001", control.source_xy),),
        FixtureProvenance.SYNTHETIC,
    )
    monkeypatch.setattr(
        spatial_fixtures,
        "REFERENCE_FIXTURES",
        MappingProxyType({**spatial_fixtures.REFERENCE_FIXTURES, key: fixture}),
    )
    with _client(world.owner_user_id) as client:
        path, source, _, pins = _setup(client, world, key)
        assert source["validation_state"] == "ready"
        before, counts = _rows(source["id"]), _counts(world)
        for suffix in ("", "/point-000001"):
            response = client.get(path + suffix, params={**pins, "response_crs": 3857})
            _error(response, 422, "TRANSFORMATION_OUT_OF_AREA")
        assert _rows(source["id"]) == before
        assert _counts(world) == counts
