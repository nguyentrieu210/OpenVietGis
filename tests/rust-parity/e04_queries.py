"""E04 differential cases: typed layer query and saved queries (FastAPI oracle vs Rust)."""

from __future__ import annotations

import math
from datetime import datetime
from uuid import uuid4

import httpx
from openvietgis_api.security.csrf import make_csrf_token

API = "/api/v1"
COOKIE_NAME = "ovg_session"
MERCATOR = 20037508.342789244


def _require(condition, detail):
    if not condition:
        raise AssertionError(detail)


def _mercator(lon, lat):
    y = math.log(math.tan((90.0 + lat) * math.pi / 360.0)) * MERCATOR / math.pi
    return [lon * MERCATOR / 180.0, y]


def _box(west, south, east, north, project=None):
    ring = [[west, south], [east, south], [east, north], [west, north], [west, south]]
    if project is not None:
        ring = [project(x, y) for x, y in ring]
    return {"type": "Polygon", "coordinates": [ring]}


class Requests:
    def __init__(self, suite, difference):
        self.suite = suite
        self.difference = difference

    def run(
        self,
        label,
        method,
        path,
        status,
        *,
        token,
        csrf=None,
        body=None,
        params=None,
        normalize=None,
        rust_path=None,
    ):
        """Send the same request to both APIs and compare status, JSON and Cache-Control."""
        headers = {}
        if token is not None:
            headers["cookie"] = f"{COOKIE_NAME}={token}"
        if csrf is not None:
            headers["x-openvietgis-csrf"] = csrf
        suite = self.suite
        expected_body = actual_body = None
        try:
            expected = suite.oracle.request(
                method, path, headers=headers, json=body, params=params
            )
            actual = suite.rust.request(
                method, rust_path or path, headers=headers, json=body, params=params
            )
            _require(
                expected.status_code == status,
                f"FastAPI status {expected.status_code}, wanted {status}",
            )
            _require(
                actual.status_code == status,
                f"Rust status {actual.status_code}, wanted {status}: {actual.text[:300]}",
            )
            if status == 204:
                _require(actual.content == b"", "Rust 204 response has a body")
            else:
                expected_body = expected.json()
                actual_body = actual.json()
                norm = normalize or (lambda value: value)
                mismatch = self.difference(norm(expected_body), norm(actual_body))
                _require(mismatch is None, mismatch)
            _require(
                actual.headers.get("cache-control") == expected.headers.get("cache-control"),
                "Cache-Control differs from FastAPI",
            )
        except (AssertionError, ValueError, httpx.HTTPError) as exc:
            suite.failures.append(label)
            print(f"FAIL {label}: {exc}", flush=True)
        else:
            suite.passed += 1
            print(f"PASS {label}", flush=True)
        return expected_body, actual_body


def _saved(body):
    """Saved definitions differ only by server-generated identity and clock."""
    for key in ("created_at", "updated_at"):
        _require(
            datetime.fromisoformat(body[key]).utcoffset() is not None,
            f"{key} must be an ISO 8601 timestamp with offset",
        )
    return {k: v for k, v in body.items() if k not in ("id", "created_at", "updated_at")}


def compare_queries(suite, difference, fixture, surveyor, csrf_secret):
    req = Requests(suite, difference)
    owner, csrf = fixture["session_a_token"], fixture["csrf_a_token"]
    root = f"{API}/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
    layers = fixture["layers"]
    polygons = layers["epsg4326_1k"]
    points = layers["edit_4326"]
    query = f"{root}/layers/{polygons['layer_id']}/query"
    point_query = f"{root}/layers/{points['layer_id']}/query"

    def post(label, body, status=200, path=query, **kwargs):
        kwargs.setdefault("token", owner)
        kwargs.setdefault("csrf", csrf)
        return req.run(label, "POST", path, status, body=body, **kwargs)[0]

    # Typed attribute semantics, ordering and paging.
    post("E04 text equality", {"attribute_filter": {"field": "ward", "op": "eq", "value": "HN-05"}})
    complex_filter = {
        "op": "and",
        "conditions": [
            {"field": "metric_val", "op": "between", "value": [1000, 8000]},
            {
                "op": "or",
                "conditions": [
                    {"field": "ward", "op": "in", "value": ["HN-01", "HN-02", "HN-03"]},
                    {"op": "not", "condition": {"field": "active", "op": "eq", "value": True}},
                ],
            },
            {"field": "code", "op": "is_null", "value": False},
        ],
    }
    first = post(
        "E04 nested and/or/not/in/between sorted desc",
        {
            "attribute_filter": complex_filter,
            "sort_field": "metric_val",
            "sort_direction": "desc",
            "limit": 7,
            "expected_content_revision": polygons["content_revision"],
            "expected_schema_revision": polygons["schema_revision"],
        },
    )
    if first and first.get("query_fingerprint"):
        post(
            "E04 second page pinned by Python fingerprint",
            {
                "attribute_filter": complex_filter,
                "sort_field": "metric_val",
                "sort_direction": "desc",
                "limit": 7,
                "offset": 7,
                "expected_query_fingerprint": first["query_fingerprint"],
            },
        )
    post(
        "E04 literal prefix and contains",
        {
            "attribute_filter": {
                "op": "or",
                "conditions": [
                    {"field": "code", "op": "prefix", "value": "P4326-0001"},
                    {"field": "code", "op": "contains", "value": "%_"},
                ],
            },
            "sort_field": "code",
            "limit": 25,
        },
    )
    post(
        "E04 integer comparison ascending text sort",
        {
            "attribute_filter": {"field": "metric_val", "op": "lt", "value": 300},
            "sort_field": "ward",
        },
    )
    post("E04 no filter default page", {"limit": 3})

    # Spatial filters with explicit CRS, transformations and references.
    post(
        "E04 drawn polygon intersects EPSG:4326",
        {
            "spatial_filter": {
                "predicate": "intersects",
                "geometry": _box(105.8, 20.9, 105.9, 21.0),
                "crs_code": 4326,
            },
            "limit": 50,
        },
    )
    post(
        "E04 drawn polygon within EPSG:3857 transformed to 4326",
        {
            "spatial_filter": {
                "predicate": "within",
                "geometry": _box(105.7, 20.9, 105.95, 21.1, _mercator),
                "crs_code": 3857,
            },
            "attribute_filter": {"field": "active", "op": "eq", "value": True},
            "limit": 20,
        },
    )
    post(
        "E04 point layer bbox over whole extent",
        {
            "spatial_filter": {
                "predicate": "intersects",
                "geometry": _box(104.0, 10.0, 108.0, 22.0),
                "crs_code": 4326,
            },
            "limit": 500,
        },
        path=point_query,
    )
    post(
        "E04 reference layer set (contains)",
        {
            "spatial_filter": {"predicate": "contains", "reference_layer_id": points["layer_id"]},
            "limit": 10,
        },
    )
    post(
        "E04 reference single feature",
        {
            "spatial_filter": {
                "predicate": "intersects",
                "reference_layer_id": polygons["layer_id"],
                "reference_feature_id": "poly-000001",
            }
        },
        path=point_query,
    )
    post(
        "E04 reference feature missing",
        {
            "spatial_filter": {
                "predicate": "intersects",
                "reference_layer_id": polygons["layer_id"],
                "reference_feature_id": "poly-999999",
            }
        },
        404,
        path=point_query,
    )
    post(
        "E04 reference set above 500 features",
        {"spatial_filter": {"predicate": "within", "reference_layer_id": polygons["layer_id"]}},
        422,
        path=point_query,
    )
    vn2000 = layers.get("vn2000_5899_1k")
    if vn2000:
        vn_query = f"{root}/layers/{vn2000['layer_id']}/query"
        post(
            "E04 native EPSG:5899 attribute query",
            {"attribute_filter": {"field": "metric_val", "op": "gte", "value": 40000}},
            path=vn_query,
        )
        post(
            "E04 fixture-only transformation refused",
            {
                "spatial_filter": {
                    "predicate": "intersects",
                    "geometry": _box(107.15, 20.70, 107.18, 20.73),
                    "crs_code": 4326,
                }
            },
            422,
            path=vn_query,
        )
    not_ready = layers.get("vn2000_3405_1k")
    if not_ready:
        post(
            "E04 source not ready",
            {},
            422,
            path=f"{root}/layers/{not_ready['layer_id']}/query",
        )

    # Validation, pins and bounds.
    for label, body, status in (
        ("E04 unknown field", {"attribute_filter": {"field": "x", "op": "eq", "value": 1}}, 422),
        (
            "E04 wrong value type",
            {"attribute_filter": {"field": "metric_val", "op": "eq", "value": "5"}},
            422,
        ),
        (
            "E04 boolean range rejected",
            {"attribute_filter": {"field": "active", "op": "gt", "value": False}},
            422,
        ),
        ("E04 unknown sort field", {"sort_field": "nope"}, 422),
        ("E04 invalid sort direction", {"sort_direction": "up"}, 422),
        ("E04 page without fingerprint", {"offset": 5}, 409),
        ("E04 stale fingerprint", {"expected_query_fingerprint": "0" * 64}, 409),
        ("E04 stale content pin", {"expected_content_revision": "0" * 64}, 409),
        ("E04 malformed pin", {"expected_schema_revision": "ABC"}, 422),
        ("E04 page size above limit", {"limit": 501}, 422),
        ("E04 non-strict page size", {"limit": "5"}, 422),
        ("E04 unknown request key", {"filter": {}}, 422),
        ("E04 array body rejected", [], 422),
        ("E04 array positional body rejected", [None, None, None, None, None, None, "asc", 5, 0], 422),
        ("E04 offset beyond HTTP bound", {"offset": 25001}, 422),
        ("E04 unknown injection-shaped field", {"attribute_filter": {"field": "code) OR TRUE --", "op": "eq", "value": "x"}}, 422),
        ("E04 injection-shaped literal remains data", {"attribute_filter": {"field": "code", "op": "eq", "value": "' OR TRUE --"}}, 200),
        (
            "E04 unsupported predicate",
            {"spatial_filter": {"predicate": "touches", "geometry": {}, "crs_code": 4326}},
            422,
        ),
        (
            "E04 drawn area without CRS",
            {"spatial_filter": {"predicate": "intersects", "geometry": _box(105, 21, 106, 22)}},
            422,
        ),
        (
            "E04 unreadable CRS",
            {
                "spatial_filter": {
                    "predicate": "intersects",
                    "geometry": _box(105, 21, 106, 22),
                    "crs_code": 3405,
                }
            },
            422,
        ),
    ):
        post(label, body, status)

    # Authorization and tenant boundaries.
    post("E04 missing layer", {}, 404, path=f"{root}/layers/{uuid4()}/query")
    post(
        "E04 foreign tenant layer id",
        {},
        404,
        path=f"{root}/layers/{layers['tenant_b_1k']['layer_id']}/query",
    )
    post(
        "E04 other tenant session",
        {},
        404,
        token=fixture["session_b_token"],
        csrf=fixture["csrf_b_token"],
    )
    post("E04 unauthenticated", {}, 401, token=None, csrf=None)
    post("E04 CSRF required", {}, 403, csrf="invalid")
    post(
        "E04 GIS-scoped member denied",
        {},
        403,
        token=surveyor,
        csrf=make_csrf_token(surveyor, csrf_secret),
    )

    # Saved query definitions.
    saved = f"{root}/saved-queries"
    definition = {
        "layer_id": polygons["layer_id"],
        "name": "  Phường HN-05  ",
        "description": "Truy vấn mẫu",
        "attribute_filter": {"field": "ward", "op": "eq", "value": "HN-05"},
        "sort_field": "metric_val",
        "sort_direction": "desc",
    }
    oracle_created, rust_created = req.run(
        "E04 create saved query",
        "POST",
        saved,
        201,
        token=owner,
        csrf=csrf,
        body=definition,
        normalize=_saved,
    )
    req.run("E04 list saved queries", "GET", saved, 200, token=owner)
    req.run(
        "E04 list saved queries by layer",
        "GET",
        saved,
        200,
        token=owner,
        params={"layer_id": points["layer_id"]},
    )
    req.run(
        "E04 list saved queries invalid layer id",
        "GET",
        saved,
        422,
        token=owner,
        params={"layer_id": "nope"},
    )
    for label, body, status in (
        ("E04 saved query blank name", definition | {"name": "   "}, 422),
        (
            "E04 saved query invalid filter",
            definition | {"attribute_filter": {"field": "x", "op": "eq", "value": 1}},
            422,
        ),
        ("E04 saved query missing layer", definition | {"layer_id": str(uuid4())}, 404),
        ("E04 saved query unknown key", definition | {"extra": 1}, 422),
    ):
        req.run(label, "POST", saved, status, token=owner, csrf=csrf, body=body)
    req.run("E04 saved query CSRF", "POST", saved, 403, token=owner, csrf="x", body=definition)
    req.run(
        "E04 saved query scoped member denied",
        "GET",
        saved,
        403,
        token=surveyor,
    )
    if oracle_created and rust_created:
        req.run(
            "E04 delete saved query",
            "DELETE",
            f"{saved}/{oracle_created['id']}",
            204,
            token=owner,
            csrf=csrf,
            rust_path=f"{saved}/{rust_created['id']}",
        )
        req.run(
            "E04 delete saved query again",
            "DELETE",
            f"{saved}/{oracle_created['id']}",
            404,
            token=owner,
            csrf=csrf,
        )
    req.run(
        "E04 delete saved query CSRF",
        "DELETE",
        f"{saved}/{uuid4()}",
        403,
        token=owner,
        csrf=None,
    )
