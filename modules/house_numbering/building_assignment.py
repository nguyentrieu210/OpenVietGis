"""Reviewable assignments of unlinked buildings to address routes."""
from __future__ import annotations

import hashlib
import json
import re
import unicodedata
from collections import defaultdict
from typing import Any
from uuid import UUID

from sqlalchemy import text
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError


def normalize(value: Any) -> str:
    raw = unicodedata.normalize("NFD", str(value or "").casefold().replace("đ", "d"))
    return " ".join(re.sub(r"[^a-z0-9/]+", " ", "".join(c for c in raw if not unicodedata.combining(c))).split())


def road_name(value: Any) -> str:
    return re.sub(r"^(duong|street)\s+", "", normalize(value))


def alley_name(value: Any) -> str:
    v = re.sub(r"^(hem|ngo|ngach|nhanh|alley)\s+", "", normalize(value))
    if v in {"", "0", "khong", "khong co", "mat tien", "none", "null"}:
        return ""
    return re.sub(r"\s*/\s*", "/", v)


class RouteIndex:
    def __init__(self, routes: list[dict[str, Any]]) -> None:
        self.routes = {str(r["id"]): r for r in routes}
        self.codes: dict[str, set[str]] = defaultdict(set)
        self.names: dict[str, set[str]] = defaultdict(set)
        self.aliases: dict[tuple[str, str], set[str]] = defaultdict(set)
        self.root: dict[str, str] = {}
        self.root_name: dict[str, str] = {}
        for rid, r in self.routes.items():
            current = r
            seen = {rid}
            while current.get("parent_route_id"):
                parent_id = str(current["parent_route_id"])
                if parent_id in seen or parent_id not in self.routes:
                    current = None
                    break
                seen.add(parent_id)
                current = self.routes[parent_id]
            if current is None or current["route_type"] != "street":
                continue
            root = str(current["id"])
            root_name = road_name(current["name"])
            self.root[rid] = root
            self.root_name[rid] = root_name
            self.codes[rid].add(rid)
            for segment in r.get("segments") or []:
                if segment.get("code"):
                    self.codes[str(segment["code"])].add(rid)
            label = r.get("source_label") or ""
            if label.startswith("route-tree-import:"):
                self.codes[label.removeprefix("route-tree-import:").strip()].add(rid)
            if r["route_type"] == "street":
                self.names[root_name].add(rid)
            elif r["route_type"] == "alley":
                for alias in {alley_name(r["prefix"]), alley_name(r["name"])} - {""}:
                    self.aliases[(root_name, alias)].add(rid)
                    self.aliases[("", alias)].add(rid)

    def match(self, road: Any, alley: Any, code: Any, alley_mapped: bool) -> tuple[set[str], str]:
        name, branch, key = road_name(road), alley_name(alley), str(code or "").strip()
        by_code = self.codes.get(key, set()) if key else set()
        candidates = self.aliases.get((name, branch), set()) if branch else self.names.get(name, set()) if name else set()
        if key:
            if not by_code:
                return candidates, "unknown_code"
            if name or branch:
                if candidates and not candidates.intersection(by_code):
                    return candidates | by_code, "attribute_conflict"
                if name and any(self.root_name.get(r) != name for r in by_code):
                    return by_code, "attribute_conflict"
                if branch and not candidates:
                    return by_code, "attribute_conflict"
                if alley_mapped and not branch and any(self.routes[r]["route_type"] == "alley" for r in by_code):
                    return by_code, "attribute_conflict"
            return by_code, "exact_code"
        if branch:
            return candidates, "exact_address" if name and candidates else "alley_only" if candidates else "unknown_address"
        if name:
            return candidates, "exact_address" if alley_mapped and candidates else "road_only" if candidates else "unknown_address"
        return set(), "no_attributes"


def score_candidates(index: RouteIndex, row: dict[str, Any], textual: set[str], reason: str,
                     distances: dict[str, float], max_distance: float) -> dict[str, Any]:
    if row["locked"] or not row["geometry_usable"]:
        return {**row, "confidence": "blocked", "reason": "official_number" if row["locked"] else "invalid_geometry", "candidates": [], "suggested_route_id": None}
    all_ids = textual | set(distances)
    exact = len(textual) == 1 and reason in {"exact_code", "exact_address"}
    items = []
    for rid in all_ids:
        if rid not in index.root or rid not in distances:
            continue
        r = index.routes[rid]
        distance = distances[rid]
        attr_matched = rid in textual
        score = (70 if attr_matched and exact else 45 if attr_matched else 0) + max(0, 30 * (1 - distance / max_distance))
        items.append({"route_id": rid, "name": r["name"], "root_name": index.routes[index.root[rid]]["name"],
                      "prefix": r["prefix"], "route_status": r["status"], "route_revision": r["revision"],
                      "content_revision": r["content_revision"], "distance_m": round(distance, 2), "score": round(score, 1),
                      "attribute_match": attr_matched})
    items.sort(key=lambda x: (-x["score"], x["distance_m"], x["route_id"]))
    if not items:
        return {**row, "confidence": "unmatched", "reason": reason, "candidates": [], "suggested_route_id": None}
    best = items[0]
    if exact and best["attribute_match"] and best["distance_m"] <= min(35.0, max_distance):
        confidence = "high"
    elif exact and best["attribute_match"] and best["distance_m"] <= max_distance:
        confidence = "medium"
    elif reason in {"attribute_conflict", "unknown_code", "unknown_address"} or len(textual) > 1:
        confidence = "conflict"
    else:
        confidence = "low"
    return {**row, "confidence": confidence, "reason": reason, "candidates": items[:10],
            "suggested_route_id": best["route_id"] if confidence in {"high", "medium"} else None}


class BuildingRouteAssigner:
    def __init__(self, repo: Any) -> None:
        self.repo = repo
        self.session = repo.session

    def preview(self, *, source_id: UUID | None, after_id: UUID | None, limit: int,
                mapping: dict[str, str], max_distance_m: float) -> dict[str, Any]:
        self.repo.prepare("edit")
        self.repo.prepare("read")
        params = {"org": self.repo.org_id, "project": self.repo.project_id,
                  "source": source_id, "after": after_id, "limit": limit + 1}
        routes = [dict(r) for r in self.session.execute(text("""
            SELECT r.id,r.name,r.route_type,r.parent_route_id,r.prefix,r.source_label,r.status,r.revision,r.segments,
                   f.content_revision
            FROM house_numbering.routes r JOIN house_numbering.route_lines f
                ON (f.organization_id,f.project_id,f.route_id)=(r.organization_id,r.project_id,r.id)
            WHERE r.organization_id=:org AND r.project_id=:project
            ORDER BY r.id
        """), params).mappings().all()]
        index = RouteIndex(routes)
        if not index.root:
            raise PlatformValidationError("Chưa có cây tuyến hợp lệ; hãy import và kiểm tra tuyến trước", code="ROUTES_REQUIRED")
        data = self.session.execute(text("""
            SELECT b.id,b.revision,b.linked_source_id,b.linked_feature_id,b.area_id,f.attributes,f.content_revision,
                   COALESCE(f.geometry_state='present' AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND ST_SRID(f.geometry)>0
                     AND s.lifecycle='active' AND s.validation_state='ready',false) AS geometry_usable,
                   EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                     AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('submitted','approved','reserved')) AS locked
            FROM house_numbering.buildings b LEFT JOIN spatial.reference_features f
              ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
            LEFT JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
            WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active' AND b.route_id IS NULL
                AND (CAST(:source AS uuid) IS NULL OR b.linked_source_id=:source)
                AND (CAST(:after AS uuid) IS NULL OR b.id>:after)
            ORDER BY b.id LIMIT :limit
        """), params).mappings().all()
        page = data[:limit]
        rows, matched, pairs = [], {}, []
        for b in page:
            a = b["attributes"] or {}
            road, alley, code = (a.get(mapping.get(k, "")) for k in ("road", "alley", "code"))
            rid = str(b["id"])
            ids, reason = index.match(road, alley, code, bool(mapping.get("alley") and mapping["alley"] in a))
            matched[rid] = (ids, reason)
            if b["geometry_usable"] and not b["locked"]:
                pairs.extend({"building_id":rid, "route_id":v} for v in sorted(ids)[:20])
            rows.append({"building_id": rid, "source_id": str(b["linked_source_id"]), "feature_id":str(b["linked_feature_id"]),
                         "revision":b["revision"], "content_revision":b["content_revision"], "area_id":str(b["area_id"]) if b["area_id"] else None,
                         "road":str(road or ""), "alley":str(alley or ""), "code":str(code or ""),
                         "geometry_usable":b["geometry_usable"], "locked":b["locked"]})
        ids = [r["building_id"] for r in rows if r["geometry_usable"] and not r["locked"]]
        distances: dict[str, dict[str, float]] = defaultdict(dict)
        if ids:
            queries = self.session.execute(text("""
                WITH buildings AS MATERIALIZED (
                  SELECT b.id,ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography g
                  FROM house_numbering.buildings b JOIN spatial.reference_features f
                    ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
                  WHERE b.organization_id=:org AND b.project_id=:project AND b.id=ANY(CAST(:ids AS uuid[]))
                ), routes AS MATERIALIZED (
                  SELECT r.id, ST_Transform(f.geometry,4326)::geography g
                  FROM house_numbering.routes r JOIN house_numbering.route_lines f
                    ON (f.organization_id,f.project_id,f.route_id)=(r.organization_id,r.project_id,r.id)
                  WHERE r.organization_id=:org AND r.project_id=:project AND r.id=ANY(CAST(:route_ids AS uuid[]))
                ), nearest AS (
                  SELECT b.id building_id,c.id route_id,c.distance FROM buildings b CROSS JOIN LATERAL (
                    SELECT r.id,ST_Distance(b.g,r.g) distance FROM routes r
                    WHERE ST_DWithin(b.g,r.g,:max_distance) ORDER BY distance,r.id LIMIT 5
                  ) c
                ), candidates AS (
                  SELECT b.id AS building_id,r.id AS route_id, ST_Distance(b.g,r.g) AS distance
                  FROM jsonb_to_recordset(CAST(:pairs AS jsonb)) AS p(building_id uuid,route_id uuid)
                  JOIN buildings b ON b.id=p.building_id JOIN routes r ON r.id=p.route_id
                )
                SELECT * FROM nearest UNION SELECT * FROM candidates
            """), {"org":self.repo.org_id,"project":self.repo.project_id,"ids":ids,"route_ids":sorted(index.root),
                   "pairs":json.dumps(pairs),"max_distance":max_distance_m}).mappings().all()
            for q in queries:
                distances[str(q["building_id"])][str(q["route_id"])] = float(q["distance"])
        result = [score_candidates(index,r,*matched[r["building_id"]],distances[r["building_id"]],max_distance_m) for r in rows]
        counts = {k:sum(r["confidence"]==k for r in result) for k in ("high","medium","low","conflict","unmatched","blocked")}
        token = hashlib.sha256(json.dumps(result,sort_keys=True,ensure_ascii=False,separators=(",",":")).encode()).hexdigest()
        return {"items":result,"counts":counts,"preview_token":token,"next_after_id":str(page[-1]["id"]) if len(data)>limit else None}

    def apply(self, *, preview_token: str, assignments: dict[UUID, UUID], **kwargs: Any) -> dict[str, Any]:
        self.repo.prepare("edit")
        self.session.execute(text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
                             {"key":f"house-numbering-assignment:{self.repo.project_id}"})
        plan = self.preview(**kwargs)
        if plan["preview_token"] != preview_token:
            raise PlatformConflictError("Dữ liệu đã thay đổi, hãy xem trước lại",code="STALE_ASSIGNMENT")
        by_id = {r["building_id"]:r for r in plan["items"]}
        for bid, route_id in sorted(assignments.items(),key=lambda x:str(x[0])):
            row = by_id.get(str(bid))
            if not row or row["confidence"] in {"blocked","unmatched"} or not any(c["route_id"]==str(route_id) for c in row["candidates"]):
                raise PlatformValidationError("Tuyến chưa được đề xuất cho công trình hoặc công trình bị khóa",code="INVALID_ASSIGNMENT")
            # Repo locks building row, checks revision and prevents official-number changes.
            current = self.repo.get_building(bid,for_update=True)
            if current["route_id"] is not None or current["revision"] != row["revision"]:
                raise PlatformConflictError("Công trình đã thay đổi tuyến",code="STALE_ASSIGNMENT")
            self.repo.update_building(bid,expected_revision=row["revision"],route_id=route_id,
                area_id=current["area_id"],parcel_ref=current["parcel_ref"],
                source_id=current["linked_source_id"],feature_id=current["linked_feature_id"])
        self.session.commit()
        return {"assigned":len(assignments),"next_after_id":plan["next_after_id"]}
