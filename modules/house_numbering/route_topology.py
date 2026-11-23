"""Explicit recovery suggestions; no geometry is synthesized or modified."""
from __future__ import annotations

import hashlib
import json
import re
from collections import defaultdict
from typing import Any

from sqlalchemy import text
from openvietgis_api.domains.platform_errors import PlatformValidationError
from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.building_assignment import RouteIndex
from modules.house_numbering.route_recovery import address_label, normalized_road


def stable_id(*parts: str) -> str:
    return hashlib.sha256(json.dumps(parts, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()[:32]


def linked_features(existing: list[dict[str, Any]]) -> set[tuple[str,str]]:
    return {(str(ref["source_id"]),str(ref["feature_id"])) for r in existing
            for ref in (r.get("segments") or [{"source_id":r["linked_source_id"],"feature_id":r["linked_feature_id"]}])}


def existing_paths(existing: list[dict[str, Any]]) -> dict[tuple[str,str], list[dict[str, Any]]]:
    index = RouteIndex(existing)
    paths: dict[tuple[str,str],list[dict[str,Any]]] = defaultdict(list)
    for key, root in index.root_name.items():
        route = index.routes[key]
        paths[(normalized_road(root),route["prefix"] or "")].append(route)
    return paths


def route_path(row: dict[str, Any]) -> tuple[str,str] | None:
    label = address_label(row["name"])
    if label and label[0] not in {"chua ro ten","chua co ten","khong ten"}:
        return label
    if row["route_type"] == "street" and not row["prefix"] and not label:
        road = normalized_road(row["name"])
        if road and road not in {"chua ro ten","chua co ten","khong ten"}:
            return road,""
    return None


def merge_options(session: Any, params: dict[str, Any], rows: list[dict[str, Any]], existing: list[dict[str, Any]]) -> list[dict[str,Any]]:
    """All unlinked review segments sharing one unambiguous address path."""
    occupied = linked_features(existing)
    paths = existing_paths(existing)
    by_path: dict[tuple[str,str],list[dict[str,Any]]] = defaultdict(list)
    for row in rows:
        if not row.get("review_reason") or row.get("logical_code") or row.get("manual_parent"):
            continue
        if (row["source_id"],row["feature_id"]) in occupied:
            continue
        path = route_path(row)
        if path and path[1]:
            by_path[path].append(row)
    suggestions = []
    for (root,prefix), members in sorted(by_path.items()):
        if len(members)<2:
            continue
        members = sorted(members,key=lambda r:r["key"])
        item = {"id":"recovery-"+stable_id(*(r["key"] for r in members)), "root":root,
                "prefix":prefix,"name":members[0]["name"],
                "members":[{k:r[k] for k in ("key","code","source_id","feature_id","name","parent_code","prefix","content_revision")} for r in members],
                "status":"geometry_pending"}
        if paths.get((root,prefix)):
            item["status"]="existing_conflict"
        elif len(members)>200:
            item["status"]="too_many_segments"
        else:
            try:
                if any(canonical_prefix(r["prefix"]) != prefix for r in members):
                    item["status"]="attribute_conflict"
            except ValueError:
                item["status"]="attribute_conflict"
            if len({r["direction_state"] for r in members})!=1 or any(r["errors"] for r in members):
                item["status"]="attribute_conflict"
            if len({r["code"] for r in members})!=len(members):
                item["status"]="attribute_conflict"
        suggestions.append(item)
    specs=[{"id":s["id"],"links":s["members"]} for s in suggestions if s["status"]=="geometry_pending"]
    if specs:
        valid=set(session.execute(text("""
            WITH specs AS (SELECT v->>'id' id,v->'links' links FROM jsonb_array_elements(CAST(:specs AS jsonb)) v),
            merged AS (
              SELECT s.id,house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) g
              FROM specs s CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY l(v,ord)
              LEFT JOIN spatial.reference_features f ON f.organization_id=:org AND f.project_id=:project
                 AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
              LEFT JOIN spatial.data_sources ds ON ds.organization_id=f.organization_id AND ds.project_id=f.project_id AND ds.id=f.source_id
              GROUP BY s.id HAVING bool_and(COALESCE(f.geometry_state='present' AND ds.lifecycle='active' AND ds.validation_state='ready',false))
            ) SELECT id FROM merged WHERE g IS NOT NULL
        """),{**params,"specs":json.dumps(specs)}).scalars())
        for item in suggestions:
            if item["status"]=="geometry_pending":
                item["status"]="connected" if item["id"] in valid else "disconnected"
    return suggestions


def apply_merges(rows: list[dict[str, Any]], existing: list[dict[str, Any]],
                 options: list[dict[str,Any]], selected: list[str]) -> None:
    by_key = {r["key"]:r for r in rows}
    by_id = {s["id"]:s for s in options}
    if len(set(selected))!=len(selected):
        raise PlatformValidationError("Nhóm ghép bị trùng",code="DUPLICATE_MERGE_GROUP")
    saved_ids = set()
    # Restore previously saved recovery groups automatically, preserving every segment alias.
    for old in existing:
        code = (old.get("source_label") or "").removeprefix("route-tree-import:")
        if not code.startswith("recovery-") or not old.get("segments"):
            continue
        keys = [f'{s["source_id"]}:{s["feature_id"]}' for s in old["segments"]]
        if not all(k in by_key for k in keys):
            continue
        if any(by_key[k]["code"] != s.get("code") for k,s in zip(keys,old["segments"],strict=True)):
            continue
        if any(r["key"] not in keys and (r.get("logical_code")==code or r["code"]==code) for r in rows):
            raise PlatformValidationError("Mã nhóm ghép đã được dùng cho đoạn khác",code="MERGE_CODE_CONFLICT")
        saved_ids.add(code)
        for k in keys:
            row = by_key[k]
            row.update(logical_code=code,recovery_group=True,name=old["name"],route_type=old["route_type"],
                       prefix=old["prefix"] or "",parent_code=str(old["parent_route_id"] or ""),
                       direction_state=old["direction_state"])
    for key in selected:
        if key in saved_ids:
            continue
        item = by_id.get(key)
        if not item or item["status"]!="connected":
            raise PlatformValidationError("Chỉ ghép nhóm cùng địa chỉ và có hình học liên tục",code="INVALID_MERGE_GROUP")
        members = [by_key[m["key"]] for m in item["members"]]
        member_keys={r["key"] for r in members}
        if any(r["key"] not in member_keys and (r.get("logical_code")==key or r["code"]==key) for r in rows):
            raise PlatformValidationError("Mã nhóm ghép đã được dùng cho đoạn khác",code="MERGE_CODE_CONFLICT")
        old_parents = sorted({m["parent_code"] for m in members if m["parent_code"]})
        # Parent disagreement is not silently guessed: require a new reviewed selection.
        parent = old_parents[0] if len(old_parents)==1 else ""
        for row in members:
            row.update(logical_code=key,recovery_group=True,name=item["name"],route_type="alley",
                       prefix=item["prefix"],parent_code=parent,
                       legacy_parent_codes=old_parents)


def parent_tasks(rows: list[dict[str,Any]], existing: list[dict[str,Any]]) -> list[dict[str,Any]]:
    """Missing ancestor address paths across the loaded layers, not assumed missing geometry."""
    known = set(existing_paths(existing))
    occupied = linked_features(existing)
    labeled = [(row,route_path(row)) for row in rows if (row["source_id"],row["feature_id"]) not in occupied]
    known.update(path for _,path in labeled if path)
    tasks: dict[tuple[str,str],dict[str,Any]] = {}
    for row,path in labeled:
        if not path or not path[1] or row.get("existing_id") or not row.get("review_reason"):
            continue
        root,full=path
        pieces=full.split("/")
        raw_root=re.split(r"\s+(?:hẻm|hem|ngõ|ngo|ngách|ngach|nhánh|nhanh|alley)\s+",row["name"],maxsplit=1,flags=re.IGNORECASE)[0].strip()
        for i in range(len(pieces)):
            prefix="/".join(pieces[:i])
            if (root,prefix) in known:
                continue
            item=tasks.setdefault((root,prefix),{"id":stable_id(root,prefix),"root":root,"prefix":prefix,
                "name":raw_root+(f" hẻm {prefix}" if prefix else ""),"parent_prefix":prefix.rpartition("/")[0],
                "children":0,"buildings":0,"examples":[]})
            item["children"]+=1
            item["buildings"]+=row.get("survey_evidence",{}).get("referenced",0)
            if len(item["examples"])<5:
                item["examples"].append({"source_id":row["source_id"],"feature_id":row["feature_id"],"name":row["name"]})
    return sorted(tasks.values(),key=lambda v:(v["root"],v["prefix"].count("/") if v["prefix"] else -1,v["prefix"]))


def append_parent_features(session: Any, params: dict[str,Any], rows: list[dict[str,Any]],
                           existing: list[dict[str,Any]], selected: list[dict[str,Any]]) -> None:
    tasks = {t["id"]:t for t in parent_tasks(rows,existing)}
    by_saved = {r.get("source_label"):r for r in existing}
    used = {(r["source_id"],r["feature_id"]) for r in rows}
    linked = linked_features(existing)
    by_id = {str(r["id"]):r for r in existing}
    paths = existing_paths(existing)
    # Stable order gives deterministic anchors and preview tokens.
    for item in sorted(selected,key=lambda x:x["task_id"]):
        tid,source,fid=item["task_id"],str(item["source_id"]),item["feature_id"]
        key=f"{source}:{fid}"
        code=f"recovery-parent-{tid}"
        saved=by_saved.get(f"route-tree-import:{code}")
        task=tasks.get(tid)
        if (source,fid) in used:
            raise PlatformValidationError("Đối tượng đã nằm trong lớp import; hãy dùng dòng tuyến có sẵn",code="PARENT_FEATURE_ALREADY_SELECTED")
        if (source,fid) in linked and (not saved or str(saved["linked_source_id"])!=source or saved["linked_feature_id"]!=fid or saved.get("segments")):
            raise PlatformValidationError("Đối tượng đã liên kết tuyến khác",code="PARENT_FEATURE_ALREADY_LINKED")
        if not task and not saved:
            raise PlatformValidationError("Tuyến cha không còn nằm trong danh sách thiếu; xem trước lại",code="PARENT_TASK_STALE")
        if saved and (str(saved["linked_source_id"])!=source or saved["linked_feature_id"]!=fid):
            raise PlatformValidationError("Tuyến cha đã được liên kết bằng đối tượng khác",code="PARENT_TASK_STALE")
        feature=session.execute(text("""
            SELECT f.content_revision,f.geometry_state,ST_GeometryType(f.geometry) geometry_type,ST_SRID(f.geometry) srid
            FROM spatial.reference_features f JOIN spatial.data_sources ds
              ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
            WHERE f.organization_id=:org AND f.project_id=:project AND f.source_id=:source AND f.feature_id=:fid
              AND ds.lifecycle='active' AND ds.validation_state='ready'
        """),{**params,"source":source,"fid":fid}).mappings().one_or_none()
        if not feature or feature["geometry_state"]!="present" or feature["geometry_type"] not in {"ST_LineString","ST_MultiLineString"} or not feature["srid"]:
            raise PlatformValidationError("Tuyến cha phải dùng đối tượng đường GIS hợp lệ trong dự án",code="INVALID_PARENT_FEATURE")
        if saved:
            name,prefix,kind,parent=saved["name"],saved["prefix"] or "",saved["route_type"],str(saved["parent_route_id"] or "")
        else:
            name,prefix=task["name"],task["prefix"]
            kind="alley" if prefix else "street"
            parent=""
            if prefix:
                refs=paths.get((task["root"],task["parent_prefix"]),[])
                if len(refs)==1:
                    parent=str(refs[0]["id"])
        rows.append({"key":key,"source_id":source,"feature_id":fid,"content_revision":feature["content_revision"],
                     "geometry_type":feature["geometry_type"],"code":code,"logical_code":"","name":name,
                     "parent_code":parent,"prefix":prefix,"route_type":kind,"direction_state":saved["direction_state"] if saved else "forward",
                     "errors":[],"review_reason":"manual_parent_feature","manual_parent":True,"parent_task_id":tid})
        used.add((source,fid))
    if len({x["task_id"] for x in selected})!=len(selected):
        raise PlatformValidationError("Một tuyến cha chỉ được chọn một đối tượng",code="DUPLICATE_PARENT_TASK")
    # When several missing ancestor levels were selected together, propose the
    # unique immediate parent from the same draft or existing tree.
    candidates: dict[tuple[str,str],list[str]]=defaultdict(list)
    for r in rows:
        path=route_path(r)
        if path and (r["source_id"],r["feature_id"]) not in linked:
            candidates[path].append(r["logical_code"] or r["code"])
    for row in rows:
        if not row.get("manual_parent") or not row["prefix"] or row["parent_code"]:
            continue
        label=address_label(row["name"])
        if label:
            refs=set(candidates[(label[0],label[1].rpartition("/")[0])])
            if len(refs)==1:
                row["parent_code"]=next(iter(refs))

