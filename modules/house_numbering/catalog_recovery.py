"""Review missing address paths using existing source geometry, never invent lines."""

from __future__ import annotations

import hashlib
import json
from collections import Counter, defaultdict
from typing import Any
from uuid import UUID

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformValidationError,
)
from sqlalchemy import text

from modules.house_numbering.building_assignment import (
    RouteIndex,
    alley_name,
    road_name,
)
from modules.house_numbering.route_recovery import address_label


def digest(value: Any) -> str:
    return hashlib.sha256(
        json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            default=str,
        ).encode()
    ).hexdigest()


def catalog_preview(
    repo: Any,
    *,
    mapping: dict[str, str],
    source_id: UUID | None = None,
    search: str = "",
    offset: int = 0,
    limit: int = 100,
    tree: bool = False,
) -> dict[str, Any]:
    repo.prepare("read")
    session = repo.session
    p = {
        "org": repo.org_id,
        "project": repo.project_id,
        "source": source_id,
        "road": mapping.get("road", ""),
        "alley": mapping.get("alley", ""),
        "code": mapping.get("code", ""),
    }
    routes = [
        dict(r)
        for r in session.execute(
            text("""
       SELECT r.*, l.content_revision,f.attributes->>'ten' source_name,f.attributes->>'ten_quy_hoach' planned_name
       FROM house_numbering.routes r JOIN house_numbering.route_lines l
        ON (r.organization_id,r.project_id,r.id)=(l.organization_id,l.project_id,l.route_id)
       LEFT JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(r.organization_id,r.project_id,r.linked_source_id,r.linked_feature_id)
       WHERE r.organization_id=:org AND r.project_id=:project ORDER BY r.id
    """),
            p,
        ).mappings()
    ]
    idx = RouteIndex(routes)
    occupied = {
        (str(ref["source_id"]), str(ref["feature_id"]))
        for r in routes
        for ref in (
            r.get("segments")
            or [
                {
                    "source_id": r["linked_source_id"],
                    "feature_id": r["linked_feature_id"],
                }
            ]
        )
    }
    source_paths = defaultdict(list)
    for f in session.execute(
        text("""
        SELECT f.source_id,f.feature_id,f.attributes->>'ten' name,f.attributes->>'prefix' prefix,
         f.attributes->>'route_type' route_type,f.attributes->>'route_code' code,f.attributes->>'review_reason' review_reason
        FROM spatial.reference_features f JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
        WHERE f.organization_id=:org AND f.project_id=:project AND s.lifecycle='active' AND s.validation_state='ready'
          AND f.geometry_state='present' AND ST_GeometryType(f.geometry)='ST_LineString' AND ST_SRID(f.geometry)>0
          AND nullif(f.attributes->>'ten','') IS NOT NULL
        ORDER BY f.source_id,f.feature_id
    """),
        p,
    ).mappings():
        if (str(f["source_id"]), str(f["feature_id"])) in occupied or idx.codes.get(
            str(f["code"] or "")
        ):
            continue
        label = address_label(f["name"])
        if label:
            key = (label[0], label[1].lower())
        elif f["route_type"] == "street" and not f["prefix"]:
            key = (road_name(f["name"]), "")
        else:
            continue
        source_paths[key].append(
            {
                "source_id": str(f["source_id"]),
                "feature_id": f["feature_id"],
                "name": f["name"],
                "code": f["code"],
                "reason": f["review_reason"],
            }
        )
    obs = (
        session.execute(
            text("""
      SELECT b.id,b.revision,b.eligibility,b.route_id,f.content_revision,
        f.attributes->>:road road,f.attributes->>:alley alley,f.attributes->>:code code,
        COALESCE(jsonb_exists(f.attributes,:alley),false) AND :alley<>'' mapped,
        COALESCE(f.geometry_state='present' AND NOT ST_IsEmpty(f.geometry) AND ST_SRID(f.geometry)>0
          AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND s.lifecycle='active' AND s.validation_state='ready',false) usable,
        EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
          AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('proposed','submitted','approved','reserved')) locked
      FROM house_numbering.buildings b JOIN spatial.reference_features f
       ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
      JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
      WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active'
        AND (CAST(:source AS uuid) IS NULL OR b.linked_source_id=:source)
      ORDER BY b.id
    """),
            p,
        )
        .mappings()
        .all()
    )
    groups = {}
    prefix_groups = {}
    missing = defaultdict(lambda: {"count": 0, "sources": [], "road": "", "alley": ""})
    counts = Counter()
    pairs = []
    by_id = {}
    code_observations = defaultdict(Counter)
    for row in obs:
        code = str(row["code"] or "").strip()
        name = road_name(row["road"])
        alley = alley_name(row["alley"])
        code_observations[code][(name, alley)] += 1
        if (
            row["route_id"]
            or row["locked"]
            or not row["usable"]
            or not row["mapped"]
            or not name
            or name in {"chua ro ten", "chua co ten", "khong ten"}
            or "nuoc thai" in name
        ):
            continue
        _, why = idx.match(row["road"], row["alley"], code, row["mapped"])
        exact, _ = idx.match(row["road"], row["alley"], "", True)
        if why != "attribute_conflict" or exact:
            continue
        counts["total"] += 1
        source = source_paths.get((name, alley), [])
        if source:
            counts["source_path_houses"] += 1
            x = missing[(name, alley)]
            x.update(road=str(row["road"]), alley=alley, sources=source[:10])
            x["count"] += 1
        coded = idx.codes.get(code, set())
        if len(coded) != 1:
            counts["ambiguous_code"] += 1
            continue
        rid = next(iter(coded))
        route = idx.routes[rid]
        if alley != alley_name(route["prefix"]):
            counts["prefix_mismatch"] += 1
            if tree:
                key = digest([name, alley])[:32]
                g = prefix_groups.setdefault(
                    key,
                    {
                        "id": key,
                        "road": str(row["road"]),
                        "normalized_road": name,
                        "alley": alley,
                        "codes": Counter(),
                        "old_routes": {},
                        "building_ids": [],
                    },
                )
                g["building_ids"].append(str(row["id"]))
                g["codes"][code] += 1
                g["old_routes"][rid] = {
                    "route_id": rid,
                    "name": idx.routes[idx.root[rid]]["name"],
                    "prefix": route["prefix"],
                    "source_id": str(route["linked_source_id"]),
                    "feature_id": route["linked_feature_id"],
                }
            continue
        if name == idx.root_name[rid]:
            counts["other"] += 1
            continue
        counts["same_prefix"] += 1
        if tree:
            continue
        key = digest([rid, name, alley, code])[:32]
        if key not in groups:
            groups[key] = {
                "id": key,
                "route_id": rid,
                "route_name": route["name"],
                "root_name": idx.routes[idx.root[rid]]["name"],
                "prefix": route["prefix"],
                "route_revision": route["revision"],
                "content_revision": route["content_revision"],
                "source_name": route["source_name"],
                "planned_name": route["planned_name"],
                "source_id": str(route["linked_source_id"]),
                "feature_id": route["linked_feature_id"],
                "road": str(row["road"]),
                "normalized_road": name,
                "alley": alley,
                "code": code,
                "count": 0,
                "near": 0,
                "observations": 0,
                "evidence": [],
            }
        g = groups[key]
        g["count"] += 1
        evidence = {
            "id": str(row["id"]),
            "revision": row["revision"],
            "content_revision": row["content_revision"],
            "distance": None,
            "eligible": row["eligibility"] != "ineligible",
        }
        g["evidence"].append(evidence)
        by_id[str(row["id"])] = (g, evidence)
        if evidence["eligible"]:
            pairs.append({"building_id": str(row["id"]), "route_id": rid})
    if tree:
        from modules.house_numbering.prefix_recovery import prefix_plan

        return prefix_plan(
            session, p, idx, source_paths, prefix_groups, search, offset, limit
        )
    if pairs:
        for row in session.execute(
            text("""
            WITH p AS MATERIALIZED (SELECT * FROM jsonb_to_recordset(CAST(:pairs AS jsonb)) AS p(building_id uuid,route_id uuid))
            SELECT p.building_id,ST_Distance(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,
              ST_Transform(l.geometry,4326)::geography) d
            FROM p JOIN house_numbering.buildings b ON b.id=p.building_id AND b.organization_id=:org AND b.project_id=:project
            JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
            JOIN house_numbering.route_lines l ON l.route_id=p.route_id AND l.organization_id=:org AND l.project_id=:project
        """),
            {**p, "pairs": json.dumps(pairs)},
        ).mappings():
            item, evidence = by_id[str(row["building_id"])]
            if row["d"] is not None:
                evidence["distance"] = round(float(row["d"]), 3)
                if row["d"] <= 35:
                    item["near"] += 1
    result = []
    for g in groups.values():
        observed = code_observations[g["code"]]
        g["observations"] = sum(observed.values())
        g["matching_observations"] = observed[(g["normalized_road"], g["alley"])]
        g["reviewable"] = g["near"] > 0
        g["token"] = digest(g)
        g.pop("evidence")
        result.append(g)
    counts["near"] = sum(g["near"] for g in result)
    counts["reviewable_groups"] = sum(g["reviewable"] for g in result)
    result.sort(key=lambda x: (-x["near"], -x["count"], x["id"]))
    needle = road_name(search)
    if needle:
        result = [
            g
            for g in result
            if needle
            in road_name(
                " ".join([g["road"], g["root_name"], g["code"], str(g["prefix"] or "")])
            )
        ]
    source_matches = sorted(missing.values(), key=lambda x: -x["count"])[:30]
    return {
        "counts": dict(counts),
        "total_groups": len(groups),
        "filtered_groups": len(result),
        "items": result[offset : offset + limit],
        "next_offset": offset + limit if len(result) > offset + limit else None,
        "source_matches": source_matches,
    }


def catalog_apply(
    repo: Any,
    *,
    mapping: dict[str, str],
    source_id: UUID | None,
    selections: dict[str, str],
    reason: str,
) -> dict[str, Any]:
    reason = reason.strip()
    if not selections or len(selections) > 100 or len(reason) < 10:
        raise PlatformValidationError(
            "Chọn tối đa 100 tên đối chiếu và ghi lý do", code="INVALID_ALIAS_SELECTION"
        )
    repo.prepare("edit")
    session = repo.session
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": f"house-numbering-assignment:{repo.project_id}"},
    )
    plan = catalog_preview(
        repo, mapping=mapping, source_id=source_id, offset=0, limit=100000
    )
    by_id = {g["id"]: g for g in plan["items"]}
    changes = defaultdict(list)
    for key, token in selections.items():
        g = by_id.get(key)
        if not g or not g["reviewable"] or g["token"] != token:
            raise PlatformConflictError(
                "Danh mục hoặc bằng chứng khảo sát đã thay đổi", code="STALE_ALIAS_PLAN"
            )
        changes[g["route_id"]].append(g)
    for rid, groups in sorted(changes.items()):
        route = repo.get_route(UUID(rid), for_update=True)
        if int(route["revision"]) != groups[0]["route_revision"]:
            raise PlatformConflictError("Tuyến đã thay đổi", code="STALE_ALIAS_PLAN")
        aliases = list(route["reviewed_aliases"] or [])
        keys = {(a["code"], a["normalized_road"], a["alley"]) for a in aliases}
        for g in groups:
            k = (g["code"], g["normalized_road"], g["alley"])
            if k in keys:
                raise PlatformConflictError(
                    "Tên đối chiếu đã tồn tại", code="STALE_ALIAS_PLAN"
                )
            keys.add(k)
            aliases.append(
                {
                    "code": g["code"],
                    "road": g["road"],
                    "normalized_road": g["normalized_road"],
                    "alley": g["alley"],
                }
            )
        if len(aliases) > 200:
            raise PlatformValidationError(
                "Quá nhiều tên đối chiếu cho một tuyến", code="ALIAS_LIMIT"
            )
        repo.prepare("edit")
        session.execute(
            text("""
          UPDATE house_numbering.routes SET reviewed_aliases=CAST(:aliases AS jsonb),revision=revision+1,
             updated_by_actor_id=:actor,updated_at=now()
          WHERE organization_id=:org AND project_id=:project AND id=:id
        """),
            {
                "org": repo.org_id,
                "project": repo.project_id,
                "actor": repo.actor_id,
                "id": UUID(rid),
                "aliases": json.dumps(aliases, ensure_ascii=False),
            },
        )
        for g in groups:
            repo.audit(
                "house_numbering.route.review_address_alias",
                "house_numbering_route",
                UUID(rid),
                {
                    "code": g["code"],
                    "road": g["road"],
                    "alley": g["alley"],
                    "near": g["near"],
                    "observations": g["observations"],
                    "reason": reason,
                    "token": g["token"],
                },
            )
    session.commit()
    return {
        "aliases": len(selections),
        "routes": len(changes),
        "potential_houses": sum(
            g["near"] for groups in changes.values() for g in groups
        ),
    }
