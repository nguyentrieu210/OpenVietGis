"""Advisory address candidates for the geometry review queue; never persist decisions."""

from __future__ import annotations

import json
from collections import defaultdict
from typing import Any

from sqlalchemy import text

from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.building_assignment import (
    RouteIndex,
    alley_name,
    road_name,
)

NEAR_M = 80.0
CLOSE_M = 35.0
UNKNOWN_ROADS = {
    "",
    "0",
    "none",
    "null",
    "unknown",
    "khong ro",
    "chua ro",
    "chua ro ten",
    "chua co ten",
    "khong ten",
    "chua xac dinh",
}


def candidate(
    row: dict, route: dict, distance: float, stage_road: str, stage_prefix: str
) -> dict:
    raw_road, raw_prefix = road_name(row["road"]), alley_name(row["alley"])
    name, prefix = road_name(route["road"]), alley_name(route["prefix"])
    road_match = bool(raw_road and raw_road not in UNKNOWN_ROADS and raw_road == name)
    prefix_match = bool(raw_prefix and raw_prefix == prefix)
    road_conflict = raw_road not in UNKNOWN_ROADS and not road_match
    prefix_conflict = bool(raw_prefix and not prefix_match)
    stage = route["origin"] == "staged"
    same = name == road_name(stage_road) and prefix == alley_name(stage_prefix)
    # Missing prefix is UNKNOWN, not evidence of street frontage.
    score = (
        30 * road_match
        + 35 * prefix_match
        + 15 * stage
        + 20 * max(0, 1 - distance / NEAR_M)
    )
    score -= 20 * road_conflict + 20 * prefix_conflict
    signals = []
    if road_match:
        signals.append("road_match")
    if prefix_match:
        signals.append("prefix_match")
    if stage:
        signals.append("original_code")
    if distance <= CLOSE_M:
        signals.append("within_35m")
    elif distance <= NEAR_M:
        signals.append("within_80m")
    else:
        signals.append("far")
    if road_conflict:
        signals.append("road_conflict")
    if prefix_conflict:
        signals.append("prefix_conflict")
    if raw_road in UNKNOWN_ROADS:
        signals.append("missing_road")
    if not raw_prefix:
        signals.append("missing_prefix")
    if route.get("status") == "draft":
        signals.append("draft_route")
    usable = (
        row["eligible"]
        and distance <= (CLOSE_M if same else NEAR_M)
        and (not same or row["can_belong"])
    )
    return {
        **route,
        "distance_m": round(distance, 2),
        "score": round(score, 1),
        "signals": signals,
        "full_match": road_match and prefix_match,
        "conflict": road_conflict or prefix_conflict,
        "can_prefill": bool(usable),
        "decision": "belongs" if same else "other_address",
    }


def rank(
    row: dict,
    available: list,
    stage_road: str,
    stage_prefix: str,
    ambiguous: bool = False,
) -> dict:
    if not row["eligible"]:
        return {"priority": "blocked", "candidates": []}
    candidates = [candidate(row, r, d, stage_road, stage_prefix) for r, d in available]
    candidates.sort(key=lambda x: (-x["score"], x["distance_m"], x["key"]))
    exact = [c for c in candidates if c["full_match"]]
    if (
        not ambiguous
        and len(exact) == 1
        and exact[0]["distance_m"] <= CLOSE_M
        and exact[0]["can_prefill"]
        and exact[0]["status"] == "confirmed"
    ):
        priority = "strong"
    elif (
        not ambiguous
        and len(exact) <= 1
        and candidates
        and candidates[0]["can_prefill"]
        and not candidates[0]["conflict"]
        and candidates[0]["distance_m"] <= CLOSE_M
    ):
        priority = "partial"
    elif any(c["can_prefill"] for c in candidates):
        priority = "conflict"
    else:
        priority = "unmatched"
    shown = candidates[:5]
    stage = next((c for c in candidates if c["origin"] == "staged"), None)
    if stage and stage not in shown:
        shown.append(stage)
    return {"priority": priority, "candidates": shown}


def add_suggestions(repo: Any, stage: dict, items: list[dict]) -> dict:
    """Enrich only the current page of the read-only queue, not promotion or save."""
    counts = {k: 0 for k in ("strong", "partial", "conflict", "unmatched", "blocked")}
    if not items:
        return counts
    s = stage["snapshot"]
    params = {"org": repo.org_id, "project": repo.project_id}
    routes = [
        dict(r)
        for r in repo.session.execute(
            text("""
       SELECT id,name,route_type,parent_route_id,prefix,source_label,status,revision,segments,reviewed_aliases,
         linked_source_id,linked_feature_id
       FROM house_numbering.routes WHERE organization_id=:org AND project_id=:project ORDER BY id
    """),
            params,
        ).mappings()
    ]
    index = RouteIndex(routes)
    parent = str(s["parent_id"])
    if parent not in index.root:
        for row in items:
            priority = "unmatched" if row["eligible"] else "blocked"
            row["suggestions"] = {"priority": priority, "candidates": []}
            counts[priority] += 1
        return counts
    stage_road = index.routes[index.root[parent]]["name"]
    stage_prefix = s["resolved_prefix"]
    from modules.house_numbering.route_recovery import address_label

    label = address_label(s["name"])
    if not label or (road_name(stage_road), alley_name(stage_prefix)) != (
        label[0],
        label[1].lower(),
    ):
        for row in items:
            priority = "unmatched" if row["eligible"] else "blocked"
            row["suggestions"] = {"priority": priority, "candidates": []}
            counts[priority] += 1
        return counts
    stage_route = {
        "key": "stage:" + str(stage["id"]),
        "origin": "staged",
        "name": s["name"],
        "road": stage_road,
        "prefix": stage_prefix,
        "status": "staged",
        "source_id": s["source_id"],
        "feature_id": s["feature_id"],
    }
    catalog = {}
    by_address = defaultdict(list)
    for rid, r in index.routes.items():
        if rid not in index.root:
            continue
        root = index.routes[index.root[rid]]["name"]
        if road_name(root) in UNKNOWN_ROADS or "nuoc thai" in road_name(root):
            continue
        try:
            prefix = canonical_prefix(r["prefix"]) if r["route_type"] == "alley" else ""
        except (ValueError, TypeError):
            continue
        catalog[rid] = {
            "key": "route:" + rid,
            "route_id": rid,
            "origin": "catalog",
            "name": r["name"],
            "road": root,
            "prefix": prefix,
            "status": r["status"],
            "source_id": str(r["linked_source_id"]),
            "feature_id": r["linked_feature_id"],
        }
        by_address[(road_name(root), alley_name(prefix))].append(rid)
    pairs = []
    for row in items:
        if not row["eligible"]:
            continue
        name, branch = road_name(row["road"]), alley_name(row["alley"])
        if name not in UNKNOWN_ROADS and branch:
            for rid in by_address[(name, branch)][:20]:
                pairs.append({"building_id": row["building_id"], "route_id": rid})
    ids = [row["building_id"] for row in items if row["eligible"]]
    distances = defaultdict(dict)
    if ids and catalog:
        for rec in repo.session.execute(
            text("""
          WITH buildings AS MATERIALIZED (
            SELECT b.id,ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography g
            FROM house_numbering.buildings b JOIN spatial.reference_features f
              ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
            JOIN spatial.data_sources ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
            WHERE b.organization_id=:org AND b.project_id=:project AND b.id=ANY(CAST(:ids AS uuid[]))
              AND f.geometry_state='present' AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
              AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND ds.lifecycle='active' AND ds.validation_state='ready'
          ), lines AS MATERIALIZED (
            SELECT l.route_id,ST_Transform(l.geometry,4326)::geography g
            FROM house_numbering.route_lines l WHERE l.organization_id=:org AND l.project_id=:project
                AND l.route_id=ANY(CAST(:route_ids AS uuid[]))
          ), near AS (
            SELECT b.id building_id,c.route_id,c.distance FROM buildings b CROSS JOIN LATERAL (
              SELECT l.route_id,ST_Distance(b.g,l.g) distance FROM lines l
              WHERE ST_DWithin(b.g,l.g,:radius) ORDER BY distance,l.route_id LIMIT 8
            ) c
          ), exact AS (
            SELECT b.id building_id,l.route_id,ST_Distance(b.g,l.g) distance
            FROM jsonb_to_recordset(CAST(:pairs AS jsonb)) p(building_id uuid,route_id uuid)
            JOIN buildings b ON b.id=p.building_id JOIN lines l ON l.route_id=p.route_id
          )
          SELECT * FROM near UNION SELECT * FROM exact
        """),
            {
                **params,
                "ids": ids,
                "route_ids": list(catalog),
                "radius": NEAR_M,
                "pairs": json.dumps(pairs),
            },
        ).mappings():
            distances[str(rec["building_id"])][str(rec["route_id"])] = float(
                rec["distance"]
            )
    for row in items:
        available = [
            (catalog[rid], d) for rid, d in distances[row["building_id"]].items()
        ]
        if row["distance_m"] is not None:
            available.append((stage_route, row["distance_m"]))
        name, branch = road_name(row["road"]), alley_name(row["alley"])
        ambiguous = bool(branch and len(by_address[(name, branch)]) > 1)
        result = rank(row, available, stage_road, stage_prefix, ambiguous)
        row["suggestions"] = result
        counts[result["priority"]] += 1
    return counts
