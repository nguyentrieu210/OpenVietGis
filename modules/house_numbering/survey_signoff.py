"""Spec 051: reviewer sign-off of a fully reconciled hamlet or road (Bình Hưng rule).

A survey point still needs work while it is not "khop" and its xu_ly is empty or "chua_xu_ly"
(same rule as the Bình Hưng app and the plugin UI). A scope can be signed only when no point
needs work; the database trigger from migration hn_0011 then refuses edits until a reviewer revokes.
"""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any
from uuid import UUID, uuid4

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformNotFoundError,
)
from sqlalchemy import text

from modules.house_numbering.persistence import HouseNumberingRepository

_COLUMNS = """id,source_id,ap,ten_duong,point_count,signed_by_actor_id,signed_at,note,
  revoked_by_actor_id,revoked_at,revoke_reason"""

# Values are trimmed like the UI's surveyText(); keep in sync with needsWork() in SurveyNavigator.tsx.
PENDING = (
    "btrim(coalesce(attributes->>'trang_thai','')) <> 'khop' "
    "AND coalesce(nullif(btrim(attributes->>'xu_ly'),''),'chua_xu_ly')='chua_xu_ly'"
)


def _wire(row: Any) -> dict[str, Any]:
    out = dict(row)
    for key in ("signed_at", "revoked_at"):
        if out.get(key) is not None:
            out[key] = out[key].isoformat()
    for key in ("id", "source_id", "signed_by_actor_id", "revoked_by_actor_id"):
        if out.get(key) is not None:
            out[key] = str(out[key])
    return out


def scope_counts(
    repo: HouseNumberingRepository, source_id: UUID, ap: str, ten_duong: str | None
) -> tuple[int, int]:
    row = repo.session.execute(
        text(f"""
          SELECT count(*)::int AS total, count(*) FILTER (WHERE {PENDING})::int AS pending
          FROM spatial.reference_features
          WHERE organization_id=:org AND project_id=:project AND source_id=:source
            AND attributes ? 'xu_ly' AND btrim(attributes->>'ap')=:ap
            AND (CAST(:road AS text) IS NULL OR btrim(attributes->>'ten_duong')=:road)
        """),
        {"org": repo.org_id, "project": repo.project_id, "source": source_id, "ap": ap, "road": ten_duong},
    ).mappings().one()
    return row["total"], row["pending"]


def list_signoffs(repo: HouseNumberingRepository, source_id: UUID) -> list[dict[str, Any]]:
    repo.prepare("read")
    rows = repo.session.execute(
        text(f"""SELECT {_COLUMNS} FROM house_numbering.survey_signoffs
          WHERE organization_id=:org AND project_id=:project AND source_id=:source AND revoked_at IS NULL
          ORDER BY ap, ten_duong NULLS FIRST"""),
        {"org": repo.org_id, "project": repo.project_id, "source": source_id},
    ).mappings().all()
    return [_wire(r) for r in rows]


def sign(
    repo: HouseNumberingRepository,
    source_ids: list[UUID],
    ap: str,
    ten_duong: str | None,
    note: str | None,
) -> list[dict[str, Any]]:
    """Sign the scope in every given survey source that holds points of it, all or nothing."""
    repo.prepare("manage")
    label = ap + (f" · {ten_duong}" if ten_duong else "")
    counted: list[tuple[UUID, int]] = []
    pending = total = 0
    for source_id in sorted(set(source_ids)):
        # Exclusive per source; point edits take it shared in the guard trigger, so a
        # concurrent "mở lại" either commits before this count or waits and is refused.
        repo.session.execute(
            text("SELECT pg_advisory_xact_lock(hashtextextended('hn_signoff:'||CAST(:source AS text),0))"),
            {"source": source_id},
        )
        n, p = scope_counts(repo, source_id, ap, ten_duong)
        if n:
            counted.append((source_id, n))
            total, pending = total + n, pending + p
    if total == 0:
        raise PlatformNotFoundError(f"{label} không có điểm khảo sát nào")
    if pending:
        raise PlatformConflictError(
            f"{label} còn {pending} điểm chưa đối chiếu",
            code="SURVEY_SCOPE_INCOMPLETE",
            details={"pending": pending, "total": total},
        )
    out = []
    for source_id, n in counted:
        exists = repo.session.execute(
            text("""SELECT 1 FROM house_numbering.survey_signoffs
              WHERE organization_id=:org AND project_id=:project AND source_id=:source AND revoked_at IS NULL
                AND ap=:ap AND (ten_duong IS NULL OR ten_duong=CAST(:road AS text)) LIMIT 1"""),
            {"org": repo.org_id, "project": repo.project_id, "source": source_id, "ap": ap, "road": ten_duong},
        ).first()
        if exists:
            raise PlatformConflictError(f"{label} đã được ký duyệt", code="SURVEY_SCOPE_SIGNED")
        signoff_id = uuid4()
        row = repo.session.execute(
            text(f"""INSERT INTO house_numbering.survey_signoffs
              (id,organization_id,project_id,source_id,ap,ten_duong,point_count,signed_by_actor_id,signed_at,note)
              VALUES (:id,:org,:project,:source,:ap,:road,:total,:actor,:now,:note) RETURNING {_COLUMNS}"""),
            {
                "id": signoff_id, "org": repo.org_id, "project": repo.project_id, "source": source_id,
                "ap": ap, "road": ten_duong, "total": n, "actor": repo.actor_id,
                "now": datetime.now(UTC), "note": note,
            },
        ).mappings().one()
        repo.audit("house_numbering.survey_signoff.sign", "survey_signoff", signoff_id,
                   {"source_id": str(source_id), "ap": ap, "ten_duong": ten_duong, "point_count": n})
        out.append(_wire(row))
    return out


def revoke(repo: HouseNumberingRepository, signoff_id: UUID, reason: str) -> dict[str, Any]:
    repo.prepare("manage")
    row = repo.session.execute(
        text(f"""UPDATE house_numbering.survey_signoffs
          SET revoked_by_actor_id=:actor, revoked_at=:now, revoke_reason=:reason
          WHERE organization_id=:org AND project_id=:project AND id=:id AND revoked_at IS NULL
          RETURNING {_COLUMNS}"""),
        {"actor": repo.actor_id, "now": datetime.now(UTC), "reason": reason,
         "org": repo.org_id, "project": repo.project_id, "id": signoff_id},
    ).mappings().first()
    if row is None:
        raise PlatformNotFoundError("không tìm thấy chữ ký đang hiệu lực")
    repo.audit("house_numbering.survey_signoff.revoke", "survey_signoff", signoff_id,
               {"ap": row["ap"], "ten_duong": row["ten_duong"], "reason": reason})
    return _wire(row)
