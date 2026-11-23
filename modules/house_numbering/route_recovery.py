"""Address and topology evidence for reviewable route import.

Suggestions never change the plan. A legacy review row requires explicit review,
matching address hierarchy and a measured connection to its immediate parent.
"""

from __future__ import annotations

import json
import re
from collections import defaultdict
from typing import Any

from sqlalchemy import text

from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.building_assignment import (
    RouteIndex,
    alley_name,
    normalize,
    road_name,
)

MAX_JUNCTION_M = 8.0


def normalized_road(name: str) -> str:
    result = road_name(name)
    return "quoc lo 50" if result in {"ql50", "ql 50", "quoc lo 50"} else result


def address_label(name: str) -> tuple[str, str] | None:
    """Recognize a full street + alley label, without guessing an unnamed street."""
    match = re.fullmatch(
        r"(.+?)\s+(?:hem|ngo|ngach|nhanh|alley)\s+([a-z0-9]+(?:/[a-z0-9]+)*)",
        normalize(name),
    )
    if not match:
        return None
    try:
        return normalized_road(match[1]), canonical_prefix(match[2])
    except ValueError:
        return None


def expected_parent(row: dict[str, Any]) -> tuple[str, str] | None:
    label = address_label(row["name"])
    if label:
        root, full = label
        return root, full.rpartition("/")[0]
    try:
        full = canonical_prefix(row["prefix"])
    except ValueError:
        return None
    return "", full.rpartition("/")[0]


def selection_rows(
    plan: dict[str, Any], selected: list[str] | None
) -> list[dict[str, Any]]:
    """Select only valid new rows; dependencies must be explicitly selected."""
    from openvietgis_api.domains.platform_errors import PlatformValidationError

    if selected is None:
        if plan["errors"]:
            raise PlatformValidationError(
                "Cần xử lý toàn bộ lỗi trước khi áp dụng", code="ROUTE_IMPORT_ERRORS"
            )
        return plan["items"]
    keys = set(selected)
    by_key = {r["key"]: r for r in plan["items"]}
    if (
        not keys
        or len(keys) != len(selected)
        or any(k not in by_key or by_key[k]["state"] != "ready" for k in keys)
    ):
        raise PlatformValidationError(
            "Chỉ chọn các tuyến mới đã hợp lệ", code="INVALID_ROUTE_SELECTION"
        )
    for k in keys:
        parent = by_key[k].get("parent_key")
        if parent and (
            parent not in by_key
            or by_key[parent]["state"] not in {"ready", "existing"}
            or by_key[parent]["state"] == "ready"
            and parent not in keys
        ):
            raise PlatformValidationError(
                "Cần chọn cả tuyến cha mới trước khi áp dụng",
                code="ROUTE_PARENT_NOT_SELECTED",
            )
    return [r for r in plan["items"] if r["state"] == "existing" or r["key"] in keys]


def survey_evidence(
    session: Any,
    params: dict[str, Any],
    rows: list[dict[str, Any]],
    existing: list[dict[str, Any]] | None = None,
) -> None:
    # Only aggregate legacy surveyed address attributes. Never expose occupant data or change houses.
    pending = [r for r in rows if r.get("review_reason") and not r.get("existing_id")]
    if not pending:
        return
    codes = {
        c
        for row in pending
        for c in [row["code"]]
        + [s["code"] for s in row.get("segments") or [] if s.get("code")]
    }
    index = RouteIndex(existing or [])
    observations: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for item in session.execute(
        text("""
        SELECT f.attributes->>'tuyen_id' code, f.attributes->>'ten_duong' road,
               f.attributes->>'hem' alley, b.route_id IS NULL unassigned, count(*) amount,
               (array_agg(b.linked_source_id::text ORDER BY b.id))[1:20] example_sources,
               (array_agg(b.linked_feature_id ORDER BY b.id))[1:20] example_features
        FROM house_numbering.buildings b JOIN spatial.reference_features f
          ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active'
          AND f.attributes->>'tuyen_id'=ANY(:codes)
        GROUP BY code,road,alley,unassigned ORDER BY code,road,alley,unassigned
    """),
        {**params, "codes": sorted(codes)},
    ).mappings():
        observations[item["code"]].append(dict(item))
    for row in pending:
        row_codes = {row["code"]} | {
            s["code"] for s in row.get("segments") or [] if s.get("code")
        }
        label = address_label(row["name"])
        root, full = label or (normalized_road(row["name"]), "")
        total = unassigned = conflict = 0
        review_groups = []
        review_counts = defaultdict(int)
        for code in sorted(row_codes):
            for v in observations[code]:
                amount = v["amount"]
                total += amount
                if v["unassigned"]:
                    unassigned += amount
                road, alley = normalized_road(v["road"]), alley_name(v["alley"])
                if road and road != root:
                    kind = "different_road"
                elif alley and alley != full.lower():
                    kind = (
                        "descendant"
                        if road == root
                        and full
                        and alley.startswith(full.lower() + "/")
                        else "different_branch"
                    )
                elif not road:
                    kind = "missing_road"
                elif full and not alley:
                    kind = "missing_prefix"
                else:
                    kind = "exact"
                if kind in {"different_road", "descendant", "different_branch"}:
                    conflict += amount
                review_counts[kind] += amount
                if kind != "exact":
                    candidates, _ = index.match(v["road"], v["alley"], "", True)
                    review_groups.append(
                        {
                            "kind": kind,
                            "road": v["road"] or "",
                            "alley": v["alley"] or "",
                            "code": code,
                            "count": amount,
                            "unassigned": bool(v["unassigned"]),
                            "candidate_count": len(candidates),
                            "examples": [
                                {"source_id": s, "feature_id": f}
                                for s, f in zip(
                                    v["example_sources"], v["example_features"]
                                )
                            ],
                        }
                    )
        row["survey_evidence"] = {
            "referenced": total,
            "unassigned": unassigned,
            "address_conflicts": conflict,
        }
        # Explicit diagnostic only: no waiver of conflict or linked-house guard.
        row["survey_review"] = {
            "counts": dict(review_counts),
            "groups": sorted(
                review_groups,
                key=lambda g: (
                    g["kind"] in {"missing_road", "missing_prefix"},
                    -g["count"],
                    g["code"],
                    g["road"],
                    g["alley"],
                ),
            )[:100],
            "total_groups": len(review_groups),
        }
        if conflict:
            row["errors"].append("review_survey_address_conflict")
        if total != unassigned:
            row["errors"].append("review_building_already_linked")
        if row["errors"]:
            row["state"] = "error"


def recover_parent_evidence(
    session: Any,
    params: dict[str, Any],
    rows: list[dict[str, Any]],
    existing: list[dict[str, Any]],
) -> None:
    index = RouteIndex(existing)
    by_key = {r["key"]: r for r in rows}
    candidates: dict[str, dict[str, Any]] = {}
    paths: dict[tuple[str, str], list[str]] = defaultdict(list)
    by_prefix: dict[str, list[str]] = defaultdict(list)
    resolved_roots: dict[str, str] = {}
    by_code: dict[str, str] = {}

    for rid, route in index.routes.items():
        if rid not in index.root:
            continue
        root = normalized_road(index.root_name[rid])
        prefix = route["prefix"] or ""
        candidates[rid] = {
            "code": rid,
            "name": route["name"],
            "root": root,
            "prefix": prefix,
            "state": "existing",
            "revision": route["revision"],
        }
        paths[(root, prefix)].append(rid)
        by_prefix[prefix].append(rid)

    # Planner returns parent-first order. Prefer resolved root over a name parsed from an unverified child.
    for row in rows:
        key = row["key"]
        parent = row.get("parent_key") or row.get("parent_id")
        root = (
            normalized_road(row["name"])
            if row["route_type"] == "street"
            else resolved_roots.get(parent, candidates.get(parent, {}).get("root", ""))
        )
        resolved_roots[key] = root
        if row.get("existing_id"):
            rid = row["existing_id"]
            by_code[row["code"]] = rid
            continue
        by_code[row["code"]] = key
        label = address_label(row["name"])
        if label:
            root, prefix = label
        elif row["route_type"] == "street":
            prefix = ""
        else:
            prefix = row.get("resolved_prefix")
            if not prefix:
                try:
                    prefix = canonical_prefix(row["prefix"])
                except ValueError:
                    continue
        candidates[key] = {
            "code": row["code"],
            "name": row["name"],
            "root": root,
            "prefix": prefix,
            "state": row["state"],
        }
        paths[(root, prefix)].append(key)
        by_prefix[prefix].append(key)

    # Same full address path cannot be imported twice by selecting only one of its conflicting segments.
    for row in rows:
        if not row.get("review_reason") or row.get("existing_id"):
            continue
        candidate = candidates.get(row["key"])
        if (
            candidate
            and candidate["root"]
            and len(paths[(candidate["root"], candidate["prefix"])]) > 1
        ):
            row["errors"].append("review_duplicate_address_path")
            row["state"] = "error"
            candidate["state"] = "error"
    pairs = []
    inspected = {}
    for row in rows:
        if row.get("existing_id") or (
            row["route_type"] != "alley" and not address_label(row["name"])
        ):
            continue
        if not row.get("review_reason") and not set(row["errors"]).intersection(
            {
                "parent_not_found",
                "parent_required",
                "prefix_does_not_follow_parent",
                "prefix_must_be_one_level",
                "parent_has_errors",
                "parent_prefix_missing",
                "street_cannot_have_parent_or_prefix",
            }
        ):
            continue
        expect = expected_parent(row)
        if expect is None:
            continue
        root, prefix = expect
        row["expected_parent_prefix"] = prefix
        row["expected_root"] = root
        row["suggested_type"] = "alley"
        label = address_label(row["name"])
        if label:
            row["suggested_prefix"] = label[1]
        actual = row.get("parent_key") or row.get("parent_id")
        if actual in by_key and by_key[actual].get("existing_id"):
            actual = by_key[actual]["existing_id"]
        candidates_for_row = list(
            paths.get((root, prefix), []) if root else by_prefix.get(prefix, [])
        )
        if actual and actual in candidates and actual not in candidates_for_row:
            candidates_for_row.append(actual)
        # Bound comparison size for broad, unnamed paths.
        if len(candidates_for_row) > 250:
            row["parent_search_truncated"] = True
            candidates_for_row = candidates_for_row[:250] + (
                [actual]
                if actual
                and actual not in candidates_for_row[:250]
                and actual in candidates
                else []
            )
        inspected[row["key"]] = (row, root, prefix, actual)
        for other in candidates_for_row:
            if other != row["key"]:
                pairs.append({"child": row["key"], "parent": other})

    needed = {p["child"] for p in pairs} | {p["parent"] for p in pairs}
    specs = []
    for key in sorted(needed.intersection(by_key)):
        row = by_key[key]
        links = row.get("segments") or [
            {"source_id": row["source_id"], "feature_id": row["feature_id"]}
        ]
        specs.append({"key": key, "links": links})
    distances = {}
    if pairs:
        values = session.execute(
            text("""
            WITH specs AS MATERIALIZED (
              SELECT v->>'key' key,v->'links' links FROM jsonb_array_elements(CAST(:specs AS jsonb)) v
            ), planned_lines AS MATERIALIZED (
              SELECT s.key,house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) geom,
                     string_agg(f.content_revision,'|' ORDER BY l.ord) AS revision
              FROM specs s CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY l(v,ord)
              LEFT JOIN spatial.reference_features f ON f.organization_id=:org AND f.project_id=:project
                AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
              LEFT JOIN spatial.data_sources ds ON ds.organization_id=f.organization_id AND ds.project_id=f.project_id AND ds.id=f.source_id
              GROUP BY s.key HAVING bool_and(COALESCE(f.geometry_state='present' AND ds.lifecycle='active' AND ds.validation_state='ready',false))
            ), lines AS MATERIALIZED (
              SELECT key,ST_Transform(geom,4326)::geography g,revision FROM planned_lines WHERE geom IS NOT NULL
              UNION ALL
              SELECT route_id::text,ST_Transform(geometry,4326)::geography,content_revision FROM house_numbering.route_lines
              WHERE organization_id=:org AND project_id=:project AND route_id::text=ANY(:existing_ids)
            ), comparisons AS (
              SELECT v->>'child' child,v->>'parent' parent FROM jsonb_array_elements(CAST(:pairs AS jsonb)) v
            )
            SELECT c.child,c.parent,b.revision,LEAST(ST_Distance(ST_StartPoint(a.g::geometry)::geography,b.g),
                    ST_Distance(ST_EndPoint(a.g::geometry)::geography,b.g)) distance
            FROM comparisons c JOIN lines a ON a.key=c.child JOIN lines b ON b.key=c.parent
        """),
            {
                **params,
                "specs": json.dumps(specs),
                "pairs": json.dumps(pairs),
                "existing_ids": sorted(needed.intersection(index.routes)),
            },
        ).mappings()
        distances = {
            (v["child"], v["parent"]): (float(v["distance"]), v["revision"])
            for v in values
            if v["distance"] is not None
        }

    results: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for pair in pairs:
        child, parent = pair["child"], pair["parent"]
        row, root, prefix, actual = inspected[child]
        candidate = candidates[parent]
        distance, geometry_revision = distances.get((child, parent), (None, None))
        match = bool(
            root and candidate["root"] == root and candidate["prefix"] == prefix
        )
        results[child].append(
            {
                **candidate,
                "distance_m": round(distance, 2) if distance is not None else None,
                "address_match": match,
                "connected": distance is not None and distance <= MAX_JUNCTION_M,
                "current": parent == actual,
                "geometry_revision": geometry_revision,
            }
        )
    # A newly supplied parent feature must physically meet at least one
    # immediate child with the intended address, not merely be a valid line.
    manual_keys = {
        r["key"] for r in rows if r.get("manual_parent") and not r.get("existing_id")
    }
    child_matches: dict[str, set[str]] = defaultdict(set)
    for pair in pairs:
        child, parent = pair["child"], pair["parent"]
        if parent not in manual_keys:
            continue
        _, root, prefix, _ = inspected[child]
        candidate = candidates[parent]
        distance, _ = distances.get((child, parent), (None, None))
        if (
            root
            and candidate["root"] == root
            and candidate["prefix"] == prefix
            and distance is not None
            and distance <= MAX_JUNCTION_M
        ):
            child_matches[parent].add(child)
    for row in rows:
        if row["key"] in manual_keys:
            row["manual_parent_child_count"] = len(child_matches[row["key"]])
            if not row["manual_parent_child_count"]:
                row["errors"].append("review_parent_has_no_connected_child")
                row["state"] = "error"
    for key, (row, root, prefix, actual) in inspected.items():
        options = sorted(
            results[key],
            key=lambda v: (
                not v["address_match"],
                not v["connected"],
                v["distance_m"] if v["distance_m"] is not None else float("inf"),
                v["code"],
            ),
        )
        row["parent_suggestions"] = options[:6]
        current = next((v for v in options if v["current"]), None)
        if current and current not in row["parent_suggestions"]:
            row["parent_suggestions"].append(current)
        row["parent_evidence"] = current
        if row.get("review_reason") and row["route_type"] == "alley":
            # A review cannot certify a different root simply by relabeling the parent.
            label = address_label(row["name"])
            actual_root = resolved_roots.get(key, "")
            if (
                not label
                or label[1] != row.get("resolved_prefix")
                or not current
                or not current["address_match"]
                or actual_root != root
            ):
                row["errors"].append("review_address_mismatch")
            if not current or not current["connected"]:
                row["errors"].append("review_parent_not_connected")
            if row["errors"]:
                row["state"] = "error"
