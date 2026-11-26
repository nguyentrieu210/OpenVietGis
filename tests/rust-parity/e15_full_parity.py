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
HEX_RE = re.compile(r"[0-9a-f]{64}")
SIDES = ("o", "r")
PASSED: list[str] = []


def req(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def norm(value, extra: dict[str, str] | None = None):
    """Replace generated identifiers and clocks so oracle and Rust bodies compare."""
    extra = {"@o": "", "@r": "", **(extra or {})}
    if isinstance(value, dict):
        return {UUID_RE.sub("<uuid>", k): norm(v, extra) for k, v in value.items()}
    if isinstance(value, list):
        return [norm(v, extra) for v in value]
    if isinstance(value, str):
        for old, new in (extra or {}).items():
            value = value.replace(old, new)
        value = UUID_RE.sub("<uuid>", HEX_RE.sub("<sha256>", value))
        return TS_RE.sub("<ts>", value)
    return value


def body_of(resp):
    if not resp.content:
        return None
    try:
        return resp.json()
    except ValueError:
        return resp.text


# Per-case runner from run.py (064-ci-speed): a mismatch prints FAIL and the run continues.
case = getattr(parity, "case", lambda compare: compare)


def status_label(label: str, status: int) -> str:
    """Negative cases carry their HTTP status in the label."""
    return f"{label} {status}" if status >= 400 and not label.endswith(str(status)) else label


def compare(label: str, e, a, **kwargs):
    return _compare(status_label(label, e.status_code), e, a, **kwargs)


@case
def _compare(
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


def seed_source(
    owner_url: str, fx: dict, family: str, features: dict[str, str], attributes=None
) -> str:
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
                ) VALUES (%s,%s,%s,%s,%s::jsonb,ST_GeomFromText(%s,3857),'present',%s,%s)
                """,
                (
                    fx["org_a_id"],
                    fx["project_a_id"],
                    source_id,
                    feature_id,
                    json.dumps((attributes or {}).get(feature_id, {})),
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


def batch_field(ctx: Ctx) -> None:
    from datetime import date, timedelta

    today = date.today()
    editor = make_member(ctx.owner_url, ctx.fx, "editor", "HN Field Editor")
    owner = ctx.fx["user_a_id"]

    def assignment(d, **over):
        body = {
            "scope_kind": "route",
            "scope_id": d["street"]["id"],
            "assignee_actor_id": owner,
            "starts_on": str(today - timedelta(days=1)),
            "ends_on": str(today + timedelta(days=30)),
            "target_count": 4,
        }
        body.update(over)
        return body

    o, r = ctx.pair("E15 create assignment", "POST", "/assignments", body=assignment, status=201)
    ctx.keep("asg", o, r)
    ctx.pair(
        "E15 assignment end before start",
        "POST",
        "/assignments",
        body=lambda d: assignment(d, ends_on=str(today - timedelta(days=9))),
    )
    ctx.pair(
        "E15 assignment bad scope",
        "POST",
        "/assignments",
        body=lambda d: assignment(d, scope_kind="parcel"),
    )
    ctx.pair(
        "E15 assignment negative target",
        "POST",
        "/assignments",
        body=lambda d: assignment(d, target_count=-1),
    )
    ctx.pair(
        "E15 assignment editor denied",
        "POST",
        "/assignments",
        body=assignment,
        headers=editor["csrf"],
    )
    ctx.pair(
        "E15 reassign stale",
        "PATCH",
        lambda d: f"/assignments/{d['asg']['id']}",
        body={"expected_revision": 3, "assignee_actor_id": editor["id"]},
    )
    ctx.pair(
        "E15 reassign",
        "PATCH",
        lambda d: f"/assignments/{d['asg']['id']}",
        status=200,
        body={"expected_revision": 1, "assignee_actor_id": editor["id"]},
    )
    ctx.pair("E15 list assignments", "GET", "/assignments", headers=ctx.h)

    def op(d, client, **over):
        body = {"client_uuid": d.setdefault(client, str(uuid4())), "condition": "house"}
        body.update(over)
        return body

    def sync(label, ops, headers=None, status=None):
        return ctx.pair(
            label,
            "POST",
            "/survey/sync",
            headers=headers,
            status=status,
            body=lambda d: {"operations": ops(d)},
        )

    def first(d):
        return [
            op(d, "c1", building_id=d["b2"]["id"], x=41, y=10, crs_code=3857, gps_accuracy_m=5),
            op(
                d,
                "c2",
                building_id=d["b4"]["id"],
                condition="vacant",
                x=80.5,
                y=12,
                crs_code=3857,
            ),
            op(d, "c4", condition="other"),
        ]

    sync("E15 survey sync", first, status=200)
    # Replays written by either runtime must recognize each other's fingerprints.
    import hashlib

    for side in SIDES:
        d = ctx.data[side]
        semantic = {
            "building_id": d["b2"]["id"],
            "condition": "house",
            "x": 41,
            "y": 10,
            "crs_code": 3857,
            "gps_accuracy_m": 5,
            "photo_artifact_ids": [],
            "recorded_by_actor_id": owner,
        }
        expected = hashlib.sha256(
            json.dumps(semantic, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
        ).hexdigest()
        with psycopg.connect(ctx.owner_url) as conn:
            stored = conn.execute(
                "SELECT payload_fingerprint FROM house_numbering.surveys WHERE client_uuid=%s",
                (d["c1"],),
            ).fetchone()[0]
        req(stored == expected, f"survey fingerprint mismatch on {side}: {stored} != {expected}")
    PASSED.append("E15 survey payload fingerprint")
    print("PASS E15 survey payload fingerprint", flush=True)
    sync("E15 survey sync replay", first, status=200)
    sync(
        "E15 survey sync payload conflict",
        lambda d: [op(d, "c1", building_id=d["b2"]["id"], condition="other")],
    )
    sync("E15 survey sync bad condition", lambda d: [op(d, "c9", condition="ruin")])
    sync("E15 survey sync bad client uuid", lambda d: [{"client_uuid": "nope"}])
    sync("E15 survey sync accuracy without point", lambda d: [op(d, "c9", gps_accuracy_m=3)])
    sync("E15 survey sync partial point", lambda d: [op(d, "c9", x=1, y=2)])
    sync(
        "E15 survey sync point without building",
        lambda d: [op(d, "c9", x=1, y=2, crs_code=3857)],
    )
    sync(
        "E15 survey sync invalid gps",
        lambda d: [op(d, "c9", building_id=d["b2"]["id"], x="abc", y=2, crs_code=3857)],
    )
    sync(
        "E15 survey sync unknown crs",
        lambda d: [op(d, "c9", building_id=d["b2"]["id"], x=1, y=2, crs_code=999999)],
    )
    sync(
        "E15 survey sync missing photo",
        lambda d: [op(d, "c9", photo_artifact_ids=[str(uuid4())])],
    )
    sync(
        "E15 survey sync duplicate photos",
        lambda d: [op(d, "c9", photo_artifact_ids=[d["b2"]["id"], d["b2"]["id"]])],
    )
    sync(
        "E15 survey sync delegated non member",
        lambda d: [op(d, "c9", recorded_by_actor_id=str(uuid4()))],
    )
    sync(
        "E15 survey sync editor delegation denied",
        lambda d: [op(d, "c9", recorded_by_actor_id=owner)],
        headers=editor["csrf"],
    )
    sync("E15 survey sync empty", lambda d: [])
    ctx.pair("E15 list surveys", "GET", "/surveys", headers=ctx.h, params={"state": "submitted"})
    ctx.pair("E15 list surveys all", "GET", "/surveys", headers=ctx.h)

    def survey_id(d, client):
        with psycopg.connect(ctx.owner_url) as conn:
            row = conn.execute(
                "SELECT id FROM house_numbering.surveys WHERE client_uuid=%s", (d[client],)
            ).fetchone()
            return str(row[0])

    for side in SIDES:
        d = ctx.data[side]
        d["s1"], d["s2"] = survey_id(d, "c1"), survey_id(d, "c2")
    ctx.pair(
        "E15 review survey editor denied",
        "POST",
        lambda d: f"/surveys/{d['s1']}/review",
        headers=editor["csrf"],
        body={"expected_revision": 1, "decision": "accepted"},
    )
    ctx.pair(
        "E15 review survey",
        "POST",
        lambda d: f"/surveys/{d['s1']}/review",
        status=200,
        body={"expected_revision": 1, "decision": "accepted"},
    )
    ctx.pair(
        "E15 review survey stale",
        "POST",
        lambda d: f"/surveys/{d['s1']}/review",
        body={"expected_revision": 1, "decision": "rejected"},
    )
    ctx.pair(
        "E15 review survey missing",
        "POST",
        f"/surveys/{uuid4()}/review",
        body={"expected_revision": 1, "decision": "accepted"},
    )
    ctx.pair(
        "E15 review survey rejected",
        "POST",
        lambda d: f"/surveys/{d['s2']}/review",
        status=200,
        body={"expected_revision": 1, "decision": "rejected"},
    )
    sync(
        "E15 survey sync building already accepted",
        lambda d: [op(d, "c5", building_id=d["b2"]["id"], condition="vacant")],
    )
    ctx.pair("E15 progress", "GET", "/progress", headers=ctx.h)

    attrs = {
        "s-1": {"ap": "Ấp 1", "ten_duong": "Đường A", "xu_ly": "da_xu_ly", "trang_thai": "lech"},
        "s-2": {
            "ap": "Ấp 1",
            "ten_duong": "Đường B",
            "xu_ly": "chua_xu_ly",
            "trang_thai": "lech",
        },
        "s-3": {"ap": "Ấp 2", "xu_ly": "", "trang_thai": "khop"},
    }
    for side in SIDES:
        ctx.data[side]["survey_src"] = seed_source(
            ctx.owner_url,
            ctx.fx,
            "Point",
            {"s-1": "POINT(1 1)", "s-2": "POINT(2 2)", "s-3": "POINT(3 3)"},
            attrs,
        )

    def sign(d, ap, road=None, **over):
        body = {"source_ids": [d["survey_src"]], "ap": ap, "ten_duong": road}
        body.update(over)
        return body

    o, r = ctx.pair(
        "E15 sign survey road",
        "POST",
        "/survey-signoffs",
        status=201,
        body=lambda d: sign(d, " Ấp 1 ", "Đường A", note=" ok "),
    )
    ctx.data["o"]["sig"], ctx.data["r"]["sig"] = o.json()["items"][0], r.json()["items"][0]
    ctx.pair(
        "E15 sign survey hamlet incomplete",
        "POST",
        "/survey-signoffs",
        body=lambda d: sign(d, "Ấp 1"),
    )
    ctx.pair(
        "E15 sign survey empty scope", "POST", "/survey-signoffs", body=lambda d: sign(d, "Ấp 9")
    )
    ctx.pair(
        "E15 sign survey twice",
        "POST",
        "/survey-signoffs",
        body=lambda d: sign(d, "Ấp 1", "Đường A"),
    )
    ctx.pair(
        "E15 sign survey hamlet 2",
        "POST",
        "/survey-signoffs",
        status=201,
        body=lambda d: sign(d, "Ấp 2"),
    )
    ctx.pair("E15 sign survey blank ap", "POST", "/survey-signoffs", body=lambda d: sign(d, "   "))
    ctx.pair(
        "E15 sign survey editor denied",
        "POST",
        "/survey-signoffs",
        headers=editor["csrf"],
        body=lambda d: sign(d, "Ấp 2"),
    )
    ctx.pair(
        "E15 list signoffs",
        "GET",
        "/survey-signoffs",
        headers=ctx.h,
        params=lambda d: {"source_id": d["survey_src"]},
    )
    ctx.pair("E15 list signoffs missing source", "GET", "/survey-signoffs", headers=ctx.h)
    ctx.pair(
        "E15 revoke signoff short reason",
        "POST",
        lambda d: f"/survey-signoffs/{d['sig']['id']}/revoke",
        body={"reason": " ab "},
    )
    ctx.pair(
        "E15 revoke signoff",
        "POST",
        lambda d: f"/survey-signoffs/{d['sig']['id']}/revoke",
        status=200,
        body={"reason": "mở lại để sửa"},
    )
    ctx.pair(
        "E15 revoke signoff again",
        "POST",
        lambda d: f"/survey-signoffs/{d['sig']['id']}/revoke",
        body={"reason": "lần hai"},
    )


def seed_evidence(ctx: Ctx, d: dict, building, origin: str, raw, route=None) -> str:
    evidence_id = uuid4()
    with psycopg.connect(ctx.owner_url) as conn:
        conn.execute(
            """
            INSERT INTO house_numbering.evidence (
                id, organization_id, project_id, building_id, route_id, origin, raw_number,
                source_reference, evidence_date, confidence, private_ref,
                created_by_actor_id, created_at
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,'PARITY-REF',NULL,'observed',NULL,%s,now())
            """,
            (
                evidence_id,
                ctx.fx["org_a_id"],
                ctx.fx["project_a_id"],
                d[building]["id"] if building else None,
                d[route]["id"] if route else None,
                origin,
                raw,
                ctx.fx["user_a_id"],
            ),
        )
    return str(evidence_id)


def seed_artifact(ctx: Ctx, namespace: str, filename: str, content_type: str, data: bytes):
    import hashlib

    artifact_id = str(uuid4())
    root = Path(os.environ["OPENVIETGIS_ARTIFACT_STORE_DIR"])
    folder = (
        root / "extensions" / ctx.fx["org_a_id"] / ctx.fx["project_a_id"] / namespace / artifact_id
    )
    folder.mkdir(parents=True)
    (folder / filename).write_bytes(data)
    (folder / f"{filename}.json").write_text(
        json.dumps(
            {
                "id": artifact_id,
                "filename": filename,
                "content_type": content_type,
                "byte_size": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
                "storage_key": f"extensions/{namespace}/{artifact_id}/{filename}",
            }
        ),
        encoding="utf-8",
    )
    return artifact_id


@case
def compare_download(label: str, e, a) -> None:
    req(e.status_code == a.status_code, f"{label}: status {e.status_code} != {a.status_code}")
    for name in ("content-type", "content-disposition"):
        req(
            e.headers.get(name) == a.headers.get(name),
            f"{label}: {name} {e.headers.get(name)!r} != {a.headers.get(name)!r}",
        )
    req(e.content == a.content, f"{label}: body differs")
    PASSED.append(label)
    print(f"PASS {label}", flush=True)


def batch_records(ctx: Ctx) -> None:
    editor = make_member(ctx.owner_url, ctx.fx, "editor", "HN Records Editor")
    for side in SIDES:
        d = ctx.data[side]
        seed_evidence(ctx, d, "b2", "certificate", "4")
        seed_evidence(ctx, d, "b2", "cad", "5")
        seed_evidence(ctx, d, "b4", "survey", "7")
        seed_evidence(ctx, d, "b5", "certificate", None)
        d["ev_unmatched"] = seed_evidence(ctx, d, None, "certificate", " 4 ", route="street")
        d["ev_noroute"] = seed_evidence(ctx, d, None, "certificate", "9")

    ctx.pair(
        "E15 reconciliation rebuild",
        "POST",
        "/reconciliation/rebuild",
        status=200,
        body=lambda d: {"estimated_building_ids": [d["b4"]["id"]]},
    )
    ctx.pair("E15 reconciliation rebuild idempotent", "POST", "/reconciliation/rebuild", body={})
    ctx.pair(
        "E15 reconciliation rebuild unknown field",
        "POST",
        "/reconciliation/rebuild",
        body={"bogus": []},
    )
    ctx.pair("E15 list reconcile cases", "GET", "/reconciliation/cases", headers=ctx.h)
    ctx.pair(
        "E15 list reconcile cases filtered",
        "GET",
        "/reconciliation/cases",
        headers=ctx.h,
        params={"status": "open", "case_class": "cad_certificate_mismatch"},
    )
    ctx.pair(
        "E15 suggest certificate match",
        "GET",
        lambda d: f"/reconciliation/evidence/{d['ev_unmatched']}/suggest",
        headers=ctx.h,
        extra={},
        view=lambda b: b,
    )
    ctx.pair(
        "E15 suggest certificate no route",
        "GET",
        lambda d: f"/reconciliation/evidence/{d['ev_noroute']}/suggest",
        headers=ctx.h,
    )
    ctx.pair(
        "E15 suggest certificate missing",
        "GET",
        f"/reconciliation/evidence/{uuid4()}/suggest",
        headers=ctx.h,
    )
    for side in SIDES:
        d = ctx.data[side]
        with psycopg.connect(ctx.owner_url) as conn:
            d["case"] = str(
                conn.execute(
                    "SELECT id FROM house_numbering.reconcile_cases WHERE building_id=%s",
                    (d["b2"]["id"],),
                ).fetchone()[0]
            )
    ctx.pair(
        "E15 resolve case stale",
        "POST",
        lambda d: f"/reconciliation/cases/{d['case']}/resolve",
        body={"expected_revision": 2, "status": "resolved", "resolution": "ok"},
    )
    ctx.pair(
        "E15 resolve case",
        "POST",
        lambda d: f"/reconciliation/cases/{d['case']}/resolve",
        status=200,
        body=lambda d: {
            "expected_revision": 1,
            "status": "resolved",
            "resolution": "  đã kiểm tra  ",
            "building_id": d["b2"]["id"],
        },
    )
    ctx.pair(
        "E15 resolve case closed",
        "POST",
        lambda d: f"/reconciliation/cases/{d['case']}/resolve",
        body={"expected_revision": 2, "status": "ignored", "resolution": "x"},
    )
    ctx.pair(
        "E15 resolve case bad status",
        "POST",
        lambda d: f"/reconciliation/cases/{d['case']}/resolve",
        body={"expected_revision": 2, "status": "open", "resolution": "x"},
    )

    def household(d, **over):
        body = {
            "building_id": d["b2"]["id"],
            "purpose": " cấp số nhà ",
            "consent": True,
            "lawful_basis_ref": "ND-01",
            "protected_data": {"chu_ho": "Nguyễn Văn A", "so_nguoi": 4, "ghi_chu": None},
            "expires_at": "2099-01-01T00:00:00Z",
        }
        body.update(over)
        return body

    o, r = ctx.pair(
        "E15 private household create", "POST", "/private-households", status=201, body=household
    )
    ctx.keep("hh", o, r)
    ctx.pair(
        "E15 private household update",
        "POST",
        "/private-households",
        status=201,
        body=lambda d: household(
            d, record_id=d["hh"]["id"], expected_revision=1, protected_data={"chu_ho": "B"}
        ),
    )
    ctx.pair(
        "E15 private household stale",
        "POST",
        "/private-households",
        body=lambda d: household(d, record_id=d["hh"]["id"], expected_revision=1),
    )
    ctx.pair(
        "E15 private household update needs revision",
        "POST",
        "/private-households",
        body=lambda d: household(d, record_id=d["hh"]["id"]),
    )
    ctx.pair(
        "E15 private household blank purpose",
        "POST",
        "/private-households",
        body=lambda d: household(d, purpose="   "),
    )
    ctx.pair(
        "E15 private household missing building",
        "POST",
        "/private-households",
        body=lambda d: household(d, building_id=str(uuid4())),
    )
    ctx.pair(
        "E15 private household editor denied",
        "POST",
        "/private-households",
        headers=editor["csrf"],
        body=household,
    )
    ctx.pair(
        "E15 private household bad payload",
        "POST",
        "/private-households",
        body=lambda d: household(d, protected_data=["not", "object"]),
    )
    ctx.pair(
        "E15 private household read",
        "GET",
        lambda d: f"/buildings/{d['b2']['id']}/private-households",
        headers=ctx.h,
        params={"purpose": "tra cứu"},
    )
    ctx.pair(
        "E15 private household read blank purpose",
        "GET",
        lambda d: f"/buildings/{d['b2']['id']}/private-households",
        headers=ctx.h,
        params={"purpose": "  "},
    )
    ctx.pair(
        "E15 private household read editor denied",
        "GET",
        lambda d: f"/buildings/{d['b2']['id']}/private-households",
        headers=editor["h"],
        params={"purpose": "tra cứu"},
    )

    for label, params in (
        ("E15 search exact parcel", {"query": "P-1A"}),
        ("E15 search like route", {"query": "Parity", "limit": 3}),
        ("E15 search no hit", {"query": "zzzz"}),
        ("E15 search blank", {"query": "   "}),
        ("E15 search coordinates", {"x": 40, "y": 10, "crs_code": 3857, "max_distance": 5}),
        ("E15 search partial coordinates", {"x": 40}),
        ("E15 search nothing", {}),
        ("E15 search bad limit", {"query": "P", "limit": 0}),
        ("E15 search bad distance", {"x": 1, "y": 1, "crs_code": 3857, "max_distance": 0}),
    ):
        ctx.pair(label, "GET", "/search", headers=ctx.h, params=params)

    ctx.pair("E15 list imports", "GET", "/imports", headers=ctx.h)
    ctx.pair(
        "E15 document history",
        "GET",
        lambda d: f"/buildings/{d['b1']['id']}/documents",
        headers=ctx.h,
    )
    ctx.pair(
        "E15 document download missing", "GET", f"/documents/{uuid4()}/download", headers=ctx.h
    )
    ctx.pair(
        "E15 export download private denied",
        "GET",
        f"/exports/{uuid4()}/download",
        headers=editor["h"],
    )

    data = b"building_id,house_number\nb1,2\n"
    resp = {}
    for side in SIDES:
        artifact = seed_artifact(ctx, "house-numbering-exports", "export.csv", "text/csv", data)
        resp[side] = ctx.call(side, "GET", f"/exports/{artifact}/download", headers=ctx.h)
    compare_download("E15 export download", resp["o"], resp["r"])
    pdf = b"%PDF-1.4 parity document"
    for side in SIDES:
        d = ctx.data[side]
        artifact = seed_artifact(
            ctx, "house-numbering-documents", "inspection.pdf", "application/pdf", pdf
        )
        d["doc"] = str(uuid4())
        with psycopg.connect(ctx.owner_url) as conn:
            conn.execute(
                """
                INSERT INTO house_numbering.documents (
                    id, organization_id, project_id, building_id, number_id, doc_type,
                    template_revision, status, artifact_id, issued_at, replaces_document_id,
                    revision, created_by_actor_id, updated_by_actor_id, created_at, updated_at
                ) VALUES (%s,%s,%s,%s,NULL,'inspection','tpl-1','draft',%s,NULL,NULL,1,%s,%s,
                          now(),now())
                """,
                (
                    d["doc"],
                    ctx.fx["org_a_id"],
                    ctx.fx["project_a_id"],
                    d["b1"]["id"],
                    artifact,
                    ctx.fx["user_a_id"],
                    ctx.fx["user_a_id"],
                ),
            )
        resp[side] = ctx.call(side, "GET", f"/documents/{d['doc']}/download", headers=ctx.h)
    compare_download("E15 document download", resp["o"], resp["r"])
    ctx.pair(
        "E15 document history with row",
        "GET",
        lambda d: f"/buildings/{d['b1']['id']}/documents",
        headers=ctx.h,
    )


def batch_numbering(ctx: Ctx) -> None:
    for side in SIDES:
        d = ctx.data[side]
        d["nlines"] = seed_source(
            ctx.owner_url, ctx.fx, "LineString", {"line-n": "LINESTRING(0 200,200 200)"}
        )
        d["npoints"] = seed_source(
            ctx.owner_url,
            ctx.fx,
            "Point",
            {
                "n-1": "POINT(20 190)",
                "n-2": "POINT(40 210)",
                "n-3": "POINT(60 190)",
                "n-4": "POINT(80 210)",
                "n-5": "POINT(100 400)",
            },
        )
    o, r = ctx.pair(
        "E15 numbering rule",
        "POST",
        "/rules",
        status=201,
        body=lambda d: {
            "version": f"num@{d['s']}",
            "status": "authoritative",
            "authority_ref": "QD-9",
            "max_distance_m": 50,
        },
    )
    ctx.keep("nrule", o, r)
    o, r = ctx.pair(
        "E15 numbering route",
        "POST",
        "/routes",
        status=201,
        body=lambda d: {"name": "Đường Số", "source_id": d["nlines"], "feature_id": "line-n"},
    )
    ctx.keep("nroute", o, r)
    for i in range(1, 6):
        o, r = ctx.pair(
            f"E15 numbering building {i}",
            "POST",
            "/buildings",
            status=201,
            body=lambda d, i=i: {
                "route_id": d["nroute"]["id"],
                "source_id": d["npoints"],
                "feature_id": f"n-{i}",
                "eligibility": "eligible",
            },
        )
        ctx.keep(f"n{i}", o, r)

    def inspect(d, **over):
        body = {
            "x": 20,
            "y": 190,
            "crs_code": 3857,
            "route_id": d["nroute"]["id"],
            "rule_id": d["nrule"]["id"],
            "existing_building_id": d["n1"]["id"],
        }
        body.update(over)
        return body

    ctx.pair("E15 inspect empty route side", "POST", "/inspect", headers=ctx.h, body=inspect)
    ctx.pair(
        "E15 inspect transformed point",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, x=0.0005, y=0.0016, crs_code=4326, existing_building_id=None),
    )
    ctx.pair(
        "E15 inspect ambiguous side",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, x=50, y=200, existing_building_id=None),
    )
    ctx.pair(
        "E15 inspect missing route",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, route_id=str(uuid4())),
    )
    ctx.pair(
        "E15 inspect missing rule",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, rule_id=str(uuid4())),
    )
    ctx.pair(
        "E15 inspect building off route",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, existing_building_id=d["b2"]["id"]),
    )
    ctx.pair(
        "E15 inspect bad crs",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, crs_code=0),
    )

    def preview(d, **over):
        body = {
            "building_ids": [d[f"n{i}"]["id"] for i in range(1, 6)],
            "rule_id": d["nrule"]["id"],
            "operation_id": d.setdefault("op_preview", str(uuid4())),
        }
        body.update(over)
        return body

    ctx.pair(
        "E15 preview duplicate buildings",
        "POST",
        "/previews",
        body=lambda d: preview(d, building_ids=[d["n1"]["id"], d["n1"]["id"]]),
    )
    ctx.pair(
        "E15 preview missing building",
        "POST",
        "/previews",
        body=lambda d: preview(d, building_ids=[str(uuid4())]),
    )
    ctx.pair(
        "E15 preview missing rule",
        "POST",
        "/previews",
        body=lambda d: preview(d, rule_id=str(uuid4())),
    )
    o, r = ctx.pair("E15 preview numbering", "POST", "/previews", status=201, body=preview)
    ctx.keep("preview", o, r)
    ctx.pair("E15 preview same operation", "POST", "/previews", body=preview)
    ctx.pair(
        "E15 preview protected building",
        "POST",
        "/previews",
        status=201,
        body=lambda d: preview(d, building_ids=[d["b5"]["id"]], operation_id=str(uuid4())),
    )
    ctx.pair(
        "E15 apply preview",
        "POST",
        lambda d: f"/previews/{d['preview']['id']}/apply",
        status=200,
    )
    ctx.pair("E15 apply preview replay", "POST", lambda d: f"/previews/{d['preview']['id']}/apply")
    ctx.pair("E15 apply preview missing", "POST", f"/previews/{uuid4()}/apply")
    ctx.pair("E15 inspect existing number", "POST", "/inspect", headers=ctx.h, body=inspect)
    ctx.pair(
        "E15 inspect between anchors",
        "POST",
        "/inspect",
        headers=ctx.h,
        body=lambda d: inspect(d, x=30, y=190, existing_building_id=None),
    )
    ctx.pair(
        "E15 preview already numbered",
        "POST",
        "/previews",
        status=201,
        body=lambda d: preview(d, operation_id=str(uuid4())),
        # Warning order follows each side's random building UUIDs (primary-key scan).
        view=lambda b: {**b, "warnings": sorted(b["warnings"])},
    )
    ctx.pair(
        "E15 numbering targets after apply",
        "GET",
        "/numbering-targets",
        headers=ctx.h,
        params=lambda d: {"route_id": d["nroute"]["id"]},
    )


def batch_geometry(ctx: Ctx) -> None:
    with psycopg.connect(ctx.owner_url) as conn:
        for code, reason in (("GR-1", "lưu hình học tuyến 1"), ("GR-2", "lưu hình học tuyến 2")):
            conn.execute(
                """
                INSERT INTO house_numbering.geometry_reviews (
                    id, organization_id, project_id, route_code, snapshot, snapshot_token,
                    reason, created_by_actor_id, updated_by_actor_id
                ) VALUES (%s,%s,%s,%s,%s::jsonb,%s,%s,%s,%s)
                """,
                (
                    uuid4(),
                    ctx.fx["org_a_id"],
                    ctx.fx["project_a_id"],
                    code,
                    json.dumps({"name": f"Hẻm {code}", "route_type": "alley"}),
                    "d" * 64,
                    reason,
                    ctx.fx["user_a_id"],
                    ctx.fx["user_a_id"],
                ),
            )
    for label, params in (
        ("E15 list geometry reviews", {}),
        ("E15 list geometry reviews page", {"limit": 1, "offset": 1}),
        ("E15 list geometry reviews bad limit", {"limit": 0}),
        ("E15 list geometry reviews bad offset", {"offset": -1}),
        ("E15 list geometry reviews non-integer", {"limit": "x"}),
    ):
        ctx.pair(label, "GET", "/geometry-reviews", headers=ctx.h, params=params)


def batch_photos(ctx: Ctx) -> None:
    jpeg = b"\xff\xd8\xff\xe0" + b"parity-photo" * 8

    def upload(label, files, data=None):
        resp = {
            side: ctx.call(side, "POST", "/survey/photos", files=files, data=data) for side in SIDES
        }
        compare(label, resp["o"], resp["r"])
        return resp

    resp = upload("E15 upload survey photo", {"file": ("p.jpg", jpeg, "image/jpeg")})
    photos = {side: resp[side].json()["id"] for side in SIDES}
    upload("E15 upload survey photo wrong type", {"file": ("p.txt", b"hello", "text/plain")})
    upload("E15 upload survey photo empty", {"file": ("p.png", b"", "image/png")})
    upload("E15 upload survey photo missing file", {"other": ("p.jpg", jpeg, "image/jpeg")})
    # Each runtime must accept a photo stored by the other one.
    ctx.pair(
        "E15 survey sync with cross-runtime photo",
        "POST",
        "/survey/sync",
        status=200,
        body=lambda d: {
            "operations": [
                {
                    "client_uuid": d.setdefault("c_photo", str(uuid4())),
                    "condition": "house",
                    "photo_artifact_ids": [photos["r" if d["s"] == "o" else "o"]],
                }
            ]
        },
    )


def ordered(body):
    """Sort lists whose order breaks ties on generated ids (after normalization)."""
    if isinstance(body, list):
        return [ordered(v) for v in body]
    if not isinstance(body, dict):
        return body
    out = {k: ordered(v) for k, v in body.items()}
    for key in ("items", "existing", "clusters"):
        if isinstance(out.get(key), list):
            out[key] = sorted(out[key], key=lambda x: json.dumps(x, sort_keys=True))
    return out


def batch_building_assignment(ctx: Ctx) -> None:
    """Building-assignment family: each side owns its geometry (offset x) and road names."""
    from modules.house_numbering.catalog_recovery import digest
    from modules.house_numbering.cluster_correction import cluster_key

    viewer = make_member(ctx.owner_url, ctx.fx, "viewer", "HN Assignment Viewer")
    keys = {"Qxo": "Qx", "Qxr": "Qx", "qxo": "qx", "qxr": "qx"}

    def made(side: str, path: str, body: dict) -> dict:
        resp = ctx.call(side, "POST", path, json=body)
        req(resp.status_code == 201, f"{path}: {resp.text}")
        return resp.json()

    for side in SIDES:
        d = ctx.data[side]
        x, t = (100000 if side == "o" else 200000), f"Qx{side}"

        def pt(dx, dy, x=x):
            return f"POINT({x + dx} {dy})"

        def ln(*points, x=x):
            return "LINESTRING(" + ",".join(f"{x + a} {b}" for a, b in points) + ")"

        def source(family, features, attributes=None):
            return seed_source(ctx.owner_url, ctx.fx, family, features, attributes)

        def buildings(section, src, attrs, extra=None, side=side, d=d):
            for fid, a in attrs.items():
                body = {"source_id": src, "feature_id": fid, **(extra or {}).get(fid, {})}
                d[f"{section}_{fid}"] = made(side, "/buildings", body)["id"]
                key = cluster_key(a.get("ten_duong"), a.get("hem"), a.get("tuyen_id"))
                keys[key] = f"<cluster-{section}-{fid}>"

        # A: preview/apply and audit (mirrors test_building_assignment_api).
        a_lines = source(
            "LineString",
            {
                "main": ln((0, 0), (100, 0)),
                "branch": ln((50, 0), (50, 80)),
                "pending": ln((0, 50), (100, 50)),
            },
            {
                "pending": {
                    "route_code": f"legacy-{t}",
                    "review_reason": "parent_required;invalid_prefix",
                    "parent_code": "missing",
                    "ten": "Hẻm 77",
                }
            },
        )
        main = made(
            side,
            "/routes",
            {
                "name": f"Đường Nguyễn Văn Linh {t}",
                "source_id": a_lines,
                "feature_id": "main",
                "status": "confirmed",
            },
        )
        alley = made(
            side,
            "/routes",
            {
                "name": "Hẻm 155",
                "route_type": "alley",
                "parent_route_id": main["id"],
                "prefix": "155",
                "source_id": a_lines,
                "feature_id": "branch",
            },
        )
        d["a_main"], d["a_alley"] = main["id"], alley["id"]
        nvl = f"Nguyễn Văn Linh {t}"
        a_attrs = {
            "b1": {"ten_duong": f"Nguyen Van Linh {t}", "hem": "155"},
            "b2": {"ten_duong": nvl, "hem": ""},
            "b3": {},
            "b4": {"ten_duong": nvl, "hem": "155"},
            "b5": {"ten_duong": nvl, "hem": "77", "tuyen_id": f"legacy-{t}"},
            "b6": {"ten_duong": nvl, "hem": ""},
        }
        d["a_pts"] = source(
            "Point",
            {
                "b1": pt(55, 2),
                "b2": pt(20, 3),
                "b3": pt(54, 5),
                "b4": pt(22, 4),
                "b5": pt(50, 52),
                "b6": pt(30, 5),
            },
            a_attrs,
        )
        buildings("a", d["a_pts"], a_attrs, {"b4": {"route_id": main["id"]}})

        # C: survey-code conflict correction (mirrors test_cluster_correction).
        c_lines = source(
            "LineString", {"old": ln((0, 600), (100, 600)), "new": ln((0, 500), (100, 500))}
        )
        old = made(
            side, "/routes", {"name": f"Đường Sai {t}", "source_id": c_lines, "feature_id": "old"}
        )
        new = made(
            side, "/routes", {"name": f"Đường Đúng {t}", "source_id": c_lines, "feature_id": "new"}
        )
        d["c_old"], d["c_new"] = old["id"], new["id"]
        c_attr = {"ten_duong": f"Đường Đúng {t}", "hem": "", "tuyen_id": old["id"]}
        c_attrs = {fid: c_attr for fid in ("near1", "near2", "far", "ineligible")}
        d["c_pts"] = source(
            "Point",
            {
                "near1": pt(20, 503),
                "near2": pt(20, 508),
                "far": pt(20, 600),
                "ineligible": pt(20, 504),
            },
            c_attrs,
        )
        buildings(
            "c",
            d["c_pts"],
            c_attrs,
            {"ineligible": {"eligibility": "ineligible", "ineligibility_reason": "fixture"}},
        )
        d["c_ids"] = [d[f"c_{fid}"] for fid in c_attrs]
        d["cluster"] = cluster_key(c_attr["ten_duong"], "", old["id"])

        # D: reviewed alias catalog (mirrors test_catalog_recovery).
        d_lines = source(
            "LineString",
            {"old9": ln((0, 1000), (100, 1000)), "pending77": ln((20, 1000), (20, 1050))},
            {
                "old9": {"ten": f"Đường số 9 {t}"},
                "pending77": {
                    "ten": f"Đường số 9 Ấp 21 {t} hẻm 77",
                    "prefix": "77",
                    "route_type": "alley",
                    "route_code": f"legacy_77_{t}",
                    "review_reason": "parent_required",
                },
            },
        )
        old9 = made(
            side,
            "/routes",
            {"name": f"Đường số 9 {t}", "source_id": d_lines, "feature_id": "old9"},
        )["id"]
        d_attrs = {
            fid: {"ten_duong": f"Đường số 9 Ấp 21 {t}", "hem": hem, "tuyen_id": code}
            for fid, hem, code in (
                ("a", "", old9),
                ("b", "", old9),
                ("far", "", old9),
                ("wrong_prefix", "77", old9),
                ("nocode", "", ""),
            )
        }
        d["d_pts"] = source(
            "Point",
            {
                "a": pt(20, 1003),
                "b": pt(20, 1008),
                "far": pt(20, 1100),
                "wrong_prefix": pt(20, 1004),
                "nocode": pt(20, 1005),
            },
            d_attrs,
        )
        buildings("d", d["d_pts"], d_attrs, {fid: {"eligibility": "eligible"} for fid in d_attrs})
        d["d_ids"] = [d[f"d_{fid}"] for fid in d_attrs]
        keys[digest([old9, f"so 9 ap 21 qx{side}", "", old9])[:32]] = "<catalog-group>"

        # E: prefix tree recovery (mirrors test_prefix_recovery).
        e_lines = source(
            "LineString",
            {
                "root": ln((0, 2000), (100, 2000)),
                "other": ln((0, 2200), (100, 2200)),
                "parent": ln((20, 2000), (20, 2050)),
            },
            {
                "root": {"ten": f"Đường A {t}"},
                "other": {"ten": f"Đường A {t}"},
                "parent": {"ten": f"Đường A {t} hẻm 12"},
            },
        )
        root = made(
            side, "/routes", {"name": f"Đường A {t}", "source_id": e_lines, "feature_id": "root"}
        )["id"]
        made(side, "/routes", {"name": f"Đường A {t}", "source_id": e_lines, "feature_id": "other"})
        made(
            side,
            "/routes",
            {
                "name": f"Đường A {t} hẻm 12",
                "route_type": "alley",
                "parent_route_id": root,
                "prefix": "12",
                "source_id": e_lines,
                "feature_id": "parent",
            },
        )
        branches = {"a": "12/3", "b": "12/3", "c": "12/8/9", "d": "???", "e": "12/4", "f": "12/99"}
        e_attrs = {
            fid: {"ten_duong": f"Đường A {t}", "hem": b, "tuyen_id": root}
            for fid, b in branches.items()
        }
        d["e_pts"] = source(
            "Point",
            {
                "a": pt(22, 2035),
                "b": pt(95, 2150),
                "c": pt(50, 2050),
                "d": pt(20, 2002),
                "e": pt(35, 2038),
                "f": pt(20, 2003),
            },
            e_attrs,
        )
        buildings("e", d["e_pts"], e_attrs, {fid: {"eligibility": "eligible"} for fid in e_attrs})
        segments = {
            "seg1": (((20, 2035), (30, 2035)), "12/3"),
            "seg2": (((30, 2035), (40, 2035)), "12/3"),
            "gap": (((50, 2050), (70, 2050)), "12/8/9"),
            "branch_a": (((20, 2040), (30, 2040)), "12/4"),
            "branch_b": (((50, 2045), (60, 2045)), "12/4"),
        }
        source(
            "LineString",
            {fid: ln(*points) for fid, (points, _) in segments.items()},
            {
                fid: {
                    "ten": f"Đường A {t} hẻm {b}",
                    "prefix": b,
                    "route_type": "alley",
                    "route_code": f"{fid}-{t}",
                    "review_reason": "parent_required",
                }
                for fid, (_, b) in segments.items()
            },
        )
        for b in ("12/3", "12/8/9", "12/4", "12/99"):
            keys[digest([f"a qx{side}", b])[:32]] = f"<prefix-{b}>"

    def case(label, path, body, *, headers=None, keep=None, status=None, view=ordered):
        o, r = ctx.pair(
            label,
            "POST",
            "/building-assignment" + path,
            body=body,
            headers=headers,
            extra=keys,
            view=view,
            status=status,
        )
        if keep:
            ctx.keep(keep, o, r)

    # A: reviewable preview → apply, then the audit queue.
    plan = {"limit": 100, "max_distance_m": 80}

    def apply(d, **kw):
        return {
            **plan,
            "source_id": d["a_pts"],
            "preview_token": d["a_plan"]["preview_token"],
            "assignments": {d["a_b1"]: d["a_alley"], d["a_b2"]: d["a_main"]},
            **kw,
        }

    def audit(d, **kw):
        return {"source_id": d["a_pts"], "limit": 100, **kw}

    def strict(d):
        return {"building_ids": [d["a_b6"]], "strict": True, "limit": 100}

    def strict_apply(d, rid):
        token = d["a_strict"]["preview_token"]
        return {**strict(d), "preview_token": token, "assignments": {d["a_b6"]: d[rid]}}

    case(
        "E15 assignment preview",
        "/preview",
        lambda d: {**plan, "source_id": d["a_pts"]},
        keep="a_plan",
        status=200,
    )
    case(
        "E15 assignment preview page",
        "/preview",
        lambda d: {"source_id": d["a_pts"], "limit": 2},
        view=lambda b: {"n": len(b["items"]), "next": b["next_after_id"]},
    )
    case("E15 assignment preview viewer", "/preview", plan, headers=viewer["csrf"])
    case("E15 assignment preview without CSRF", "/preview", plan, headers=ctx.h)
    case("E15 assignment preview invalid limit", "/preview", {"limit": 0})
    case("E15 assignment preview unknown mapping key", "/preview", {"mapping": {"street": "x"}})
    case("E15 assignment preview extra field", "/preview", {"foo": 1})
    case(
        "E15 assignment preview duplicate batch",
        "/preview",
        lambda d: {"building_ids": [d["a_b5"], d["a_b5"]], "limit": 100},
    )
    case(
        "E15 assignment preview batch after_id",
        "/preview",
        lambda d: {"building_ids": [d["a_b5"]], "after_id": d["a_b5"]},
    )
    case("E15 assignment apply stale token", "/apply", lambda d: apply(d, preview_token="0" * 64))
    case("E15 assignment apply bad token pattern", "/apply", lambda d: apply(d, preview_token="x"))
    case(
        "E15 assignment apply unproposed route",
        "/apply",
        lambda d: apply(d, assignments={d["a_b1"]: str(uuid4())}),
    )
    case("E15 assignment apply viewer", "/apply", apply, headers=viewer["csrf"])
    case("E15 assignment apply", "/apply", apply, status=200)
    case(
        "E15 assignment preview linked batch",
        "/preview",
        lambda d: {"building_ids": [d["a_b1"]], "limit": 100},
    )
    case(
        "E15 assignment audit parent missing",
        "/audit",
        lambda d: audit(d, group="parent_missing"),
        keep="a_audit",
        status=200,
    )
    case(
        "E15 assignment audit cluster",
        "/audit",
        lambda d: audit(d, group="parent_missing", cluster=d["a_audit"]["clusters"][0]["key"]),
    )
    case("E15 assignment audit ready", "/audit", lambda d: audit(d, group="ready"))
    case(
        "E15 assignment audit missing page",
        "/audit",
        lambda d: audit(d, group="address_missing", limit=1),
    )
    case("E15 assignment audit project", "/audit", {})
    case("E15 assignment audit viewer", "/audit", audit, headers=viewer["csrf"], status=200)
    case("E15 assignment audit invalid group", "/audit", {"group": "bogus"})
    case("E15 assignment audit invalid cluster", "/audit", {"cluster": "XYZ"})
    case("E15 assignment strict preview", "/preview", strict, keep="a_strict", status=200)
    case(
        "E15 assignment strict apply other route",
        "/apply",
        lambda d: strict_apply(d, "a_alley"),
    )
    case(
        "E15 assignment strict apply",
        "/apply",
        lambda d: strict_apply(d, "a_main"),
        status=200,
    )
    case("E15 assignment audit after apply", "/audit", audit)

    # C: explicit correction of a survey-code conflict cluster.
    def fix(d, **kw):
        return {"cluster": d["cluster"], "building_ids": d["c_ids"], "max_distance_m": 35, **kw}

    def confirm(d, **kw):
        return {
            **fix(d),
            "route_id": d["c_new"],
            "selected_ids": [d["c_near1"], d["c_near2"]],
            "reason": "Đã kiểm tra đúng đường thực tế",
            "preview_token": d["c_plan"]["preview_token"],
            **kw,
        }

    def conflicts(d, **kw):
        return {"source_id": d["c_pts"], "group": "address_conflict", "cluster": d["cluster"], **kw}

    case("E15 correction audit", "/audit", lambda d: conflicts(d, limit=100), status=200)
    case("E15 correction preview", "/correction/preview", fix, keep="c_plan", status=200)
    case(
        "E15 correction preview stale cluster",
        "/correction/preview",
        lambda d: fix(d, cluster="0" * 24),
    )
    case(
        "E15 correction preview over limit",
        "/correction/preview",
        lambda d: fix(d, max_distance_m=80),
    )
    case(
        "E15 correction preview duplicate ids",
        "/correction/preview",
        lambda d: fix(d, building_ids=[d["c_near1"], d["c_near1"]]),
    )
    case("E15 correction preview viewer", "/correction/preview", fix, headers=viewer["csrf"])
    case(
        "E15 correction apply original route",
        "/correction/apply",
        lambda d: confirm(d, route_id=d["c_old"]),
    )
    case(
        "E15 correction apply far house",
        "/correction/apply",
        lambda d: confirm(d, selected_ids=[d["c_far"]]),
    )
    case(
        "E15 correction apply ineligible house",
        "/correction/apply",
        lambda d: confirm(d, selected_ids=[d["c_ineligible"]]),
    )
    case(
        "E15 correction apply stale token",
        "/correction/apply",
        lambda d: confirm(d, preview_token="0" * 64),
    )
    case(
        "E15 correction apply blank reason",
        "/correction/apply",
        lambda d: confirm(d, reason="         x"),
    )
    case(
        "E15 correction apply missing route",
        "/correction/apply",
        lambda d: confirm(d, route_id=str(uuid4())),
    )
    case("E15 correction apply", "/correction/apply", confirm, status=200)
    case("E15 correction apply repeated", "/correction/apply", confirm)
    case("E15 correction audit after", "/audit", conflicts)

    # D: reviewed aliases are code-scoped and stale-safe.
    def pick(d, token=None, reason="Đã xác minh tên đường tại Ấp 21"):
        group = d["d_plan"]["items"][0]
        return {
            "source_id": d["d_pts"],
            "selections": {group["id"]: token or group["token"]},
            "reason": reason,
        }

    case(
        "E15 catalog preview",
        "/catalog-recovery/preview",
        lambda d: {"source_id": d["d_pts"]},
        keep="d_plan",
        status=200,
    )
    case(
        "E15 catalog preview search",
        "/catalog-recovery/preview",
        lambda d: {"source_id": d["d_pts"], "search": "Ấp 21", "limit": 1},
    )
    case("E15 catalog preview bad limit", "/catalog-recovery/preview", {"limit": 101})
    case(
        "E15 catalog apply stale token",
        "/catalog-recovery/apply",
        lambda d: pick(d, token="0" * 64),
    )
    case(
        "E15 catalog apply blank reason",
        "/catalog-recovery/apply",
        lambda d: pick(d, reason="         x"),
    )
    case(
        "E15 catalog apply short reason",
        "/catalog-recovery/apply",
        lambda d: pick(d, reason="short"),
    )
    case("E15 catalog apply viewer", "/catalog-recovery/apply", pick, headers=viewer["csrf"])
    case("E15 catalog apply", "/catalog-recovery/apply", pick, status=200)
    case("E15 catalog apply repeated", "/catalog-recovery/apply", pick)
    case(
        "E15 catalog strict preview after alias",
        "/preview",
        lambda d: {"building_ids": d["d_ids"], "strict": True, "limit": 100},
        status=200,
    )

    # E: prefix tree recovery queue (read-only).
    case(
        "E15 prefix recovery preview",
        "/prefix-recovery/preview",
        lambda d: {"source_id": d["e_pts"]},
        status=200,
    )
    case(
        "E15 prefix recovery search",
        "/prefix-recovery/preview",
        lambda d: {"source_id": d["e_pts"], "search": "12/3", "limit": 1},
    )
    case("E15 prefix recovery bad offset", "/prefix-recovery/preview", {"offset": -1})


def sorted_lists(*names):
    """View sorting lists ordered by generated building ids (after normalization)."""

    def view(body):
        if isinstance(body, list):
            return [view(v) for v in body]
        if not isinstance(body, dict):
            return body
        out = {k: view(v) for k, v in body.items()}
        for key in names:
            if isinstance(out.get(key), list):
                out[key] = sorted(out[key], key=lambda x: json.dumps(x, sort_keys=True))
        return out

    return view


def batch_route_import(ctx: Ctx) -> None:
    """Route-tree import, staged geometry, address verification and verified links.

    Each side owns its lines/points (offset x) and side-tagged road names and codes, so
    address paths, survey codes and staged reviews never collide in the shared project.
    """
    from modules.house_numbering.route_topology import stable_id

    viewer = make_member(ctx.owner_url, ctx.fx, "viewer", "HN Route Import Viewer")
    keys = {"Zqo": "Zq", "Zqr": "Zq", "zqo": "zq", "zqr": "zq"}
    mapping = {
        "code": "code",
        "name": "name",
        "parent_code": "parent",
        "prefix": "prefix",
        "route_type": "kind",
    }
    examples = sorted_lists("examples")
    listed = sorted_lists("items", "members", "examples")

    for side in SIDES:
        d = ctx.data[side]
        x, t = (300000 if side == "o" else 400000), f"Zq{side}"
        road = f"Đường {t}"
        d["t"], d["road"] = t, road

        def ln(*points, x=x):
            return "LINESTRING(" + ",".join(f"{x + a} {b}" for a, b in points) + ")"

        def attrs(code, name, kind, parent="", prefix="", review=None, t=t):
            a = {
                "code": f"{code}-{t}",
                "name": name,
                "kind": kind,
                "parent": f"{parent}-{t}" if parent else "",
                "prefix": prefix,
            }
            if review:
                a["review_reason"] = review
            return a

        def source(family, features, attributes=None):
            return seed_source(ctx.owner_url, ctx.fx, family, features, attributes)

        d["s_main"] = source(
            "LineString",
            {"A": ln((0, 0), (100, 0)), "B": ln((50, 0), (50, 80)), "C": ln((50, 40), (100, 40))},
            {
                "A": attrs("A", road, "street"),
                "B": attrs("B", f"{road} hẻm 155", "alley", "A", "155"),
                "C": attrs("C", f"{road} hẻm 155/3", "alley", "B", "3"),
            },
        )
        d["s_x"] = source(
            "LineString",
            {"X": ln((50, 60), (0, 60))},
            {"X": attrs("X", f"{road} hẻm 155/7", "alley", "B", "7", "legacy_review")},
        )
        d["s_rec"] = source(
            "LineString",
            {
                "M1": ln((50, 20), (20, 20)),
                "M2": ln((20, 20), (0, 20)),
                "P": ln((60, -50), (90, -50)),
            },
            {
                "M1": attrs("M1", f"{road} hẻm 155/9", "alley", "B", "155/9", "legacy_review"),
                "M2": attrs("M2", f"{road} hẻm 155/9", "alley", "B", "155/9", "legacy_review"),
                "P": attrs("P", f"{road} hẻm 160/4", "alley", "", "4", "legacy_review"),
            },
        )
        d["s_par"] = source(
            "LineString", {"p160": ln((60, 0), (60, -50)), "p160b": ln((70, 0), (70, -50))}
        )
        code = f"X-{t}"
        survey = {
            "b1": ({"ten_duong": road, "hem": "155/7", "tuyen_id": code}, (10, 62)),
            "b2": ({"ten_duong": f"Đường Khác {t}", "hem": "", "tuyen_id": code}, (20, 65)),
            "b3": ({"ten_duong": road, "hem": "", "tuyen_id": code}, (20, 63)),
            "b4": ({"ten_duong": road, "hem": "", "tuyen_id": code}, (30, 57)),
        }
        d["s_pts"] = source(
            "Point",
            {fid: f"POINT({x + a} {b})" for fid, (_, (a, b)) in survey.items()},
            {fid: a for fid, (a, _) in survey.items()},
        )
        for fid in survey:
            made = ctx.call(
                side, "POST", "/buildings", json={"source_id": d["s_pts"], "feature_id": fid}
            )
            req(made.status_code == 201, f"building {fid}: {made.text}")
            d[fid] = made.json()["id"]
        d["key"] = {
            fid: f"{d[src]}:{fid}"
            for src, fids in (
                ("s_main", "ABC"),
                ("s_x", ["X"]),
                ("s_rec", ["M1", "M2", "P"]),
                ("s_par", ["p160", "p160b"]),
            )
            for fid in fids
        }
        merge = stable_id(d["key"]["M1"], d["key"]["M2"])
        d["merge_id"], d["task_id"] = f"recovery-{merge}", stable_id(f"zq{side}", "160")
        keys[merge], keys[d["task_id"]] = "<merge-155-9>", "<task-160>"

    def layer(source):
        return {"source_id": source, "mapping": mapping}

    def tree(d, **kw):
        return {"layers": [layer(d["s_main"])], **kw}

    def route_case(label, path, body, *, headers=None, keep=None, status=None, view=None):
        o, r = ctx.pair(
            label,
            "POST",
            "/route-import" + path,
            body=body,
            headers=headers,
            extra=keys,
            view=view,
            status=status,
        )
        if keep:
            ctx.keep(keep, o, r)

    def token_parity(label, body):
        """Both runtimes plan one side's fixtures to the same bytes, token included."""
        for side in SIDES:
            b = body(ctx.data[side])
            o = ctx.call("o", "POST", "/route-import/preview", json=b)
            r = ctx.call("r", "POST", "/route-import/preview", json=b)
            req(
                o.status_code == r.status_code == 200,
                f"{label}: {o.status_code}/{r.status_code} {r.text[:800]}",
            )
            diff = parity.difference(o.json(), r.json())
            req(diff is None, f"{label} ({side} fixtures): {diff}")
        PASSED.append(label)
        print(f"PASS {label}", flush=True)

    def cross_apply(label, preview, apply):
        """Preview on one runtime, apply that token on the other."""
        resp = {}
        for side, other in (("o", "r"), ("r", "o")):
            d = ctx.data[side]
            planned = ctx.call(other, "POST", "/route-import/preview", json=preview(d))
            req(planned.status_code == 200, f"{label}: {planned.text[:800]}")
            body = {**apply(d), "preview_token": planned.json()["preview_token"]}
            resp[side] = ctx.call(side, "POST", "/route-import/apply", json=body)
        compare(label, resp["o"], resp["r"], extra=keys)
        req(resp["r"].status_code == 201, f"{label}: {resp['r'].text[:800]}")

    # Tree import (mirrors test_route_tree_import).
    def missing_parent(d):
        return tree(d, overrides={d["key"]["B"]: {"parent_code": "", "prefix": "155"}})

    def apply_tree(d, **kw):
        return tree(d, preview_token=d["t_plan"]["preview_token"], **kw)

    route_case("E15 route import preview missing parent", "/preview", missing_parent, status=200)
    route_case("E15 route import preview", "/preview", tree, keep="t_plan", status=200)
    token_parity("E15 route import preview token parity", tree)
    route_case(
        "E15 route import preview duplicate source",
        "/preview",
        lambda d: {"layers": [layer(d["s_main"]), layer(d["s_main"])]},
    )
    route_case(
        "E15 route import preview unknown source",
        "/preview",
        {"layers": [layer("00000000-0000-4000-8000-000000000000")]},
    )
    route_case("E15 route import preview extra field", "/preview", lambda d: tree(d, foo=1))
    route_case("E15 route import preview no layers", "/preview", {"layers": []})
    route_case(
        "E15 route import preview bad mapping",
        "/preview",
        lambda d: {"layers": [{"source_id": d["s_main"], "mapping": {"street": "x"}}]},
    )
    route_case(
        "E15 route import preview bad override",
        "/preview",
        lambda d: tree(
            d, overrides={"k": {"parent_code": "", "prefix": "1", "route_type": "road"}}
        ),
    )
    route_case("E15 route import preview viewer", "/preview", tree, headers=viewer["csrf"])
    route_case("E15 route import preview without CSRF", "/preview", tree, headers=ctx.h)
    route_case(
        "E15 route import apply stale token", "/apply", lambda d: tree(d, preview_token="0" * 64)
    )
    route_case("E15 route import apply bad token", "/apply", lambda d: tree(d, preview_token="x"))
    route_case("E15 route import apply missing token", "/apply", tree)
    route_case(
        "E15 route import apply parent not selected",
        "/apply",
        lambda d: apply_tree(d, selected_keys=[d["key"]["C"]]),
    )
    route_case(
        "E15 route import apply duplicate selection",
        "/apply",
        lambda d: apply_tree(d, selected_keys=[d["key"]["A"], d["key"]["A"]]),
    )
    route_case(
        "E15 route import apply empty selection",
        "/apply",
        lambda d: apply_tree(d, selected_keys=[]),
    )
    route_case("E15 route import apply viewer", "/apply", apply_tree, headers=viewer["csrf"])
    cross_apply("E15 route import apply (cross-runtime token)", tree, tree)
    route_case("E15 route import preview after apply", "/preview", tree, keep="t_again", status=200)
    route_case("E15 route import apply stale after apply", "/apply", apply_tree)
    route_case(
        "E15 route import apply idempotent",
        "/apply",
        lambda d: tree(d, preview_token=d["t_again"]["preview_token"]),
        status=201,
    )
    for side in SIDES:
        d = ctx.data[side]
        routes = ctx.call(side, "GET", "/routes", headers=ctx.h).json()["items"]
        d["A_id"] = next(r["id"] for r in routes if r["name"] == d["road"])

    # Legacy review row: survey conflict → staged geometry → address decisions.
    def review(d, **kw):
        overrides = {d["key"]["X"]: {"parent_code": f"B-{d['t']}", "prefix": "7", "reviewed": True}}
        return {"layers": [layer(d["s_main"]), layer(d["s_x"])], "overrides": overrides, **kw}

    def stage(d, **kw):
        body = {
            "preview_token": d["x_plan"]["preview_token"],
            "selected_keys": [d["key"]["X"]],
            "reason": "Hình học đã khớp hiện trạng ngoài thực địa",
            **kw,
        }
        return review(d, **body)

    route_case("E15 route import review preview", "/preview", review, keep="x_plan", view=examples)
    token_parity("E15 route import review token parity", review)
    route_case(
        "E15 route import apply with errors",
        "/apply",
        lambda d: review(d, preview_token=d["x_plan"]["preview_token"]),
    )
    route_case("E15 stage geometry short reason", "/stage-geometry", lambda d: stage(d, reason="x"))
    route_case(
        "E15 stage geometry blank reason",
        "/stage-geometry",
        lambda d: stage(d, reason="          x"),
    )
    route_case(
        "E15 stage geometry duplicate keys",
        "/stage-geometry",
        lambda d: stage(d, selected_keys=[d["key"]["X"], d["key"]["X"]]),
    )
    route_case(
        "E15 stage geometry stale token",
        "/stage-geometry",
        lambda d: stage(d, preview_token="0" * 64),
    )
    route_case(
        "E15 stage geometry ineligible key",
        "/stage-geometry",
        lambda d: stage(d, selected_keys=[d["key"]["A"]]),
    )
    route_case("E15 stage geometry viewer", "/stage-geometry", stage, headers=viewer["csrf"])
    route_case("E15 stage geometry", "/stage-geometry", stage, keep="x_stage", status=201)
    route_case("E15 stage geometry stale after stage", "/stage-geometry", stage)
    route_case(
        "E15 route import review preview staged",
        "/preview",
        review,
        keep="x_plan2",
        view=examples,
        status=200,
    )
    route_case(
        "E15 stage geometry unchanged",
        "/stage-geometry",
        lambda d: stage(d, preview_token=d["x_plan2"]["preview_token"]),
        status=201,
    )

    def reviews(d, tail=""):
        return f"/geometry-reviews/{d['x_stage']['items'][0]['id']}{tail}"

    def addresses(d, tail=""):
        return reviews(d, "/addresses" + tail)

    def links(d):
        return reviews(d, "/verified-links")

    def get(label, path, *, params=None, keep=None, view=listed, status=None):
        o, r = ctx.pair(
            label, "GET", path, params=params, headers=ctx.h, extra=keys, view=view, status=status
        )
        if keep:
            ctx.keep(keep, o, r)

    get("E15 address review list", addresses, keep="x_addr", status=200)
    get(
        "E15 address review page",
        addresses,
        params={"limit": 1},
        view=lambda b: {"n": len(b["items"]), "next": b["next_offset"], "counts": b["counts"]},
    )
    get("E15 address review bad offset", addresses, params={"offset": -1})
    get("E15 address review bad limit", addresses, params={"limit": 0})
    get("E15 address review non-integer limit", addresses, params={"limit": "x"})
    get(
        "E15 address review unknown",
        "/geometry-reviews/00000000-0000-4000-8000-000000000000/addresses",
    )
    get("E15 address review invalid id", "/geometry-reviews/not-a-uuid/addresses")
    get("E15 verified links not promoted", links, status=200)

    def item(d, building):
        return next(i for i in d["x_addr"]["items"] if i["building_id"] == d[building])

    def verify(d, **kw):
        it = item(d, "b2")
        return {
            "evidence_token": it["evidence_token"],
            "expected_revision": it["revision"],
            "decision": "other_address",
            "verified_road": f"Đường Khác {d['t']}",
            "evidence": "Biển số nhà ghi đường khác",
            **kw,
        }

    def put(label, body, *, headers=None, status=None):
        ctx.pair(
            label,
            "PUT",
            lambda d: addresses(d, "/" + d["b2"]),
            body=body,
            headers=headers,
            extra=keys,
            status=status,
        )

    put("E15 address verify stale token", lambda d: verify(d, evidence_token="0" * 64))
    put("E15 address verify wrong decision", lambda d: verify(d, decision="belongs"))
    put("E15 address verify blank evidence", lambda d: verify(d, evidence="          x"))
    put("E15 address verify short evidence", lambda d: verify(d, evidence="x"))
    put("E15 address verify bad prefix", lambda d: verify(d, verified_prefix="???"))
    put("E15 address verify unknown road", lambda d: verify(d, verified_road="Chưa rõ"))
    put("E15 address verify bad decision", lambda d: verify(d, decision="maybe"))
    put("E15 address verify viewer", verify, headers=viewer["csrf"])
    put("E15 address verify without CSRF", verify, headers=ctx.h)
    put("E15 address verify", verify, status=200)
    put("E15 address verify repeated", verify)

    def batch(d, **kw):
        group = d["x_addr"]["batch_groups"][0]
        members = [
            {
                "building_id": m["building_id"],
                "evidence_token": m["evidence_token"],
                "expected_revision": m["revision"],
            }
            for m in group["members"]
        ]
        return {
            "key": group["key"],
            "members": members,
            "evidence": "Cùng tổ khảo sát xác nhận hẻm 155/7",
            "confirmed": True,
            **kw,
        }

    def post_batch(label, body, *, headers=None, status=None):
        ctx.pair(
            label,
            "POST",
            lambda d: addresses(d, "/batch"),
            body=body,
            headers=headers,
            extra=keys,
            status=status,
        )

    post_batch(
        "E15 address batch single member", lambda d: batch(d, members=batch(d)["members"][:1])
    )
    post_batch(
        "E15 address batch duplicate member",
        lambda d: batch(d, members=[batch(d)["members"][0]] * 2),
    )
    post_batch("E15 address batch wrong key", lambda d: batch(d, key="0" * 64))
    post_batch(
        "E15 address batch stale member",
        lambda d: batch(
            d, members=[{**m, "evidence_token": "0" * 64} for m in batch(d)["members"]]
        ),
    )
    post_batch("E15 address batch unconfirmed", lambda d: batch(d, confirmed=False))
    post_batch("E15 address batch viewer", batch, headers=viewer["csrf"])
    post_batch("E15 address batch", batch, status=200)
    post_batch("E15 address batch repeated", batch)
    get("E15 address review list after decisions", addresses, status=200)

    # Every address decided: the reviewed row is ready and can be promoted.
    route_case("E15 route import review preview verified", "/preview", review, keep="x_plan3")
    route_case(
        "E15 route import apply reviewed",
        "/apply",
        lambda d: review(
            d, preview_token=d["x_plan3"]["preview_token"], selected_keys=[d["key"]["X"]]
        ),
        status=201,
    )
    put("E15 address verify promoted", verify)

    # Verified links: only verified buildings of the exact promoted route.
    get("E15 verified links", links, keep="x_links", status=200)
    get("E15 verified links bad offset", links, params={"offset": -1})

    def link(d, **kw):
        return {
            "preview_token": d["x_links"]["preview_token"],
            "building_ids": [d["b3"], d["b4"]],
            "confirmed": True,
            **kw,
        }

    def post_link(label, body, *, headers=None, status=None):
        ctx.pair(label, "POST", links, body=body, headers=headers, extra=keys, status=status)

    post_link("E15 verified links stale token", lambda d: link(d, preview_token="0" * 64))
    post_link("E15 verified links other address", lambda d: link(d, building_ids=[d["b2"]]))
    post_link("E15 verified links duplicate", lambda d: link(d, building_ids=[d["b3"], d["b3"]]))
    post_link("E15 verified links unconfirmed", lambda d: link(d, confirmed=False))
    post_link("E15 verified links viewer", link, headers=viewer["csrf"])
    post_link("E15 verified links apply", link, status=200)
    post_link("E15 verified links repeated", link)
    get("E15 verified links after", links, status=200)

    # Recovery: merge two segments of one path and supply a missing parent feature.
    def recovery(d, **kw):
        k = d["key"]
        overrides = {
            k["M1"]: {"parent_code": f"B-{d['t']}", "prefix": "155/9", "reviewed": True},
            k["p160"]: {"parent_code": d["A_id"], "prefix": "160", "reviewed": True},
            k["P"]: {
                "parent_code": f"recovery-parent-{d['task_id']}",
                "prefix": "4",
                "reviewed": True,
            },
        }
        body = {
            "layers": [layer(d["s_main"]), layer(d["s_rec"])],
            "merge_groups": [d["merge_id"]],
            "parent_features": pick(d),
            "overrides": overrides,
        }
        return {**body, **kw}

    def plain(d):
        return {"layers": [layer(d["s_main"]), layer(d["s_rec"])]}

    def pick(d, **kw):
        return [{"task_id": d["task_id"], "source_id": d["s_par"], "feature_id": "p160", **kw}]

    route_case("E15 route import recovery suggestions", "/preview", plain, status=200)
    token_parity("E15 route import recovery token parity", recovery)
    route_case("E15 route import recovery preview", "/preview", recovery, status=200)
    route_case(
        "E15 route import invalid merge group",
        "/preview",
        lambda d: recovery(d, merge_groups=["recovery-" + "0" * 32]),
    )
    route_case(
        "E15 route import duplicate merge group",
        "/preview",
        lambda d: recovery(d, merge_groups=[d["merge_id"]] * 2),
    )
    route_case(
        "E15 route import stale parent task",
        "/preview",
        lambda d: recovery(d, parent_features=pick(d, task_id="0" * 32)),
    )
    route_case(
        "E15 route import duplicate parent task",
        "/preview",
        lambda d: recovery(d, parent_features=pick(d) + pick(d, feature_id="p160b")),
    )
    route_case(
        "E15 route import parent feature already selected",
        "/preview",
        lambda d: recovery(d, parent_features=pick(d, source_id=d["s_rec"], feature_id="P")),
    )
    route_case(
        "E15 route import invalid parent feature",
        "/preview",
        lambda d: recovery(d, parent_features=pick(d, feature_id="missing")),
    )
    route_case(
        "E15 route import bad parent task pattern",
        "/preview",
        lambda d: recovery(d, parent_features=pick(d, task_id="XYZ")),
    )
    route_case(
        "E15 route import locked identity",
        "/preview",
        lambda d: recovery(d, overrides={d["key"]["M1"]: {"parent_code": "", "prefix": "155/8"}}),
    )
    cross_apply(
        "E15 route import recovery apply (cross-runtime token)",
        recovery,
        lambda d: recovery(d, selected_keys=[d["key"]["M1"], d["key"]["p160"], d["key"]["P"]]),
    )
    route_case("E15 route import recovery after apply", "/preview", plain, status=200)


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
            from modules.house_numbering.migrate import (
                upgrade as upgrade_house_numbering,
            )

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
                    batch_field(ctx)
                    batch_photos(ctx)
                    batch_records(ctx)
                    batch_numbering(ctx)
                    batch_geometry(ctx)
                    batch_building_assignment(ctx)
                    batch_route_import(ctx)
            finally:
                get_engine().dispose()

    print(f"E15 full parity PASS ({len(PASSED)} cases)", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
