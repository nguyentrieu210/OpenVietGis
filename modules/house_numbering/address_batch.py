"""Atomic shared-evidence verification for non-conflicting missing address fields."""

from __future__ import annotations

from collections import defaultdict
from typing import Any
from uuid import UUID, uuid4

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformValidationError,
)
from sqlalchemy import text

from modules.house_numbering.address_verification import (
    _lock_review,
    _report,
    _validate_review,
    _write_review,
    digest,
    identity,
)
from modules.house_numbering.building_assignment import alley_name, road_name
from modules.house_numbering.route_recovery import address_label

MAX_BATCH = 100


def group_key(stage: dict, row: dict) -> str | None:
    if (
        row.get("status") != "pending"
        or not row.get("eligible")
        or not row.get("can_belong")
    ):
        return None
    root, prefix = address_label(stage["snapshot"]["name"]) or ("", "")
    if not root or not prefix:
        return None
    road, alley = road_name(row["road"]), alley_name(row["alley"])
    kind = row["kind"]
    if not (
        kind == "missing_prefix"
        and road == root
        and not alley
        or kind == "missing_road"
        and not road
        and alley == prefix.lower()
    ):
        return None
    if not row["code"]:
        return None
    return digest([identity(stage["snapshot"]), kind, road, alley, row["code"]])


def verified_path(repo: Any, stage: dict) -> tuple[str, str] | None:
    s = stage["snapshot"]
    label = address_label(s["name"])
    if not label:
        return None
    root = repo.session.execute(
        text("""
      WITH RECURSIVE ancestors AS (
        SELECT id,name,route_type,parent_route_id,ARRAY[id] path FROM house_numbering.routes
        WHERE organization_id=:org AND project_id=:project AND id=CAST(:parent AS uuid)
        UNION ALL
        SELECT r.id,r.name,r.route_type,r.parent_route_id,a.path||r.id FROM ancestors a
        JOIN house_numbering.routes r ON r.id=a.parent_route_id AND r.organization_id=:org AND r.project_id=:project
        WHERE NOT r.id=ANY(a.path) AND cardinality(a.path)<100
      )
      SELECT name FROM ancestors WHERE route_type='street' AND parent_route_id IS NULL
    """),
        {"org": repo.org_id, "project": repo.project_id, "parent": s["parent_id"]},
    ).scalar_one_or_none()
    if (
        not root
        or road_name(root) != label[0]
        or alley_name(s["resolved_prefix"]) != label[1].lower()
    ):
        return None
    return root, s["resolved_prefix"]


def group_list(repo: Any, stage: dict, items: list[dict]) -> list[dict]:
    groups = defaultdict(list)
    for row in items:
        key = group_key(stage, row)
        if key:
            groups[key].append(row)
    groups = {k: v for k, v in groups.items() if len(v) >= 2}
    if not groups:
        return []
    path = verified_path(repo, stage)
    if not path:
        return []
    road, prefix = path
    return [
        {
            "key": key,
            "kind": rows[0]["kind"],
            "road": rows[0]["road"],
            "alley": rows[0]["alley"],
            "code": rows[0]["code"],
            "verified_road": road,
            "verified_prefix": prefix,
            "count": len(rows),
            "distance_min": min(r["distance_m"] for r in rows),
            "distance_max": max(r["distance_m"] for r in rows),
            "members": [
                {
                    k: r[k]
                    for k in (
                        "building_id",
                        "source_id",
                        "feature_id",
                        "distance_m",
                        "evidence_token",
                        "revision",
                        "road",
                        "alley",
                    )
                }
                for r in rows
            ],
        }
        for key, rows in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0]))
    ]


def save_batch(
    repo: Any,
    review_id: UUID,
    *,
    key: str,
    members: list[dict],
    evidence: str,
    confirmed: bool,
) -> dict:
    ids = [m["building_id"] for m in members]
    if (
        confirmed is not True
        or not 2 <= len(ids) <= MAX_BATCH
        or len(set(ids)) != len(ids)
    ):
        raise PlatformValidationError(
            "Chọn từ 2 đến 100 nhà khác nhau và xác nhận căn cứ chung",
            code="INVALID_ADDRESS_BATCH",
        )
    stage = _lock_review(repo, review_id)
    # Serialize with building/source changes before reading the current report.
    locked = (
        repo.session.execute(
            text("""
      SELECT b.id FROM house_numbering.buildings b JOIN spatial.reference_features f
        ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
      WHERE b.organization_id=:org AND b.project_id=:project AND b.id=ANY(CAST(:ids AS uuid[]))
      ORDER BY b.id FOR UPDATE OF b FOR SHARE OF f
    """),
            {"org": repo.org_id, "project": repo.project_id, "ids": ids},
        )
        .scalars()
        .all()
    )
    if len(locked) != len(ids):
        raise PlatformConflictError(
            "Danh sách nhà đã thay đổi, hãy tải lại", code="STALE_ADDRESS_BATCH"
        )
    report = _report(repo, stage)
    path = verified_path(repo, stage)
    if not report["geometry_current"] or report["overflow"] or not path:
        raise PlatformConflictError(
            "Tuyến hoặc dữ liệu đã thay đổi, hãy tải lại", code="STALE_ADDRESS_BATCH"
        )
    rows = {r["building_id"]: r for r in report["items"]}
    validated = []
    for m in sorted(members, key=lambda v: str(v["building_id"])):
        row = rows.get(str(m["building_id"]))
        if (
            not row
            or row["status"] != "pending"
            or row["evidence_token"] != m["evidence_token"]
            or row["revision"] != m["expected_revision"]
        ):
            raise PlatformConflictError(
                "Một nhà đã thay đổi hoặc đã xác minh; chưa lưu cả lô",
                code="STALE_ADDRESS_BATCH",
            )
        if group_key(stage, row) != key:
            raise PlatformValidationError(
                "Không cùng nhóm hoặc địa chỉ có mâu thuẫn; hãy xác minh riêng",
                code="INVALID_ADDRESS_BATCH",
            )
        road, branch, reason = _validate_review(
            report,
            row,
            stage,
            evidence_token=m["evidence_token"],
            expected_revision=m["expected_revision"],
            decision="belongs",
            verified_road=path[0],
            verified_prefix=path[1],
            evidence=evidence,
        )
        validated.append((m, road, branch, reason))
    batch_id = uuid4()
    for m, road, branch, reason in validated:
        _write_review(
            repo,
            review_id,
            m["building_id"],
            evidence_token=m["evidence_token"],
            decision="belongs",
            road=road,
            branch=branch,
            evidence=reason,
            batch_id=batch_id,
        )
    repo.audit(
        "house_numbering.building.verify_address_batch",
        "house_numbering_geometry_review",
        review_id,
        {
            "batch_id": str(batch_id),
            "group_key": key,
            "building_ids": [str(v) for v in ids],
            "count": len(ids),
            "evidence": evidence.strip(),
        },
    )
    repo.session.commit()
    return {
        "batch_id": str(batch_id),
        "verified": len(ids),
        "buildings_updated": 0,
        "routes_created": 0,
    }
