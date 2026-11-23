"""Project-wide, read-only triage of unlinked buildings; no nearest-route guessing."""
from __future__ import annotations

from collections import Counter, defaultdict
import json
from modules.house_numbering.cluster_correction import cluster_key
from typing import Any
from uuid import UUID

from sqlalchemy import text
from modules.house_numbering.building_assignment import RouteIndex, road_name, alley_name

GROUPS = {
    "ready": "Đủ điều kiện xem trước gán tự động",
    "parent_missing": "Tuyến cha thiếu hoặc sai",
    "prefix_invalid": "Tiền tố / phân cấp hẻm sai",
    "route_duplicate": "Tuyến trùng hoặc mơ hồ",
    "route_pending": "Tuyến nguồn chưa import",
    "code_unknown": "Mã tuyến không tồn tại",
    "address_conflict": "Mã tuyến và địa chỉ mâu thuẫn",
    "address_ambiguous": "Địa chỉ khớp nhiều tuyến",
    "address_missing": "Thiếu tên đường / thông tin hẻm",
    "route_missing": "Chưa có tuyến khớp địa chỉ",
    "distance": "Vị trí xa tuyến khớp địa chỉ",
    "non_address_route": "Tên tuyến chưa rõ / không phải đường địa chỉ",
    "invalid_geometry": "Hình học công trình không dùng được",
    "locked": "Có số nhà được bảo vệ",
}
ACTIONS = {
    "ready": "Xem trước và xác nhận gán theo mã/địa chỉ.",
    "parent_missing": "Phục hồi hoặc vẽ tuyến cha trong phần Import / phục hồi cây tuyến.",
    "prefix_invalid": "Rà lại tiền tố và cấp đường/hẻm trong phần phục hồi cây tuyến.",
    "route_duplicate": "Đối soát đoạn tuyến trùng; chỉ ghép khi hình học liên tục.",
    "route_pending": "Kiểm tra và import tuyến nguồn bị giữ lại.",
    "code_unknown": "Đối soát mã tuyến nguồn và chọn tuyến đúng.",
    "address_conflict": "Kiểm tra tên đường, hẻm và mã; xác minh trên bản đồ.",
    "address_ambiguous": "Chọn đúng tuyến theo địa chỉ và cửa vào.",
    "address_missing": "Bổ sung thuộc tính khảo sát; không suy diễn từ tuyến gần.",
    "route_missing": "Kiểm tra tên đường/hẻm hoặc bổ sung tuyến GIS.",
    "distance": "Xác minh cửa vào và hình học trước khi gán.",
    "non_address_route": "Xác minh tên chính thức và loại tuyến trước khi gán.",
    "invalid_geometry": "Sửa hình học/lớp dữ liệu nguồn.",
    "locked": "Kiểm tra hồ sơ số nhà và quyền chuyển tuyến.",
}

def classify_pending(reasons: set[str]) -> str:
    if reasons & {"ambiguous_sibling_prefix"}:
        return "route_duplicate"
    if reasons & {"parent_required","parent_not_found","parent_has_errors","parent_prefix_missing","street_cannot_have_parent_or_prefix"}:
        return "parent_missing"
    if reasons & {"invalid_prefix","prefix_must_be_one_level","prefix_does_not_follow_parent"}:
        return "prefix_invalid"
    return "route_pending"

def audit_unlinked(repo: Any, *, source_id: UUID | None, mapping: dict[str,str], group: str | None,
                   after_id: UUID | None, limit: int, max_distance_m: float, cluster: str | None = None) -> dict[str, Any]:
    repo.prepare("read")
    params={"org":repo.org_id,"project":repo.project_id,"source":source_id,
            "road":mapping.get("road",""),"alley":mapping.get("alley",""),"code":mapping.get("code","")}
    session=repo.session
    routes=[dict(row) for row in session.execute(text("""
        SELECT r.id,r.name,r.route_type,r.parent_route_id,r.prefix,r.source_label,r.status,r.revision,r.segments,r.reviewed_aliases,f.content_revision
        FROM house_numbering.routes r JOIN house_numbering.route_lines f
          ON (f.organization_id,f.project_id,f.route_id)=(r.organization_id,r.project_id,r.id)
        WHERE r.organization_id=:org AND r.project_id=:project ORDER BY r.id
    """),params).mappings()]
    idx=RouteIndex(routes)
    pending=defaultdict(list)
    for row in session.execute(text("""
        SELECT f.source_id,f.feature_id,f.attributes->>'route_code' code,
           f.attributes->>'review_reason' reason,f.attributes->>'parent_code' parent_code,
           f.attributes->>'ten' name
        FROM spatial.reference_features f JOIN spatial.data_sources s
          ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
        WHERE f.organization_id=:org AND f.project_id=:project AND s.lifecycle='active'
          AND s.validation_state='ready' AND s.geometry_family='LineString'
          AND nullif(trim(f.attributes->>'route_code'),'') IS NOT NULL
          AND nullif(trim(f.attributes->>'review_reason'),'') IS NOT NULL
        ORDER BY f.source_id,f.feature_id
    """),params).mappings():
        pending[str(row["code"]).strip()].append(dict(row))
    rows=session.execute(text("""
        SELECT b.id,b.linked_source_id,b.linked_feature_id,b.eligibility,
           f.attributes->>:road AS road,f.attributes->>:alley AS alley,f.attributes->>:code AS code,
           COALESCE(jsonb_exists(f.attributes,:alley),false) AND :alley <> '' AS alley_mapped,
           coalesce(f.geometry_state='present' AND NOT ST_IsEmpty(f.geometry) AND ST_SRID(f.geometry)>0
             AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon')
             AND s.lifecycle='active' AND s.validation_state='ready',false) AS geometry_usable,
           EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
             AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('submitted','approved','reserved','proposed')) AS locked
        FROM house_numbering.buildings b
        LEFT JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        LEFT JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
        WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active' AND b.route_id IS NULL
          AND (CAST(:source AS uuid) IS NULL OR b.linked_source_id=:source)
        ORDER BY b.id
    """),params).mappings().all()
    result=[]; pairs=[]
    for row in rows:
        bid=str(row["id"]); code=str(row["code"] or "").strip(); road=road_name(row["road"])
        matched,reason=idx.match(row["road"],row["alley"],code,row["alley_mapped"])
        legacy=pending.get(code,[]) if code and not idx.codes.get(code) else []
        route_id=None
        if row["locked"]: kind="locked"
        elif not row["geometry_usable"]: kind="invalid_geometry"
        elif legacy:
            kind=classify_pending({reason for x in legacy for reason in str(x["reason"]).split(";")})
        elif not road or not row["alley_mapped"]: kind="address_missing"
        elif road in {"chua ro ten","chua co ten","khong ten"} or "nuoc thai" in road: kind="non_address_route"
        elif reason=="unknown_code": kind="code_unknown"
        elif reason=="attribute_conflict": kind="address_conflict"
        elif len(matched)>1: kind="address_ambiguous"
        elif len(matched)==1 and reason in {"exact_code","exact_address"}:
            route_id=next(iter(matched)); kind="distance"
            pairs.append({"building_id":bid,"route_id":route_id})
        else: kind="route_missing"
        correction_candidate=None
        if kind=="address_conflict" and group=="address_conflict" and row["eligibility"]!="ineligible":
            possible,why=idx.match(row["road"],row["alley"],"",row["alley_mapped"])
            if len(possible)==1 and why=="exact_address":
                correction_candidate=next(iter(possible))
                pairs.append({"building_id":bid,"route_id":correction_candidate})
        code_paths=[{"route_id":r,"name":idx.routes[r]["name"],"root_name":idx.routes[idx.root[r]]["name"],"prefix":idx.routes[r]["prefix"]} for r in sorted(idx.codes.get(code,set()))[:5]]
        key=cluster_key(row["road"],row["alley"],code)
        result.append({"cluster":key,"code_paths":code_paths,"building_id":bid,"source_id":str(row["linked_source_id"]),"feature_id":str(row["linked_feature_id"]),
            "road":str(row["road"] or ""),"alley":str(row["alley"] or ""),"code":code,"group":kind,"reason":reason,
            "route_id":route_id,"distance_m":None,"correction_candidate":correction_candidate,"correction_ready":False,"pending":[{"source_id":str(x["source_id"]),"feature_id":x["feature_id"],
                "reason":x["reason"],"parent_code":x["parent_code"],"name":x["name"]} for x in legacy[:5]]})
    distances={}
    if pairs:
        for row in session.execute(text("""
            WITH pairs AS MATERIALIZED (
              SELECT * FROM jsonb_to_recordset(CAST(:pairs AS jsonb)) AS p(building_id uuid, route_id uuid)
            )
            SELECT p.building_id,ST_Distance(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,
                       ST_Transform(r.geometry,4326)::geography) AS distance
            FROM pairs p JOIN house_numbering.buildings b ON b.id=p.building_id
             AND b.organization_id=:org AND b.project_id=:project AND b.route_id IS NULL
            JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
            JOIN house_numbering.route_lines r ON r.route_id=p.route_id AND r.organization_id=:org AND r.project_id=:project
        """),{**params,"pairs":json.dumps(pairs)}).mappings():
            distances[str(row["building_id"])]=float(row["distance"]) if row["distance"] is not None else None
    for item in result:
        distance=distances.get(item["building_id"])
        if distance is not None:
            item["distance_m"]=round(distance,2)
            if item["correction_candidate"]:
                item["correction_ready"]=distance<=35
            elif distance<=min(35,max_distance_m): item["group"]="ready"
    counts=Counter(row["group"] for row in result)
    groups=[{"key":key,"label":label,"count":counts[key],"action":ACTIONS[key]} for key,label in GROUPS.items()]
    scoped=[row for row in result if group and row["group"]==group]
    clusters={}
    for row in scoped:
        k=row["cluster"]
        if k not in clusters:
            clusters[k]={"key":k,"road":row["road"],"alley":row["alley"],"code":row["code"],"count":0,"correction_ready":0}
        clusters[k]["count"]+=1
        clusters[k]["correction_ready"]+=int(row["correction_ready"])
    top=sorted(clusters.values(),key=lambda x:(-x["correction_ready"],-x["count"],x["key"]))[:30]
    filtered=[row for row in scoped if (not cluster or row["cluster"]==cluster) and (after_id is None or row["building_id"]>str(after_id))]
    page=filtered[:limit]
    return {"total":len(rows),"groups":groups,"clusters":top,"active_cluster":clusters.get(cluster) if cluster else None,"cluster_count":len(clusters),"correction_ready":sum(x["correction_ready"] for x in clusters.values()),"items":page,"next_after_id":page[-1]["building_id"] if len(filtered)>limit else None}
