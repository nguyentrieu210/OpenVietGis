"""Persist verified geometry candidates separately from canonical routes.

This never creates or edits an address route, building or survey attribute.
Normal route-import remains the only promotion path, with all existing guards.
"""
from __future__ import annotations

import hashlib
import json
from typing import Any
from uuid import uuid4

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformValidationError,
)
from sqlalchemy import text


def stage_eligible(row: dict[str,Any]) -> bool:
    errors=set(row.get("errors") or [])
    evidence=row.get("survey_evidence") or {}
    parent=row.get("parent_evidence") or {}
    connected=[x for x in row.get("parent_suggestions",[]) if x.get("address_match") and x.get("connected")]
    return bool(row.get("state")=="error" and not row.get("existing_id")
        and row.get("route_type")=="alley" and row.get("resolved_prefix") and row.get("parent_id")
        and bool(errors.intersection({"review_survey_address_conflict","review_stage_stale"}))
        and errors <= {"review_survey_address_conflict","review_required","review_address_pending","review_stage_stale"}
        and (evidence.get("address_conflicts",0)>0 or "review_stage_stale" in errors)
        and evidence.get("referenced")==evidence.get("unassigned")
        and parent.get("address_match") and parent.get("connected") and parent.get("state")=="existing"
        and str(parent.get("code"))==str(row["parent_id"])
        and not row.get("parent_search_truncated") and len(connected)==1)

def stage_geometry(repo:Any, *, layers:list[dict], overrides:dict, preview_token:str, selected_keys:list[str],
                   reason:str, merge_groups:list[str]|None=None, parent_features:list[dict]|None=None)->dict:
    from modules.house_numbering.route_import import RouteTreeImporter
    reason=reason.strip()
    if len(reason)<10 or len(reason)>500 or not selected_keys or len(selected_keys)>100 or len(set(selected_keys))!=len(selected_keys):
        raise PlatformValidationError("Chọn tối đa 100 tuyến và ghi lý do lưu hình học",code="INVALID_GEOMETRY_REVIEW")
    repo.prepare("edit")
    session=repo.session
    params={"org":repo.org_id,"project":repo.project_id,"actor":repo.actor_id}
    session.execute(text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),{"key":f"house-numbering-route-import:{repo.project_id}"})
    plan=RouteTreeImporter(repo).preview(layers,overrides,merge_groups,parent_features)
    replay=plan["preview_token"]!=preview_token
    by_key={r["key"]:r for r in plan["items"]}
    if any(k not in by_key or not stage_eligible(by_key[k]) for k in selected_keys):
        if replay:
            raise PlatformConflictError("Dữ liệu hình học hoặc khảo sát đã đổi; xem trước lại",code="STALE_GEOMETRY_REVIEW")
        raise PlatformValidationError("Chỉ lưu hình học đã nối đúng cha, chưa có tuyến và chỉ còn lỗi khảo sát",code="INVALID_GEOMETRY_REVIEW")
    items=[]
    for key in sorted(selected_keys):
        row=by_key[key]
        # Capture only route topology and aggregate survey review, not occupants or numbers.
        fields=("key","code","name","route_type","parent_id","resolved_prefix","direction_state","source_id","feature_id",
                "content_revision","segments","segment_revisions","parent_evidence","survey_evidence","survey_review")
        snapshot={k:row[k] for k in fields if k in row}
        raw=json.dumps(snapshot,sort_keys=True,ensure_ascii=False,separators=(",",":"),default=str)
        token=hashlib.sha256(raw.encode()).hexdigest()
        # Staging changes the next preview (address-review status is now present).
        # Permit only an exact, non-mutating replay of the original accepted request.
        if replay:
            saved=session.execute(text("""
                SELECT id,revision FROM house_numbering.geometry_reviews
                WHERE organization_id=:org AND project_id=:project AND route_code=:code
                  AND snapshot_token=:token AND reason=:reason
                  AND snapshot->>'stage_request_token'=:request
            """),{**params,"code":row["code"],"token":token,"reason":reason,"request":preview_token}).mappings().first()
            if not saved:
                raise PlatformConflictError("Dữ liệu hình học hoặc khảo sát đã đổi; xem trước lại",code="STALE_GEOMETRY_REVIEW")
            items.append({"id":str(saved["id"]),"revision":saved["revision"],"name":row["name"]})
            continue
        snapshot["stage_request_token"]=preview_token
        raw=json.dumps(snapshot,sort_keys=True,ensure_ascii=False,separators=(",",":"),default=str)
        saved=session.execute(text("""
            INSERT INTO house_numbering.geometry_reviews(id,organization_id,project_id,route_code,snapshot,snapshot_token,
                  reason,created_by_actor_id,updated_by_actor_id)
            VALUES(:id,:org,:project,:code,CAST(:snapshot AS jsonb),:token,:reason,:actor,:actor)
            ON CONFLICT(organization_id,project_id,route_code) DO UPDATE SET
                 snapshot=EXCLUDED.snapshot,snapshot_token=EXCLUDED.snapshot_token,reason=EXCLUDED.reason,
                 revision=geometry_reviews.revision+1,updated_at=now(),updated_by_actor_id=EXCLUDED.updated_by_actor_id
            WHERE geometry_reviews.snapshot_token<>EXCLUDED.snapshot_token OR geometry_reviews.reason<>EXCLUDED.reason
            RETURNING id,revision
        """),{**params,"id":uuid4(),"code":row["code"],"snapshot":raw,"token":token,"reason":reason}).mappings().first()
        if saved:
            repo.audit("house_numbering.route.stage_geometry","house_numbering_geometry_review",saved["id"],
                    {"route_code":row["code"],"prefix":row["resolved_prefix"],"parent_id":row["parent_id"],
                     "snapshot_token":token,"survey_evidence":row["survey_evidence"],"reason":reason})
        else:
            saved=session.execute(text("SELECT id,revision FROM house_numbering.geometry_reviews WHERE organization_id=:org AND project_id=:project AND route_code=:code"),
                {**params,"code":row["code"]}).mappings().one()
        items.append({"id":str(saved["id"]),"revision":saved["revision"],"name":row["name"]})
    session.commit()
    return {"staged":len(items),"items":items,"routes_created":0,"buildings_assigned":0}

def list_geometry_reviews(repo:Any, offset:int=0, limit:int=100)->dict:
    repo.prepare("read")
    # Promotion status is computed only if the exact original route links and topology were imported.
    result=repo.session.execute(text("""
        SELECT g.id,g.route_code,g.snapshot,g.reason,g.revision,g.created_at,g.updated_at,
               (SELECT r.id FROM house_numbering.routes r
                WHERE (r.organization_id,r.project_id)=(g.organization_id,g.project_id)
                  AND r.source_label='route-tree-import:'||g.route_code
                  AND r.name=g.snapshot->>'name' AND r.route_type=g.snapshot->>'route_type'
                  AND r.parent_route_id::text=g.snapshot->>'parent_id'
                  AND r.prefix=g.snapshot->>'resolved_prefix'
                  AND r.linked_source_id::text=g.snapshot->>'source_id'
                  AND r.linked_feature_id=g.snapshot->>'feature_id'
                  AND r.direction_state=g.snapshot->>'direction_state'
                  AND COALESCE(r.segments,'[]'::jsonb)=COALESCE(g.snapshot->'segments','[]'::jsonb)
                ORDER BY r.id LIMIT 1) promoted_route_id
        FROM house_numbering.geometry_reviews g
        WHERE g.organization_id=:org AND g.project_id=:project
        ORDER BY g.updated_at DESC,g.id LIMIT :limit OFFSET :offset
    """),{"org":repo.org_id,"project":repo.project_id,"offset":offset,"limit":limit+1}).mappings().all()
    return {"items":[{**dict(r),"status":"promoted" if r["promoted_route_id"] else "pending"} for r in result[:limit]],
            "next_offset":offset+limit if len(result)>limit else None}
