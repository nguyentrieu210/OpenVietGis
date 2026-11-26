#!/usr/bin/env python3
"""Full E15 House Numbering differential parity (FastAPI oracle vs Rust) on isolated PostGIS.

Each mutating case runs once per side against that side's own fixtures in one shared
database; generated UUIDs and timestamps are normalized before comparison.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import psycopg

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity

API = "/api/v1"
UUID_RE = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
TS_RE = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?")
SIDES = ("o", "r")
PASSED: list[str] = []


def req(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def norm(value, extra: dict[str, str] | None = None):
    """Replace generated identifiers and clocks so oracle and Rust bodies compare."""
    extra = {"@o": "", "@r": "", **(extra or {})}
    if isinstance(value, dict):
        return {k: norm(v, extra) for k, v in value.items()}
    if isinstance(value, list):
        return [norm(v, extra) for v in value]
    if isinstance(value, str):
        for old, new in (extra or {}).items():
            value = value.replace(old, new)
        value = UUID_RE.sub("<uuid>", value)
        return TS_RE.sub("<ts>", value)
    return value


def body_of(resp):
    if not resp.content:
        return None
    try:
        return resp.json()
    except ValueError:
        return resp.text


def compare(
    label: str, e, a, *, extra: dict[str, str] | None = None, sort: bool = False, view=None
):
    req(
        e.status_code == a.status_code,
        f"{label}: status {e.status_code} != {a.status_code}; "
        f"oracle={e.text[:800]} rust={a.text[:800]}",
    )
    eb, ab = norm(body_of(e), extra), norm(body_of(a), extra)
    if view is not None:
        eb, ab = view(eb), view(ab)
    if sort and isinstance(eb, dict) and isinstance(eb.get("items"), list):
        eb["items"] = sorted(eb["items"], key=lambda x: json.dumps(x, sort_keys=True))
        ab["items"] = sorted(ab["items"], key=lambda x: json.dumps(x, sort_keys=True))
    diff = parity.difference(eb, ab)
    req(
        diff is None,
        f"{label}: {diff}\noracle={json.dumps(eb)[:2000]}\nrust={json.dumps(ab)[:2000]}",
    )
    PASSED.append(label)
    print(f"PASS {label}", flush=True)
    return e, a


def seed_source(owner_url: str, fx: dict, family: str, features: dict[str, str]) -> str:
    source_id = uuid4()
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """
            INSERT INTO spatial.data_sources (
                id, organization_id, project_id, name, kind, reference_key, fixture_version,
                manifest_digest, source_asset_id, lifecycle, validation_state, validation_reason,
                source_crs, stored_crs, schema_state, geometry_family, dimensions,
                feature_count_state, feature_count, extent_state, extent,
                catalog_revision, content_revision, schema_revision,
                created_at, updated_at, retired_at
            ) VALUES (
                %s,%s,%s,%s,'reference_fixture',%s,'hn-parity-v1',%s,NULL,'active','ready',NULL,
                3857,3857,'known',%s,'XY','known',%s,'available',%s::jsonb,
                1,%s,%s,now(),now(),NULL
            )
            """,
            (
                source_id,
                fx["org_a_id"],
                fx["project_a_id"],
                f"HN parity {family}",
                f"hn_parity_{source_id.hex[:12]}",
                "c" * 64,
                family,
                len(features),
                json.dumps(
                    {"min_x": 0, "min_y": -100, "max_x": 400, "max_y": 100, "crs_code": 3857}
                ),
                "a" * 64,
                "b" * 64,
            ),
        )
        for feature_id, wkt in features.items():
            conn.execute(
                """
                INSERT INTO spatial.reference_features (
                    organization_id,project_id,source_id,feature_id,attributes,geometry,
                    geometry_state,content_revision,schema_revision
                ) VALUES (%s,%s,%s,%s,'{}'::jsonb,ST_GeomFromText(%s,3857),'present',%s,%s)
                """,
                (
                    fx["org_a_id"],
                    fx["project_a_id"],
                    source_id,
                    feature_id,
                    wkt,
                    "a" * 64,
                    "b" * 64,
                ),
            )
    return str(source_id)


def make_member(owner_url: str, fx: dict, project_role: str, name: str):
    """Create an org member with a project grant and an authenticated session."""
    uid, lid, sid, mid = uuid4(), uuid4(), uuid4(), uuid4()
    token, digest, csrf = baseline.make_session_and_csrf(sid, baseline.DEFAULT_CSRF_SECRET)
    now = datetime.now(UTC)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """INSERT INTO identity.users
               (id, display_name, email_snapshot, status, created_at, updated_at)
               VALUES (%s, %s, %s, 'active', now(), now())""",
            (uid, name, f"{uid.hex[:8]}@example.test"),
        )
        conn.execute(
            """INSERT INTO identity.identity_links
               (id, user_id, issuer, subject, email_snapshot, email_verified,
                display_name_snapshot, created_at)
               VALUES (%s, %s, 'https://parity.example', %s, %s, true, %s, now())""",
            (lid, uid, str(uid), f"{uid.hex[:8]}@example.test", name),
        )
        conn.execute(
            """INSERT INTO identity.web_sessions
               (id, user_id, identity_link_id, secret_hash, issued_at, expires_at)
               VALUES (%s, %s, %s, %s, now(), now() + interval '1 hour')""",
            (sid, uid, lid, digest),
        )
        conn.execute(
            """INSERT INTO org.organization_memberships
               (id, organization_id, user_id, role, status, created_at, activated_at)
               VALUES (%s, %s, %s, 'member', 'active', now(), now())""",
            (mid, fx["org_a_id"], uid),
        )
        conn.execute(
            """INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, scope_kind,
                project_id, role, created_at, created_by_actor_type, created_by_actor_id
            ) VALUES (%s,%s,'membership',%s,'project',%s,%s,%s,'human',%s)""",
            (uuid4(), fx["org_a_id"], mid, fx["project_a_id"], project_role, now, fx["user_a_id"]),
        )
    return {
        "id": str(uid),
        "h": parity.session_headers(token),
        "csrf": parity.session_headers(token, **{"x-openvietgis-csrf": csrf}),
    }


class Ctx:
    def __init__(self, oracle, rust, root: str, owner_url: str, fx: dict) -> None:
        self.client = {"o": oracle, "r": rust}
        self.root = root
        self.owner_url = owner_url
        self.fx = fx
        self.h = parity.session_headers(fx["session_a_token"])
        self.csrf = parity.session_headers(
            fx["session_a_token"], **{"x-openvietgis-csrf": fx["csrf_a_token"]}
        )
        self.data: dict[str, dict] = {"o": {}, "r": {}}

    def call(self, side: str, method: str, path: str, *, headers=None, **kw):
        return self.client[side].request(
            method, f"{self.root}{path}", headers=headers or self.csrf, **kw
        )

    def pair(
        self,
        label: str,
        method: str,
        path,
        *,
        body=None,
        params=None,
        headers=None,
        extra=None,
        sort=False,
        status: int | None = None,
        view=None,
    ):
        """Run `method path` on both sides; `path`/`body`/`params` may be callables of side data."""
        resp = {}
        for side in SIDES:
            d = self.data[side]
            p = path(d) if callable(path) else path
            kw = {}
            b = body(d) if callable(body) else body
            if b is not None:
                kw["json"] = b
            q = params(d) if callable(params) else params
            if q is not None:
                kw["params"] = q
            resp[side] = self.call(side, method, p, headers=headers, **kw)
        compare(label, resp["o"], resp["r"], extra=extra, sort=sort, view=view)
        if status is not None:
            req(
                resp["r"].status_code == status, f"{label}: expected {status}, got {resp['r'].text}"
            )
        return resp["o"], resp["r"]

    def keep(self, key: str, o, r, field: str | None = None):
        self.data["o"][key] = o.json()[field] if field else o.json()
        self.data["r"][key] = r.json()[field] if field else r.json()


def batch_hierarchy(ctx: Ctx) -> None:
    for side in SIDES:
        d = ctx.data[side]
        d["s"] = side
        d["lines"] = seed_source(
            ctx.owner_url,
            ctx.fx,
            "LineString",
            {
                "line-1": "LINESTRING(0 0,100 0)",
                "line-2": "LINESTRING(100 0,200 0)",
                "line-4": "LINESTRING(200 0,300 0)",
                "line-9": "LINESTRING(0 80,10 80)",
            },
        )
        d["points"] = seed_source(
            ctx.owner_url,
            ctx.fx,
            "Point",
            {f"pt-{i}": f"POINT({20 * i} {-10 if i % 2 else 10})" for i in range(1, 9)},
        )
        d["polys"] = seed_source(
            ctx.owner_url,
            ctx.fx,
            "Polygon",
            {
                "poly-1": "POLYGON((0 0,10 0,10 10,0 10,0 0))",
            },
        )

    o, r = ctx.pair(
        "E15 create street route",
        "POST",
        "/routes",
        status=201,
        body=lambda d: {
            "name": "Đường Parity",
            "route_type": "street",
            "source_id": d["lines"],
            "feature_id": "line-1",
            "source_label": " manual fixture ",
            "status": "confirmed",
        },
    )
    ctx.keep("street", o, r)
    o, r = ctx.pair(
        "E15 create alley route with segments",
        "POST",
        "/routes",
        status=201,
        body=lambda d: {
            "name": "Hẻm 12/3A",
            "route_type": "alley",
            "parent_route_id": d["street"]["id"],
            "prefix": " 12/3a ",
            "source_id": d["lines"],
            "feature_id": "line-2",
            "direction_state": "reverse",
            "segments": [
                {"source_id": d["lines"], "feature_id": "line-2", "code": f"A1@{d['s']}"},
                {"source_id": d["lines"], "feature_id": "line-4", "code": f"A2@{d['s']}"},
            ],
        },
    )
    ctx.keep("alley", o, r)
    ctx.pair(
        "E15 create route invalid prefix",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "prefix": "ngõ 5",
            "source_id": d["lines"],
            "feature_id": "line-9",
        },
    )
    ctx.pair(
        "E15 create route unknown field",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["lines"],
            "feature_id": "line-9",
            "bogus": 1,
        },
    )
    ctx.pair(
        "E15 create route first segment mismatch",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["lines"],
            "feature_id": "line-9",
            "segments": [{"source_id": d["lines"], "feature_id": "line-1"}],
        },
    )
    ctx.pair(
        "E15 create route occupied segment",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["lines"],
            "feature_id": "line-2",
            "segments": [{"source_id": d["lines"], "feature_id": "line-2", "code": f"Z9@{d['s']}"}],
        },
    )
    ctx.pair(
        "E15 create route occupied code",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["lines"],
            "feature_id": "line-9",
            "segments": [{"source_id": d["lines"], "feature_id": "line-9", "code": f"A1@{d['s']}"}],
        },
    )
    ctx.pair(
        "E15 create route duplicate segments",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["lines"],
            "feature_id": "line-9",
            "segments": [
                {"source_id": d["lines"], "feature_id": "line-9"},
                {"source_id": d["lines"], "feature_id": "line-9"},
            ],
        },
    )
    ctx.pair(
        "E15 create route missing feature",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["lines"],
            "feature_id": "nope",
        },
    )
    ctx.pair(
        "E15 create route wrong geometry",
        "POST",
        "/routes",
        body=lambda d: {
            "name": "Bad",
            "source_id": d["points"],
            "feature_id": "pt-1",
        },
    )
    o, r = ctx.pair(
        "E15 create spare route",
        "POST",
        "/routes",
        status=201,
        body=lambda d: {
            "name": "Spare",
            "source_id": d["lines"],
            "feature_id": "line-9",
        },
    )
    ctx.keep("spare", o, r)
    ctx.pair(
        "E15 update route stale",
        "PUT",
        lambda d: f"/routes/{d['street']['id']}",
        body={
            "expected_revision": 9,
            "name": "X",
            "direction_state": "forward",
            "status": "draft",
        },
    )
    o, r = ctx.pair(
        "E15 update route",
        "PUT",
        lambda d: f"/routes/{d['street']['id']}",
        status=200,
        body={
            "expected_revision": 1,
            "name": " Đường Parity 2 ",
            "direction_state": "reverse",
            "status": "confirmed",
        },
    )
    ctx.keep("street", o, r)
    ctx.pair(
        "E15 update route missing",
        "PUT",
        f"/routes/{uuid4()}",
        body={
            "expected_revision": 1,
            "name": "X",
            "direction_state": "forward",
            "status": "draft",
        },
    )
    ctx.pair(
        "E15 update route invalid literal",
        "PUT",
        lambda d: f"/routes/{d['street']['id']}",
        body={"expected_revision": 2, "name": "X", "direction_state": "up", "status": "draft"},
    )
    ctx.pair("E15 delete route with child", "DELETE", lambda d: f"/routes/{d['street']['id']}")
    ctx.pair("E15 delete route", "DELETE", lambda d: f"/routes/{d['spare']['id']}", status=204)
    ctx.pair("E15 delete route missing", "DELETE", lambda d: f"/routes/{d['spare']['id']}")

    for key, name in (("area_a", "Phường A"), ("area_b", "Khu phố B")):
        o, r = ctx.pair(
            f"E15 create {key}",
            "POST",
            "/areas",
            status=201,
            body={"name": name, "kind": "current"},
        )
        ctx.keep(key, o, r)
    o, r = ctx.pair(
        "E15 update area parent",
        "PUT",
        lambda d: f"/areas/{d['area_b']['id']}",
        status=200,
        body=lambda d: {
            "expected_revision": 1,
            "name": " Khu phố B1 ",
            "kind": "custom",
            "parent_id": d["area_a"]["id"],
        },
    )
    ctx.keep("area_b", o, r)
    ctx.pair(
        "E15 update area cycle",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        body=lambda d: {
            "expected_revision": 1,
            "name": "A",
            "kind": "current",
            "parent_id": d["area_b"]["id"],
        },
    )
    ctx.pair(
        "E15 update area self parent",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        body=lambda d: {
            "expected_revision": 1,
            "name": "A",
            "kind": "current",
            "parent_id": d["area_a"]["id"],
        },
    )
    ctx.pair(
        "E15 update area stale",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        body={"expected_revision": 7, "name": "A", "kind": "current"},
    )
    ctx.pair(
        "E15 update area half link",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        body=lambda d: {
            "expected_revision": 1,
            "name": "A",
            "kind": "current",
            "source_id": d["polys"],
        },
    )
    ctx.pair(
        "E15 update area wrong geometry",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        body=lambda d: {
            "expected_revision": 1,
            "name": "A",
            "kind": "current",
            "source_id": d["points"],
            "feature_id": "pt-1",
        },
    )
    o, r = ctx.pair(
        "E15 update area linked polygon",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        status=200,
        body=lambda d: {
            "expected_revision": 1,
            "name": "Phường A",
            "kind": "current",
            "source_id": d["polys"],
            "feature_id": "poly-1",
        },
    )
    ctx.keep("area_a", o, r)
    ctx.pair(
        "E15 update area missing parent",
        "PUT",
        lambda d: f"/areas/{d['area_a']['id']}",
        body={"expected_revision": 2, "name": "A", "kind": "current", "parent_id": str(uuid4())},
    )
    ctx.pair(
        "E15 delete area with child",
        "DELETE",
        lambda d: f"/areas/{d['area_a']['id']}",
        params={"expected_revision": 2},
    )
    ctx.pair("E15 delete area missing revision", "DELETE", lambda d: f"/areas/{d['area_b']['id']}")
    ctx.pair(
        "E15 delete area stale",
        "DELETE",
        lambda d: f"/areas/{d['area_b']['id']}",
        params={"expected_revision": 5},
    )
    ctx.pair(
        "E15 delete area",
        "DELETE",
        lambda d: f"/areas/{d['area_b']['id']}",
        params={"expected_revision": 2},
        status=204,
    )
    ctx.pair(
        "E15 delete area missing",
        "DELETE",
        lambda d: f"/areas/{d['area_b']['id']}",
        params={"expected_revision": 2},
    )


def batch_buildings(ctx: Ctx) -> None:
    for i in range(1, 7):
        o, r = ctx.pair(
            f"E15 create building {i}",
            "POST",
            "/buildings",
            status=201,
            body=lambda d, i=i: {
                "route_id": d["street"]["id"] if i < 6 else d["alley"]["id"],
                "area_id": d["area_a"]["id"],
                "parcel_ref": f"P-{i}",
                "source_id": d["points"],
                "feature_id": f"pt-{i}",
                "eligibility": "eligible" if i != 5 else "unknown",
            },
        )
        ctx.keep(f"b{i}", o, r)
    ctx.pair(
        "E15 create building wrong geometry",
        "POST",
        "/buildings",
        body=lambda d: {
            "source_id": d["lines"],
            "feature_id": "line-1",
        },
    )
    ctx.pair(
        "E15 create building bad eligibility",
        "POST",
        "/buildings",
        body=lambda d: {
            "source_id": d["points"],
            "feature_id": "pt-7",
            "eligibility": "maybe",
        },
    )
    ctx.pair(
        "E15 update building stale",
        "PUT",
        lambda d: f"/buildings/{d['b1']['id']}",
        body=lambda d: {
            "expected_revision": 3,
            "source_id": d["points"],
            "feature_id": "pt-1",
        },
    )
    ctx.pair(
        "E15 update building missing route",
        "PUT",
        lambda d: f"/buildings/{d['b1']['id']}",
        body=lambda d: {
            "expected_revision": 1,
            "route_id": str(uuid4()),
            "source_id": d["points"],
            "feature_id": "pt-1",
        },
    )
    o, r = ctx.pair(
        "E15 update building",
        "PUT",
        lambda d: f"/buildings/{d['b1']['id']}",
        status=200,
        body=lambda d: {
            "expected_revision": 1,
            "route_id": d["street"]["id"],
            "area_id": d["area_a"]["id"],
            "parcel_ref": "  P-1A  ",
            "source_id": d["points"],
            "feature_id": "pt-1",
        },
    )
    ctx.keep("b1", o, r)
    ctx.pair(
        "E15 eligibility stale",
        "PATCH",
        lambda d: f"/buildings/{d['b2']['id']}/eligibility",
        body={"expected_revision": 4, "eligibility": "eligible"},
    )
    o, r = ctx.pair(
        "E15 eligibility",
        "PATCH",
        lambda d: f"/buildings/{d['b2']['id']}/eligibility",
        status=200,
        body={"expected_revision": 1, "eligibility": "unknown", "reason": None},
    )
    ctx.keep("b2", o, r)
    ctx.pair(
        "E15 plaque requires approved number",
        "PATCH",
        lambda d: f"/buildings/{d['b3']['id']}/plaque",
        body={"expected_revision": 1, "status": "installed"},
    )
    o, r = ctx.pair(
        "E15 plaque planned",
        "PATCH",
        lambda d: f"/buildings/{d['b3']['id']}/plaque",
        status=200,
        body={"expected_revision": 1, "status": "planned"},
    )
    ctx.keep("b3", o, r)
    ctx.pair(
        "E15 plaque missing building",
        "PATCH",
        f"/buildings/{uuid4()}/plaque",
        body={"expected_revision": 1, "status": "planned"},
    )

    def lineage(d, **over):
        body = {
            "relation_kind": "split",
            "operation_id": d.setdefault("op", str(uuid4())),
            "predecessor_building_ids": [d["b3"]["id"]],
            "successor_building_ids": [d["b4"]["id"], d["b5"]["id"]],
            "expected_revisions": {d["b3"]["id"]: 2, d["b4"]["id"]: 1, d["b5"]["id"]: 1},
        }
        body.update(over)
        return body

    ctx.pair(
        "E15 lineage invalid revision key",
        "POST",
        "/buildings/lineage",
        body=lambda d: lineage(d, expected_revisions={"not-a-uuid": 1}),
    )
    ctx.pair(
        "E15 lineage split cardinality",
        "POST",
        "/buildings/lineage",
        body=lambda d: lineage(d, successor_building_ids=[d["b4"]["id"]]),
    )
    ctx.pair(
        "E15 lineage revision set",
        "POST",
        "/buildings/lineage",
        body=lambda d: lineage(d, expected_revisions={d["b3"]["id"]: 2}),
    )
    ctx.pair(
        "E15 lineage stale",
        "POST",
        "/buildings/lineage",
        body=lambda d: lineage(
            d, expected_revisions={d["b3"]["id"]: 1, d["b4"]["id"]: 1, d["b5"]["id"]: 1}
        ),
    )
    ctx.pair("E15 lineage split", "POST", "/buildings/lineage", body=lineage, status=201, sort=True)
    ctx.pair("E15 lineage idempotent replay", "POST", "/buildings/lineage", body=lineage, sort=True)
    ctx.pair(
        "E15 lineage operation conflict",
        "POST",
        "/buildings/lineage",
        body=lambda d: lineage(
            d,
            successor_building_ids=[d["b4"]["id"], d["b6"]["id"]],
            expected_revisions={d["b3"]["id"]: 2, d["b4"]["id"]: 1, d["b6"]["id"]: 1},
        ),
    )
    ctx.pair(
        "E15 list lineage",
        "GET",
        lambda d: f"/buildings/{d['b4']['id']}/lineage",
        headers=ctx.h,
        sort=True,
    )
    ctx.pair(
        "E15 list lineage missing building", "GET", f"/buildings/{uuid4()}/lineage", headers=ctx.h
    )
    ctx.pair(
        "E15 retire removed building",
        "DELETE",
        lambda d: f"/buildings/{d['b3']['id']}",
        params={"expected_revision": 3},
    )
    ctx.pair(
        "E15 retire building stale",
        "DELETE",
        lambda d: f"/buildings/{d['b6']['id']}",
        params={"expected_revision": 2},
    )
    ctx.pair(
        "E15 retire building",
        "DELETE",
        lambda d: f"/buildings/{d['b6']['id']}",
        params={"expected_revision": 1},
        status=204,
    )
    ctx.pair(
        "E15 numbering targets tree",
        "GET",
        "/numbering-targets",
        headers=ctx.h,
        sort=True,
        params=lambda d: {"route_id": d["street"]["id"]},
    )
    ctx.pair(
        "E15 numbering targets root only",
        "GET",
        "/numbering-targets",
        headers=ctx.h,
        sort=True,
        params=lambda d: {
            "route_id": d["street"]["id"],
            "include_descendants": "false",
            "limit": 1,
        },
        view=lambda b: {**b, "items": len(b["items"])} if "items" in b else b,
    )
    ctx.pair(
        "E15 numbering targets bad limit",
        "GET",
        "/numbering-targets",
        headers=ctx.h,
        params=lambda d: {"route_id": d["street"]["id"], "limit": 0},
    )
    ctx.pair(
        "E15 numbering targets missing route",
        "GET",
        "/numbering-targets",
        headers=ctx.h,
        params={"route_id": str(uuid4())},
    )
    ctx.pair("E15 delete route with buildings", "DELETE", lambda d: f"/routes/{d['alley']['id']}")

    editor = make_member(ctx.owner_url, ctx.fx, "editor", "HN Editor")
    ctx.pair(
        "E15 editor plaque denied",
        "PATCH",
        lambda d: f"/buildings/{d['b2']['id']}/plaque",
        headers=editor["csrf"],
        body={"expected_revision": 2, "status": "planned"},
    )
    ctx.pair(
        "E15 editor lineage denied",
        "POST",
        "/buildings/lineage",
        headers=editor["csrf"],
        body=lambda d: lineage(d, operation_id=str(uuid4())),
    )
    o, r = ctx.pair(
        "E15 editor eligibility allowed",
        "PATCH",
        lambda d: f"/buildings/{d['b2']['id']}/eligibility",
        headers=editor["csrf"],
        status=200,
        body={"expected_revision": 2, "eligibility": "eligible"},
    )
    ctx.keep("b2", o, r)
    ctx.pair(
        "E15 write without CSRF",
        "PUT",
        lambda d: f"/routes/{d['street']['id']}",
        headers=ctx.h,
        body={"expected_revision": 2, "name": "X", "direction_state": "forward", "status": "draft"},
    )
    foreign = parity.session_headers(ctx.fx["session_b_token"])
    ctx.pair(
        "E15 foreign tenant lineage read",
        "GET",
        lambda d: f"/buildings/{d['b4']['id']}/lineage",
        headers=foreign,
    )


def seed_number(ctx: Ctx, d: dict, building: str, route: str, number: str, kind="proposed"):
    with psycopg.connect(ctx.owner_url) as conn:
        conn.execute(
            """
            INSERT INTO house_numbering.address_numbers (
                id, organization_id, project_id, building_id, route_id, number_text, kind,
                side, measure_m, rule_id, revision, created_by_actor_id, updated_by_actor_id,
                created_at, updated_at
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,'right',12.5,%s,1,%s,%s,now(),now())
            """,
            (
                uuid4(),
                ctx.fx["org_a_id"],
                ctx.fx["project_a_id"],
                d[building]["id"],
                d[route]["id"],
                number,
                kind,
                d["rule"]["id"],
                ctx.fx["user_a_id"],
                ctx.fx["user_a_id"],
            ),
        )


def batch_proposals(ctx: Ctx) -> None:
    for key, body in (
        ("rule", {"version": "auth", "status": "authoritative", "authority_ref": "QD-1"}),
        ("rule_draft", {"version": "draft"}),
    ):
        o, r = ctx.pair(
            f"E15 create {key}",
            "POST",
            "/rules",
            status=201,
            body=lambda d, body=body: {**body, "version": f"{body['version']}@{d['s']}"},
        )
        ctx.keep(key, o, r)
    for side in SIDES:
        d = ctx.data[side]
        seed_number(ctx, d, "b1", "street", "2")
        seed_number(ctx, d, "b2", "street", "4")
        seed_number(ctx, d, "b4", "street", "6")

    def create(d, buildings, rule="rule", kind="route", scope=None):
        return {
            "scope_kind": kind,
            "scope_id": scope or d["street"]["id"],
            "rule_id": d[rule]["id"],
            "building_ids": [d[b]["id"] for b in buildings],
        }

    o, r = ctx.pair(
        "E15 create proposal",
        "POST",
        "/proposals",
        status=201,
        body=lambda d: create(d, ["b1", "b2"]),
        sort=True,
    )
    ctx.keep("p1", o, r, "proposal")
    o, r = ctx.pair(
        "E15 create proposal draft rule",
        "POST",
        "/proposals",
        status=201,
        body=lambda d: create(d, ["b4"], rule="rule_draft"),
    )
    ctx.keep("p2", o, r, "proposal")
    ctx.pair("E15 proposal missing number", "POST", "/proposals", body=lambda d: create(d, ["b5"]))
    ctx.pair(
        "E15 proposal missing area",
        "POST",
        "/proposals",
        body=lambda d: create(d, ["b5"], kind="area", scope=str(uuid4())),
    )
    ctx.pair(
        "E15 proposal missing route",
        "POST",
        "/proposals",
        body=lambda d: create(d, ["b5"], scope=str(uuid4())),
    )
    ctx.pair(
        "E15 proposal missing rule",
        "POST",
        "/proposals",
        body=lambda d: {**create(d, ["b5"]), "rule_id": str(uuid4())},
    )
    ctx.pair("E15 proposal empty buildings", "POST", "/proposals", body=lambda d: create(d, []))
    ctx.pair("E15 list proposals", "GET", "/proposals", headers=ctx.h)
    ctx.pair(
        "E15 list proposals draft page",
        "GET",
        "/proposals",
        headers=ctx.h,
        params={"status": "draft", "limit": 1, "offset": 1},
    )
    ctx.pair(
        "E15 list proposals bad status",
        "GET",
        "/proposals",
        headers=ctx.h,
        params={"status": "rejected"},
    )
    ctx.pair(
        "E15 list proposals bad limit", "GET", "/proposals", headers=ctx.h, params={"limit": 0}
    )
    ctx.pair(
        "E15 get proposal", "GET", lambda d: f"/proposals/{d['p1']['id']}", headers=ctx.h, sort=True
    )
    ctx.pair("E15 get proposal missing", "GET", f"/proposals/{uuid4()}", headers=ctx.h)

    def move(label, key, rev, action, comment=None, headers=None, status=None):
        body = {"expected_revision": rev, "action": action}
        if comment is not None:
            body["comment"] = comment
        return ctx.pair(
            label,
            "POST",
            lambda d: f"/proposals/{d[key]['id']}/transition",
            body=body,
            headers=headers,
            status=status,
        )

    move("E15 proposal submit stale", "p1", 5, "submit")
    move("E15 proposal approve draft invalid", "p1", 1, "approve")
    move("E15 proposal submit", "p1", 1, "submit", status=200)
    editor = make_member(ctx.owner_url, ctx.fx, "editor", "HN Proposal Editor")
    move("E15 proposal editor approve denied", "p1", 2, "approve", headers=editor["csrf"])
    move("E15 proposal withdraw needs reason", "p1", 2, "withdraw", comment="  ")
    move("E15 proposal reject", "p1", 2, "reject", comment="sai số nhà", status=200)
    move("E15 proposal resubmit", "p1", 3, "submit", status=200)
    move("E15 proposal approve", "p1", 4, "approve", comment="đồng ý", status=200)
    move("E15 proposal approve twice", "p1", 5, "approve")
    move("E15 proposal p2 submit", "p2", 1, "submit", status=200)
    move("E15 proposal approve draft rule", "p2", 2, "approve")
    move("E15 proposal withdraw", "p2", 2, "withdraw", comment="rút lại", status=200)
    ctx.pair(
        "E15 get approved proposal",
        "GET",
        lambda d: f"/proposals/{d['p1']['id']}",
        headers=ctx.h,
        sort=True,
    )

    for side in SIDES:
        seed_number(ctx, ctx.data[side], "b5", "street", "2")
    o, r = ctx.pair(
        "E15 create proposal occupied",
        "POST",
        "/proposals",
        status=201,
        body=lambda d: create(d, ["b5"]),
    )
    ctx.keep("p3", o, r, "proposal")
    move("E15 proposal submit occupied number", "p3", 1, "submit")

    ctx.pair(
        "E15 plaque installed with approved number",
        "PATCH",
        lambda d: f"/buildings/{d['b1']['id']}/plaque",
        body={"expected_revision": 2, "status": "installed"},
        status=200,
    )
    ctx.pair(
        "E15 retire building official number",
        "DELETE",
        lambda d: f"/buildings/{d['b1']['id']}",
        params={"expected_revision": 3},
    )
    ctx.pair(
        "E15 update route direction locked",
        "PUT",
        lambda d: f"/routes/{d['street']['id']}",
        body={
            "expected_revision": 2,
            "name": "X",
            "direction_state": "forward",
            "status": "confirmed",
        },
    )
    ctx.pair(
        "E15 update building route locked",
        "PUT",
        lambda d: f"/buildings/{d['b1']['id']}",
        body=lambda d: {
            "expected_revision": 3,
            "route_id": d["alley"]["id"],
            "source_id": d["points"],
            "feature_id": "pt-1",
        },
    )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--candidate", type=Path, required=True)
    args = ap.parse_args()
    binary = args.candidate.resolve()
    req(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e15-full-") as d:
        work = Path(d)
        with parity.database(admin_url) as (owner_url, runtime_url):
            art = work / "artifacts"
            art.mkdir()
            parity.configure_python(owner_url, runtime_url, art)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            from modules.house_numbering.migrate import upgrade as upgrade_house_numbering

            upgrade_house_numbering()
            fx = baseline.seed_benchmark_database(
                owner_url, seed=20261126, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
            )

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            org = f"{API}/organizations/{fx['org_a_id']}"
            project = f"{org}/projects/{fx['project_a_id']}"
            root = f"{project}/modules/house-numbering"
            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    ctx = Ctx(oracle, rust, root, owner_url, fx)
                    registered = oracle.post(
                        f"{org}/extensions/official/house_numbering/register", headers=ctx.csrf
                    )
                    req(registered.status_code == 201, registered.text)
                    activated = oracle.post(
                        f"{project}/extensions/activations",
                        json={"manifest_id": registered.json()["id"]},
                        headers=ctx.csrf,
                    )
                    req(activated.status_code == 201, activated.text)
                    batch_hierarchy(ctx)
                    batch_buildings(ctx)
                    batch_proposals(ctx)
            finally:
                get_engine().dispose()

    print(f"E15 full parity PASS ({len(PASSED)} cases)", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
