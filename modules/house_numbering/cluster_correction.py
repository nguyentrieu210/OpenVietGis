"""Explicit address-based correction of unlinked survey-code conflict clusters.

The source observation is immutable. A confirmed correction writes only the
canonical building.route_id and an audit trail; it does not issue a number.
"""

from __future__ import annotations

import hashlib
import json
import math
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


def cluster_key(road: Any, alley: Any, code: Any) -> str:
    return hashlib.sha256(
        json.dumps(
            [road_name(road), alley_name(alley), str(code or "").strip()],
            ensure_ascii=False,
        ).encode()
    ).hexdigest()[:24]


def correction_preview(
    repo: Any,
    *,
    cluster: str,
    building_ids: list[UUID],
    mapping: dict[str, str],
    max_distance_m: float = 35,
) -> dict[str, Any]:
    if (
        not building_ids
        or len(building_ids) > 500
        or len(set(building_ids)) != len(building_ids)
    ):
        raise PlatformValidationError(
            "Lô đối soát phải có 1–500 nhà khác nhau", code="INVALID_CORRECTION_BATCH"
        )
    repo.prepare("edit")
    repo.prepare("read")
    params = {
        "org": repo.org_id,
        "project": repo.project_id,
        "ids": building_ids,
        "road": mapping.get("road", ""),
        "alley": mapping.get("alley", ""),
        "code": mapping.get("code", ""),
    }
    session = repo.session
    routes = [
        dict(r)
        for r in session.execute(
            text("""
        SELECT r.id,r.name,r.route_type,r.parent_route_id,r.prefix,r.source_label,r.status,r.revision,r.segments,r.reviewed_aliases,f.content_revision
        FROM house_numbering.routes r JOIN house_numbering.route_lines f
          ON (f.organization_id,f.project_id,f.route_id)=(r.organization_id,r.project_id,r.id)
        WHERE r.organization_id=:org AND r.project_id=:project ORDER BY r.id
    """),
            params,
        ).mappings()
    ]
    idx = RouteIndex(routes)
    data = (
        session.execute(
            text("""
        SELECT b.id,b.revision,b.route_id,b.state,b.eligibility,b.linked_source_id,b.linked_feature_id,
          f.content_revision,f.attributes->>:road road,f.attributes->>:alley alley,f.attributes->>:code code,
          COALESCE(jsonb_exists(f.attributes,:alley),false) AND :alley<>'' AS alley_mapped,
          COALESCE(f.geometry_state='present' AND NOT ST_IsEmpty(f.geometry) AND ST_SRID(f.geometry)>0
              AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND s.lifecycle='active' AND s.validation_state='ready',false) geometry_usable,
          EXISTS(SELECT 1 FROM house_numbering.address_numbers n
                 WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id AND n.building_id=b.id
                   AND n.retired_at IS NULL AND n.kind IN ('submitted','approved','reserved','proposed')) locked
        FROM house_numbering.buildings b
        LEFT JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        LEFT JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
        WHERE b.organization_id=:org AND b.project_id=:project AND b.id=ANY(CAST(:ids AS uuid[]))
        ORDER BY b.id
    """),
            params,
        )
        .mappings()
        .all()
    )
    if len(data) != len(building_ids):
        raise PlatformConflictError(
            "Danh sách nhà đã thay đổi", code="STALE_CORRECTION"
        )
    items = []
    candidates: set[str] = set()
    original: set[str] = set()
    for row in data:
        if (
            row["route_id"] is not None
            or row["state"] != "active"
            or cluster_key(row["road"], row["alley"], row["code"]) != cluster
        ):
            raise PlatformConflictError(
                "Nhà không còn thuộc cụm chưa liên kết", code="STALE_CORRECTION"
            )
        road = road_name(row["road"])
        code = str(row["code"] or "").strip()
        _, source_reason = idx.match(
            row["road"], row["alley"], code, row["alley_mapped"]
        )
        possible, address_reason = idx.match(
            row["road"], row["alley"], "", row["alley_mapped"]
        )
        if (
            not road
            or not row["alley_mapped"]
            or road in {"chua ro ten", "chua co ten", "khong ten"}
            or "nuoc thai" in road
            or address_reason != "exact_address"
        ):
            possible = set()
        if source_reason != "attribute_conflict":
            raise PlatformValidationError(
                "Cụm không có mâu thuẫn mã tuyến cần sửa", code="NOT_CODE_CONFLICT"
            )
        candidates.update(possible)
        original.update(idx.codes.get(code, set()))
        eligible = bool(
            row["geometry_usable"]
            and not row["locked"]
            and row["eligibility"] != "ineligible"
        )
        items.append(
            {
                "building_id": str(row["id"]),
                "revision": row["revision"],
                "source_id": str(row["linked_source_id"]),
                "feature_id": row["linked_feature_id"],
                "content_revision": row["content_revision"],
                "road": str(row["road"] or ""),
                "alley": str(row["alley"] or ""),
                "code": code,
                "eligible": eligible,
                "blocked_reason": "protected_number"
                if row["locked"]
                else "ineligible"
                if row["eligibility"] == "ineligible"
                else "invalid_geometry"
                if not row["geometry_usable"]
                else None,
                "distances": {},
                "old_distances": {},
            }
        )
    if len(candidates) > 30:
        # Do not truncate an ambiguous address and accidentally imply a unique route.
        raise PlatformValidationError(
            "Địa chỉ có quá nhiều tuyến trùng; cần sửa danh mục tuyến trước",
            code="AMBIGUOUS_ROUTE",
        )
    distances = {}
    all_routes = candidates | original
    if all_routes:
        points = session.execute(
            text("""
            WITH buildings AS MATERIALIZED (
                SELECT b.id,ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography g
                FROM house_numbering.buildings b JOIN spatial.reference_features f
                  ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
                WHERE b.organization_id=:org AND b.project_id=:project AND b.id=ANY(CAST(:eligible AS uuid[]))
            ), routes AS MATERIALIZED (
                SELECT route_id,ST_Transform(geometry,4326)::geography g
                FROM house_numbering.route_lines WHERE organization_id=:org AND project_id=:project AND route_id=ANY(CAST(:routes AS uuid[]))
            )
            SELECT b.id,r.route_id,ST_Distance(b.g,r.g) distance FROM buildings b CROSS JOIN routes r
        """),
            {
                **params,
                "eligible": [x["building_id"] for x in items if x["eligible"]],
                "routes": sorted(all_routes),
            },
        ).mappings()
        for q in points:
            if q["distance"] is not None:
                distances[(str(q["id"]), str(q["route_id"]))] = float(q["distance"])
    for x in items:
        for rid in sorted(all_routes):
            d = distances.get((x["building_id"], rid))
            if d is None or not math.isfinite(d):
                continue
            value = {"distance_m": round(d, 2), "within_limit": d <= max_distance_m}
            if rid in candidates:
                x["distances"][rid] = value
            if rid in original:
                x["old_distances"][rid] = value

    def descriptor(rid: str) -> dict[str, Any]:
        r = idx.routes[rid]
        usable = sum(
            bool(x["eligible"] and x["distances"].get(rid, {}).get("within_limit"))
            for x in items
        )
        ds = [
            distances[(x["building_id"], rid)]
            for x in items
            if (x["building_id"], rid) in distances
        ]
        return {
            "route_id": rid,
            "name": r["name"],
            "root_name": idx.routes[idx.root[rid]]["name"],
            "prefix": r["prefix"],
            "revision": r["revision"],
            "content_revision": r["content_revision"],
            "status": r["status"],
            "within_limit": usable,
            "mean_distance_m": round(sum(ds) / len(ds), 2) if ds else None,
        }

    targets = sorted(
        [descriptor(x) for x in candidates],
        key=lambda x: (
            -x["within_limit"],
            x["mean_distance_m"] if x["mean_distance_m"] is not None else float("inf"),
            x["route_id"],
        ),
    )
    previous = [descriptor(x) for x in sorted(original)]
    payload = {
        "cluster": cluster,
        "items": items,
        "candidates": targets,
        "original_routes": previous,
        "max_distance_m": max_distance_m,
        "recommended_route_id": targets[0]["route_id"]
        if len(targets) == 1 and targets[0]["within_limit"]
        else None,
    }
    payload["preview_token"] = hashlib.sha256(
        json.dumps(
            payload,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            default=str,
        ).encode()
    ).hexdigest()
    return payload


def correction_apply(
    repo: Any,
    *,
    preview_token: str,
    route_id: UUID,
    selected_ids: list[UUID],
    reason: str,
    **kwargs: Any,
) -> dict[str, Any]:
    reason = reason.strip()
    if (
        len(reason) < 10
        or not selected_ids
        or len(set(selected_ids)) != len(selected_ids)
    ):
        raise PlatformValidationError(
            "Cần chọn nhà và ghi lý do đối soát rõ ràng", code="INVALID_CORRECTION"
        )
    repo.prepare("edit")
    repo.session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": f"house-numbering-assignment:{repo.project_id}"},
    )
    repo.get_route(route_id, for_update=True)
    plan = correction_preview(repo, **kwargs)
    if plan["preview_token"] != preview_token:
        raise PlatformConflictError(
            "Dữ liệu đối soát đã thay đổi; xem trước lại", code="STALE_CORRECTION"
        )
    rid = str(route_id)
    if not any(c["route_id"] == rid for c in plan["candidates"]):
        raise PlatformValidationError(
            "Tuyến đích không khớp đường/hẻm khảo sát", code="INVALID_CORRECTION"
        )
    by_id = {x["building_id"]: x for x in plan["items"]}
    for bid in selected_ids:
        row = by_id.get(str(bid))
        if (
            not row
            or not row["eligible"]
            or not row["distances"].get(rid, {}).get("within_limit")
        ):
            raise PlatformValidationError(
                "Nhà không đủ điều kiện khoảng cách hoặc bị khóa",
                code="INVALID_CORRECTION",
            )
    for bid in sorted(selected_ids):
        row = by_id[str(bid)]
        current = repo.get_building(bid, for_update=True)
        if current["route_id"] is not None or current["revision"] != row["revision"]:
            raise PlatformConflictError("Nhà đã thay đổi", code="STALE_CORRECTION")
        protected = repo.session.execute(
            text("""
            SELECT 1 FROM house_numbering.address_numbers WHERE organization_id=:org AND project_id=:project
              AND building_id=:building AND retired_at IS NULL AND kind IN ('proposed','submitted','approved','reserved') LIMIT 1
        """),
            {"org": repo.org_id, "project": repo.project_id, "building": bid},
        ).first()
        if protected:
            raise PlatformConflictError("Nhà đã có số bảo vệ", code="STALE_CORRECTION")
        repo.update_building(
            bid,
            expected_revision=row["revision"],
            route_id=route_id,
            area_id=current["area_id"],
            parcel_ref=current["parcel_ref"],
            source_id=current["linked_source_id"],
            feature_id=current["linked_feature_id"],
        )
        repo.audit(
            "house_numbering.building.correct_route_code",
            "house_numbering_building",
            bid,
            {
                "cluster": plan["cluster"],
                "survey_code": row["code"],
                "survey_road": row["road"],
                "survey_alley": row["alley"],
                "route_id": rid,
                "distance_m": row["distances"][rid]["distance_m"],
                "reason": reason,
                "preview_token": preview_token,
            },
        )
    repo.session.commit()
    return {
        "assigned": len(selected_ids),
        "route_id": rid,
        "remaining": len(plan["items"]) - len(selected_ids),
    }
