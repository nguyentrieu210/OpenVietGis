"""Explicit address evidence for staged route promotion, separate from raw GIS."""

from __future__ import annotations

import hashlib
import json
from typing import Any
from uuid import UUID, uuid4

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from sqlalchemy import text

from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.building_assignment import alley_name, road_name
from modules.house_numbering.route_recovery import address_label


def digest(v: Any) -> str:
    return hashlib.sha256(
        json.dumps(
            v, sort_keys=True, ensure_ascii=False, separators=(",", ":"), default=str
        ).encode()
    ).hexdigest()


def identity(row: dict) -> str:
    fields = (
        "code",
        "name",
        "route_type",
        "parent_id",
        "resolved_prefix",
        "direction_state",
        "source_id",
        "feature_id",
        "content_revision",
        "segments",
        "segment_revisions",
    )
    parent = row.get("parent_evidence") or {}
    return digest(
        [
            {k: row.get(k) for k in fields},
            {k: parent.get(k) for k in ("code", "revision", "geometry_revision")},
        ]
    )


def refs(snapshot: dict) -> list[dict]:
    segments = snapshot.get("segments") or [
        {
            "source_id": snapshot["source_id"],
            "feature_id": snapshot["feature_id"],
            "code": snapshot["code"],
        }
    ]
    revisions = snapshot.get("segment_revisions") or [snapshot["content_revision"]]
    return [{**s, "revision": rev} for s, rev in zip(segments, revisions, strict=True)]


def review_kind(road: str, branch: str, root: str, full: str) -> str:
    if road and road != root:
        return "different_road"
    if branch and branch != full:
        return (
            "descendant"
            if road == root and branch.startswith(full + "/")
            else "different_branch"
        )
    if not road:
        return "missing_road"
    if not branch:
        return "missing_prefix"
    return "exact"


def _stage(repo: Any, review_id: UUID, lock: bool = False) -> dict:
    record = (
        repo.session.execute(
            text(
                """
      SELECT id,route_code,snapshot,revision FROM house_numbering.geometry_reviews
      WHERE organization_id=:org AND project_id=:project AND id=:id
    """
                + (" FOR UPDATE" if lock else "")
            ),
            {"org": repo.org_id, "project": repo.project_id, "id": review_id},
        )
        .mappings()
        .first()
    )
    if not record:
        raise PlatformNotFoundError("Không tìm thấy tuyến chờ xác minh")
    return dict(record)


def _report(repo: Any, stage: dict) -> dict:
    s = stage["snapshot"]
    expected = address_label(s["name"])
    if not expected:
        raise PlatformValidationError(
            "Thiếu đường dẫn tuyến chờ", code="INVALID_GEOMETRY_REVIEW"
        )
    root, full = expected[0], expected[1].lower()
    params = {
        "org": repo.org_id,
        "project": repo.project_id,
        "review": stage["id"],
        "refs": json.dumps(refs(s)),
        "codes": sorted(
            {s["code"]} | {x["code"] for x in s.get("segments", []) if x.get("code")}
        ),
        "parent": s["parent_id"],
        "parent_revision": s["parent_evidence"]["revision"],
        "parent_geometry": s["parent_evidence"]["geometry_revision"],
    }
    geom_cte = """
      WITH links AS (SELECT * FROM jsonb_to_recordset(CAST(:refs AS jsonb)) AS x(source_id uuid,feature_id text,revision text)),
      line AS (
       SELECT house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY x.source_id,x.feature_id)) AS g
       FROM links x JOIN spatial.reference_features f ON f.organization_id=:org AND f.project_id=:project
          AND f.source_id=x.source_id AND f.feature_id=x.feature_id AND f.content_revision=x.revision
       JOIN spatial.data_sources ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
       WHERE f.geometry_state='present' AND ds.lifecycle='active' AND ds.validation_state='ready'
       HAVING count(*)=(SELECT count(*) FROM links)
      ), valid AS (
       SELECT ST_Transform(line.g,4326)::geography g FROM line JOIN house_numbering.routes r ON r.organization_id=:org AND r.project_id=:project AND r.id=CAST(:parent AS uuid)
       JOIN house_numbering.route_lines l ON (l.organization_id,l.project_id,l.route_id)=(r.organization_id,r.project_id,r.id)
       WHERE line.g IS NOT NULL AND r.revision=:parent_revision AND l.content_revision=:parent_geometry
         AND LEAST(ST_Distance(ST_Transform(ST_StartPoint(line.g),4326)::geography,ST_Transform(l.geometry,4326)::geography),
             ST_Distance(ST_Transform(ST_EndPoint(line.g),4326)::geography,ST_Transform(l.geometry,4326)::geography))<=8
      )
    """
    current = repo.session.execute(
        text(geom_cte + " SELECT EXISTS(SELECT 1 FROM valid)"), params
    ).scalar_one()
    if not current:
        return {
            "geometry_current": False,
            "items": [],
            "counts": {"total": 0, "verified": 0, "pending": 0, "blocked": 0},
            "overflow": False,
        }
    rows = (
        repo.session.execute(
            text(
                geom_cte
                + """
      SELECT b.id,b.revision,b.route_id,b.eligibility,b.linked_source_id source_id,b.linked_feature_id feature_id,
        f.content_revision,f.attributes->>'ten_duong' road,f.attributes->>'hem' alley,f.attributes->>'tuyen_id' code,
        CASE WHEN f.geometry_state='present' AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
          AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND ds.lifecycle='active' AND ds.validation_state='ready'
          THEN ST_Distance(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,valid.g) END distance,
        EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
           AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('proposed','submitted','approved','reserved')) locked,
        a.evidence_token,a.decision,a.verified_road,a.verified_prefix,a.evidence,a.revision decision_revision
      FROM house_numbering.buildings b JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
      JOIN spatial.data_sources ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
      CROSS JOIN valid
      LEFT JOIN house_numbering.geometry_address_reviews a ON (a.organization_id,a.project_id,a.building_id,a.geometry_review_id)=(b.organization_id,b.project_id,b.id,CAST(:review AS uuid))
      WHERE b.organization_id=:org AND b.project_id=:project AND b.state='active' AND f.attributes->>'tuyen_id'=ANY(:codes)
      ORDER BY b.id LIMIT 10001
    """
            ),
            params,
        )
        .mappings()
        .all()
    )
    if len(rows) > 10000:
        return {
            "geometry_current": True,
            "items": [],
            "counts": {
                "total": len(rows),
                "verified": 0,
                "pending": len(rows),
                "blocked": len(rows),
            },
            "overflow": True,
        }
    items = []
    verified = blocked = 0
    for x in rows:
        kind = review_kind(road_name(x["road"]), alley_name(x["alley"]), root, full)
        if kind == "exact":
            continue
        distance = float(x["distance"]) if x["distance"] is not None else None
        eligible = (
            x["route_id"] is None
            and not x["locked"]
            and distance is not None
            and x["eligibility"] != "ineligible"
        )
        token = digest(
            [
                identity(s),
                str(x["id"]),
                x["revision"],
                str(x["source_id"]),
                x["feature_id"],
                x["content_revision"],
                x["road"],
                x["alley"],
                x["code"],
                distance,
                eligible,
            ]
        )
        decided = (
            eligible
            and x["evidence_token"] == token
            and x["decision"] in {"belongs", "other_address"}
        )
        verified += int(decided)
        blocked += int(not eligible)
        item = {
            "building_id": str(x["id"]),
            "source_id": str(x["source_id"]),
            "feature_id": x["feature_id"],
            "road": x["road"] or "",
            "alley": x["alley"] or "",
            "code": x["code"] or "",
            "kind": kind,
            "distance_m": round(distance, 3) if distance is not None else None,
            "eligible": eligible,
            "can_belong": eligible and distance <= 35,
            "evidence_token": token,
            "revision": x["decision_revision"] or 0,
            "status": "verified"
            if decided
            else "blocked"
            if not eligible
            else "pending",
        }
        if decided:
            item.update(
                decision=x["decision"],
                verified_road=x["verified_road"],
                verified_prefix=x["verified_prefix"],
                evidence=x["evidence"],
            )
        items.append(item)
    return {
        "geometry_current": True,
        "items": items,
        "counts": {
            "total": len(items),
            "verified": verified,
            "pending": len(items) - verified,
            "blocked": blocked,
        },
        "overflow": False,
    }


def address_review_list(
    repo: Any, review_id: UUID, offset: int = 0, limit: int = 100
) -> dict:
    repo.prepare("read")
    stage = _stage(repo, review_id)
    from modules.house_numbering.address_suggestions import add_suggestions

    report = _report(repo, stage)
    items = report.pop("items")
    page = items[offset : offset + limit]
    counts = add_suggestions(repo, stage, page)
    from modules.house_numbering.address_batch import group_list

    groups = group_list(repo, stage, page)
    return {
        **report,
        "name": stage["snapshot"]["name"],
        "expected_prefix": stage["snapshot"]["resolved_prefix"],
        "items": page,
        "suggestion_counts": counts,
        "batch_groups": groups,
        "next_offset": offset + limit if len(items) > offset + limit else None,
    }


def _lock_review(repo: Any, review_id: UUID) -> dict:
    repo.prepare("edit")
    session = repo.session
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": f"house-numbering-route-import:{repo.project_id}"},
    )
    stage = _stage(repo, review_id, True)
    if session.execute(
        text(
            "SELECT EXISTS(SELECT 1 FROM house_numbering.routes WHERE organization_id=:org AND project_id=:project AND source_label=:label)"
        ),
        {
            "org": repo.org_id,
            "project": repo.project_id,
            "label": "route-tree-import:" + stage["route_code"],
        },
    ).scalar_one():
        raise PlatformConflictError(
            "Tuyến đã vào danh mục; hồ sơ xác minh chỉ đọc",
            code="PROMOTED_ADDRESS_REVIEW",
        )
    return stage


def _validate_review(
    report: dict,
    row: dict | None,
    stage: dict,
    *,
    evidence_token: str,
    expected_revision: int,
    decision: str,
    verified_road: str,
    verified_prefix: str,
    evidence: str,
) -> tuple[str, str, str]:
    if (
        not row
        or not report["geometry_current"]
        or row["evidence_token"] != evidence_token
        or row["revision"] != expected_revision
    ):
        raise PlatformConflictError(
            "Hồ sơ hoặc tuyến đã thay đổi, hãy tải lại", code="STALE_ADDRESS_REVIEW"
        )
    if not row["eligible"]:
        raise PlatformValidationError(
            "Nhà đã liên kết, có số bảo vệ hoặc hình học không hợp lệ",
            code="INVALID_ADDRESS_REVIEW",
        )
    road = verified_road.strip()
    branch = verified_prefix.strip()
    evidence = evidence.strip()
    if (
        road_name(road)
        in {
            "",
            "0",
            "none",
            "null",
            "unknown",
            "khong ro",
            "chua ro",
            "chua ro ten",
            "chua xac dinh",
        }
        or len(road) > 200
        or len(evidence) < 10
        or len(evidence) > 500
    ):
        raise PlatformValidationError(
            "Cần địa chỉ xác minh và căn cứ cụ thể", code="INVALID_ADDRESS_REVIEW"
        )
    if branch:
        try:
            branch = canonical_prefix(branch)
        except ValueError:
            raise PlatformValidationError(
                "Tiền tố hẻm không hợp lệ", code="INVALID_ADDRESS_REVIEW"
            )
    label = address_label(stage["snapshot"]["name"])
    same = road_name(road) == label[0] and alley_name(branch) == label[1].lower()
    if (
        decision not in {"belongs", "other_address"}
        or (decision == "belongs" and (not same or not row["can_belong"]))
        or (decision == "other_address" and same)
    ):
        raise PlatformValidationError(
            "Quyết định không khớp địa chỉ hoặc khoảng cách",
            code="INVALID_ADDRESS_REVIEW",
        )
    return road, branch, evidence


def _write_review(
    repo: Any,
    review_id: UUID,
    building_id: UUID,
    *,
    evidence_token: str,
    decision: str,
    road: str,
    branch: str,
    evidence: str,
    batch_id: UUID | None = None,
) -> dict:
    session = repo.session
    saved = (
        session.execute(
            text("""
      INSERT INTO house_numbering.geometry_address_reviews(id,organization_id,project_id,geometry_review_id,building_id,evidence_token,
         decision,verified_road,verified_prefix,evidence,created_by_actor_id,updated_by_actor_id)
      VALUES(:id,:org,:project,:review,:bid,:token,:decision,:road,:prefix,:evidence,:actor,:actor)
      ON CONFLICT(organization_id,project_id,geometry_review_id,building_id) DO UPDATE SET
         evidence_token=EXCLUDED.evidence_token,decision=EXCLUDED.decision,verified_road=EXCLUDED.verified_road,
         verified_prefix=EXCLUDED.verified_prefix,evidence=EXCLUDED.evidence,revision=geometry_address_reviews.revision+1,
         updated_at=now(),updated_by_actor_id=EXCLUDED.updated_by_actor_id RETURNING id,revision
    """),
            {
                "id": uuid4(),
                "org": repo.org_id,
                "project": repo.project_id,
                "review": review_id,
                "bid": building_id,
                "token": evidence_token,
                "decision": decision,
                "road": road,
                "prefix": branch,
                "evidence": evidence,
                "actor": repo.actor_id,
            },
        )
        .mappings()
        .one()
    )
    repo.audit(
        "house_numbering.building.verify_address",
        "house_numbering_geometry_address_review",
        saved["id"],
        {
            "geometry_review_id": str(review_id),
            "building_id": str(building_id),
            "decision": decision,
            "verified_road": road,
            "verified_prefix": branch,
            "evidence_token": evidence_token,
            "evidence": evidence,
            **({"batch_id": str(batch_id)} if batch_id else {}),
        },
    )
    return {
        "id": str(saved["id"]),
        "revision": saved["revision"],
        "decision": decision,
        "building_updated": False,
    }


def save_address_review(
    repo: Any,
    review_id: UUID,
    building_id: UUID,
    *,
    evidence_token: str,
    expected_revision: int,
    decision: str,
    verified_road: str,
    verified_prefix: str,
    evidence: str,
) -> dict:
    stage = _lock_review(repo, review_id)
    report = _report(repo, stage)
    row = next(
        (r for r in report["items"] if r["building_id"] == str(building_id)), None
    )
    road, branch, evidence = _validate_review(
        report,
        row,
        stage,
        evidence_token=evidence_token,
        expected_revision=expected_revision,
        decision=decision,
        verified_road=verified_road,
        verified_prefix=verified_prefix,
        evidence=evidence,
    )
    saved = _write_review(
        repo,
        review_id,
        building_id,
        evidence_token=evidence_token,
        decision=decision,
        road=road,
        branch=branch,
        evidence=evidence,
    )
    repo.session.commit()
    return saved


def apply_staged_reviews(repo: Any, rows: list[dict]) -> None:
    candidates = {r["code"]: r for r in rows if not r.get("existing_id")}
    if not candidates:
        return
    stages = (
        repo.session.execute(
            text(
                "SELECT id,route_code,snapshot,revision FROM house_numbering.geometry_reviews WHERE organization_id=:org AND project_id=:project"
            ),
            {"org": repo.org_id, "project": repo.project_id},
        )
        .mappings()
        .all()
    )
    # A different logical code or regrouping cannot bypass a staged source segment.
    owners = {}
    for stage in stages:
        for ref in refs(stage["snapshot"]):
            owners.setdefault((ref["source_id"], ref["feature_id"]), set()).add(
                stage["route_code"]
            )
    for row in candidates.values():
        sources = row.get("segments") or [
            {"source_id": row["source_id"], "feature_id": row["feature_id"]}
        ]
        if any(
            owners.get((ref["source_id"], ref["feature_id"]), set()) - {row["code"]}
            for ref in sources
        ):
            row["errors"].append("review_stage_overlap")
            row["state"] = "error"
    for stage in stages:
        if stage["route_code"] not in candidates:
            continue
        row = candidates[stage["route_code"]]
        if not row.get("review_reason") or identity(row) != identity(stage["snapshot"]):
            row["errors"].append("review_stage_stale")
            row["state"] = "error"
            continue
        report = _report(repo, dict(stage))
        counts = report["counts"]
        row["address_review"] = {
            **counts,
            "review_id": str(stage["id"]),
            "geometry_current": report["geometry_current"],
            "digest": digest(
                [
                    (x["building_id"], x["evidence_token"], x["revision"], x["status"])
                    for x in report["items"]
                ]
            ),
        }
        if not report["geometry_current"]:
            row["errors"].append("review_stage_stale")
        elif report["overflow"] or counts["pending"]:
            row["errors"].append("review_address_pending")
        else:
            row["errors"] = [
                e for e in row["errors"] if e != "review_survey_address_conflict"
            ]
        row["state"] = "error" if row["errors"] else "ready"
