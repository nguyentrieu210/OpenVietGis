"""Link verified buildings to their exact promoted route, with fresh evidence and explicit selection."""

from __future__ import annotations

import json
from typing import Any
from uuid import UUID, uuid4

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformValidationError,
)
from sqlalchemy import text

from modules.house_numbering.address_batch import verified_path
from modules.house_numbering.address_verification import (
    _report,
    _stage,
    digest,
    identity,
    refs,
)
from modules.house_numbering.building_assignment import alley_name, road_name

STATUSES = ("ready", "linked", "other_address", "blocked", "stale")


def promoted_route(repo: Any, stage: dict) -> dict | None:
    snapshot = stage["snapshot"]
    rows = (
        repo.session.execute(
            text("""
      SELECT r.id,r.name,r.route_type,r.parent_route_id,r.prefix,r.status,r.revision,l.content_revision AS geometry_revision
      FROM house_numbering.routes r JOIN house_numbering.route_lines l
        ON (l.organization_id,l.project_id,l.route_id)=(r.organization_id,r.project_id,r.id)
      WHERE r.organization_id=:org AND r.project_id=:project AND r.source_label=:label
        AND r.name=:name AND r.route_type=:type AND r.parent_route_id=CAST(:parent AS uuid)
        AND r.prefix=:prefix AND r.linked_source_id=CAST(:source AS uuid) AND r.linked_feature_id=:feature
        AND r.direction_state=:direction AND COALESCE(r.segments,'[]'::jsonb)=CAST(:segments AS jsonb)
      LIMIT 2
    """),
            {
                "org": repo.org_id,
                "project": repo.project_id,
                "label": "route-tree-import:" + stage["route_code"],
                "name": snapshot["name"],
                "type": snapshot["route_type"],
                "parent": snapshot["parent_id"],
                "prefix": snapshot["resolved_prefix"],
                "source": snapshot["source_id"],
                "feature": snapshot["feature_id"],
                "direction": snapshot["direction_state"],
                "segments": json.dumps(snapshot.get("segments") or []),
            },
        )
        .mappings()
        .all()
    )
    return dict(rows[0]) if len(rows) == 1 else None


def link_status(
    record: dict, current: dict | None, target_id: str, path: tuple[str, str]
) -> str:
    if record["decision"] != "belongs":
        return "other_address"
    if record["state"] != "active" or record["eligibility"] == "ineligible":
        return "blocked"
    if record["route_id"]:
        return "linked" if str(record["route_id"]) == target_id else "blocked"
    if record["locked"]:
        return "blocked"
    if (
        not current
        or current["status"] != "verified"
        or current.get("decision") != "belongs"
    ):
        return "stale"
    if (
        not current["can_belong"]
        or record["evidence_token"] != current["evidence_token"]
        or record["review_revision"] != current["revision"]
    ):
        return "stale"
    if road_name(record["verified_road"]) != road_name(path[0]) or alley_name(
        record["verified_prefix"]
    ) != alley_name(path[1]):
        return "stale"
    return "ready"


def _plan(repo: Any, stage: dict) -> dict:
    counts = {k: 0 for k in STATUSES}
    unavailable = {
        "state": "not_promoted",
        "route": None,
        "counts": counts,
        "items": [],
        "preview_token": None,
    }
    route = promoted_route(repo, stage)
    if route is None:
        return unavailable
    # Same source links alone are insufficient: all original revisions, junction,
    # and the actual root-to-alley address path must still be current.
    report = _report(repo, stage)
    path = verified_path(repo, stage)
    public_route = {
        **route,
        "id": str(route["id"]),
        "parent_route_id": str(route["parent_route_id"]),
    }
    if not report["geometry_current"] or report["overflow"] or not path:
        return {**unavailable, "state": "stale_route", "route": public_route}
    current = {r["building_id"]: r for r in report["items"]}
    records = (
        repo.session.execute(
            text("""
      SELECT a.id review_entry_id,a.building_id,a.revision review_revision,a.decision,a.verified_road,a.verified_prefix,a.evidence,a.evidence_token,
        b.revision building_revision,b.route_id,b.state,b.eligibility,b.linked_source_id source_id,b.linked_feature_id feature_id,
        EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
          AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('proposed','submitted','approved','reserved')) locked
      FROM house_numbering.geometry_address_reviews a JOIN house_numbering.buildings b
        ON (b.organization_id,b.project_id,b.id)=(a.organization_id,a.project_id,a.building_id)
      WHERE a.organization_id=:org AND a.project_id=:project AND a.geometry_review_id=:review
      ORDER BY a.building_id LIMIT 10001
    """),
            {"org": repo.org_id, "project": repo.project_id, "review": stage["id"]},
        )
        .mappings()
        .all()
    )
    if len(records) > 10000:
        return {**unavailable, "state": "overflow", "route": public_route}
    items = []
    for rec in records:
        r = dict(rec)
        bid = str(r["building_id"])
        c = current.get(bid)
        status = link_status(r, c, str(route["id"]), path)
        counts[status] += 1
        items.append(
            {
                "building_id": bid,
                "review_entry_id": str(r["review_entry_id"]),
                "review_revision": r["review_revision"],
                "building_revision": r["building_revision"],
                "source_id": str(r["source_id"]),
                "feature_id": r["feature_id"],
                "decision": r["decision"],
                "verified_road": r["verified_road"],
                "verified_prefix": r["verified_prefix"],
                "evidence": r["evidence"],
                "evidence_token": r["evidence_token"],
                "current_token": c["evidence_token"] if c else None,
                "distance_m": c["distance_m"] if c else None,
                "raw_road": c["road"] if c else None,
                "raw_alley": c["alley"] if c else None,
                "linked_route_id": str(r["route_id"]) if r["route_id"] else None,
                "status": status,
            }
        )
    token = digest(
        [identity(stage["snapshot"]), stage["revision"], public_route, items]
    )
    return {
        "state": "current",
        "route": public_route,
        "counts": counts,
        "items": items,
        "preview_token": token,
    }


def preview_links(
    repo: Any, review_id: UUID, offset: int = 0, limit: int = 100
) -> dict:
    repo.prepare("read")
    plan = _plan(repo, _stage(repo, review_id))
    items = plan.pop("items")
    return {
        **plan,
        "items": items[offset : offset + limit],
        "next_offset": offset + limit if len(items) > offset + limit else None,
    }


def apply_links(
    repo: Any,
    review_id: UUID,
    *,
    preview_token: str,
    building_ids: list[UUID],
    confirmed: bool,
) -> dict:
    if (
        confirmed is not True
        or not 1 <= len(building_ids) <= 100
        or len(set(building_ids)) != len(building_ids)
    ):
        raise PlatformValidationError(
            "Chọn từ 1 đến 100 nhà khác nhau và xác nhận liên kết",
            code="INVALID_VERIFIED_LINKS",
        )
    repo.prepare("edit")
    session = repo.session
    # Serialize with route import, review decisions and the normal assignment workbench.
    for lock in ("route-import", "assignment"):
        session.execute(
            text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
            {"key": f"house-numbering-{lock}:{repo.project_id}"},
        )
    stage = _stage(repo, review_id, True)
    s = stage["snapshot"]
    scope = {"org": repo.org_id, "project": repo.project_id}
    routes = (
        session.execute(
            text("""
      WITH RECURSIVE lineage(id) AS (
        SELECT r.id FROM house_numbering.routes r WHERE r.organization_id=:org AND r.project_id=:project
          AND (r.source_label=:label OR r.id=CAST(:parent AS uuid))
        UNION
        SELECT r.parent_route_id FROM house_numbering.routes r JOIN lineage a ON a.id=r.id
          WHERE r.organization_id=:org AND r.project_id=:project AND r.parent_route_id IS NOT NULL
      )
      SELECT r.id,r.linked_source_id,r.linked_feature_id,r.segments FROM house_numbering.routes r JOIN lineage t ON t.id=r.id
        WHERE r.organization_id=:org AND r.project_id=:project ORDER BY r.id FOR SHARE OF r
    """),
            {
                **scope,
                "label": "route-tree-import:" + stage["route_code"],
                "parent": s["parent_id"],
            },
        )
        .mappings()
        .all()
    )
    links = {(str(x["source_id"]), x["feature_id"]) for x in refs(s)}
    for route in routes:
        links.update(
            (str(x["source_id"]), x["feature_id"])
            for x in (
                route["segments"]
                or [
                    {
                        "source_id": route["linked_source_id"],
                        "feature_id": route["linked_feature_id"],
                    }
                ]
            )
        )
    session.execute(
        text("""
      SELECT f.source_id,f.feature_id FROM jsonb_to_recordset(CAST(:links AS jsonb)) AS x(source_id uuid,feature_id text)
      JOIN spatial.reference_features f ON f.organization_id=:org AND f.project_id=:project AND f.source_id=x.source_id AND f.feature_id=x.feature_id
      JOIN spatial.data_sources ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
      ORDER BY f.source_id,f.feature_id FOR SHARE OF f,ds
    """),
        {
            **scope,
            "links": json.dumps(
                [{"source_id": sid, "feature_id": fid} for sid, fid in sorted(links)]
            ),
        },
    ).all()
    locked = (
        session.execute(
            text("""
      SELECT b.id FROM house_numbering.buildings b JOIN spatial.reference_features f
        ON (b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)=(f.organization_id,f.project_id,f.source_id,f.feature_id)
      JOIN spatial.data_sources ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
      WHERE b.organization_id=:org AND b.project_id=:project AND b.id=ANY(CAST(:ids AS uuid[]))
      ORDER BY b.id FOR UPDATE OF b FOR SHARE OF f,ds
    """),
            {**scope, "ids": building_ids},
        )
        .scalars()
        .all()
    )
    if len(locked) != len(building_ids):
        raise PlatformConflictError(
            "Danh sách nhà đã thay đổi", code="STALE_VERIFIED_LINKS"
        )
    plan = _plan(repo, stage)
    if not plan["preview_token"] or plan["preview_token"] != preview_token:
        raise PlatformConflictError(
            "Tuyến hoặc hồ sơ đã thay đổi, hãy xem trước lại",
            code="STALE_VERIFIED_LINKS",
        )
    rows = {r["building_id"]: r for r in plan["items"]}
    if any(
        str(bid) not in rows or rows[str(bid)]["status"] != "ready"
        for bid in building_ids
    ):
        raise PlatformValidationError(
            "Chỉ liên kết nhà đã xác minh thuộc tuyến, chưa liên kết và không có số bảo vệ",
            code="INVALID_VERIFIED_LINKS",
        )
    rid = UUID(plan["route"]["id"])
    batch_id = uuid4()
    for bid in sorted(building_ids, key=str):
        row = rows[str(bid)]
        current = repo.get_building(bid, for_update=True)
        result = repo.update_building(
            bid,
            expected_revision=row["building_revision"],
            route_id=rid,
            area_id=current["area_id"],
            parcel_ref=current["parcel_ref"],
            source_id=current["linked_source_id"],
            feature_id=current["linked_feature_id"],
        )
        repo.audit(
            "house_numbering.building.link_verified_address",
            "house_numbering_building",
            bid,
            {
                "batch_id": str(batch_id),
                "geometry_review_id": str(review_id),
                "address_review_id": row["review_entry_id"],
                "review_revision": row["review_revision"],
                "evidence_token": row["evidence_token"],
                "route_id": str(rid),
                "before_revision": row["building_revision"],
                "after_revision": result["revision"],
            },
        )
    repo.audit(
        "house_numbering.building.link_verified_address_batch",
        "house_numbering_geometry_review",
        review_id,
        {
            "batch_id": str(batch_id),
            "route_id": str(rid),
            "building_ids": [str(bid) for bid in building_ids],
            "count": len(building_ids),
            "preview_token": preview_token,
        },
    )
    session.commit()
    return {
        "linked": len(building_ids),
        "route_id": str(rid),
        "batch_id": str(batch_id),
        "numbers_created": 0,
    }
