"""Self-service publisher identity, signed submissions, explicit platform review.

Publication remains the separately signed operator catalog; approval alone is not publishing.
"""

from __future__ import annotations

import base64
import json
from typing import Annotated, Any, Literal
from uuid import UUID, uuid4

from cryptography.exceptions import InvalidSignature
from fastapi import APIRouter, Depends, Query, Response
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.config import get_settings
from openvietgis_api.domains.extensions.marketplace import (
    SCHEMA,
    canonical,
    load_catalog,
    validate_catalog,
    verify,
)
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformForbiddenError,
    PlatformNotFoundError,
    PlatformValidationError,
)

router = APIRouter(prefix="/developer/marketplace", tags=["marketplace-developer"])
Auth = Annotated[AuthenticatedRequest, Depends(authenticate_request)]
WriteAuth = Annotated[AuthenticatedRequest, Depends(require_csrf)]
DB = Annotated[Session, Depends(get_db_session, scope="function")]
_QUOTA = 30


class PublisherRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=120)
    public_key: str = Field(min_length=44, max_length=44)
    proof: str = Field(min_length=88, max_length=88)


class SubmissionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    publisher_id: str = Field(min_length=2, max_length=32)
    manifest: dict[str, Any]
    signature: str = Field(min_length=88, max_length=88)


class DecisionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    decision: Literal["approved", "rejected"]
    note: str = Field(min_length=1, max_length=2000)


def _human(auth: AuthenticatedRequest) -> UUID:
    if auth.actor.actor_type != "human" or auth.actor.actor_id is None:
        raise PlatformForbiddenError("human account required")
    return auth.actor.actor_id


def _can_review(uid: UUID) -> bool:
    return uid in get_settings().marketplace_reviewer_user_ids


def _operator(session: Session, uid: UUID) -> None:
    if not _can_review(uid):
        raise PlatformForbiddenError("platform reviewer required")
    session.execute(text("SELECT set_config('openvietgis.marketplace_reviewer','yes',true)"))


def _private(response: Response) -> None:
    response.headers["Cache-Control"] = "private, no-store"


def _serialize_uid(uid: UUID) -> dict[str, UUID]:
    return {"uid": uid}


def _lock_user(session: Session, uid: UUID) -> None:
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:lock_key,0))"),
        {"lock_key": f"marketplace-publisher:{uid}"},
    )


def _publisher(session: Session, uid: UUID) -> dict[str, Any] | None:
    row = (
        session.execute(
            text("""
            SELECT id,name,public_key,created_at
            FROM extensions.marketplace_publishers WHERE owner_user_id=:uid
        """),
            _serialize_uid(uid),
        )
        .mappings()
        .one_or_none()
    )
    return dict(row) if row else None


def _catalog() -> tuple[list[dict[str, Any]], bool]:
    settings = get_settings()
    try:
        return load_catalog(
            settings.marketplace_catalog_path, settings.marketplace_operator_public_key
        ), True
    except PlatformValidationError:
        # A broken catalog must not turn a pending/approved request into a public release.
        return [], False


def _submission(row, *, uid: UUID, catalog: list[dict[str, Any]]) -> dict[str, Any]:
    result = dict(row)
    result["own"] = result.pop("owner_user_id") == uid
    result["published"] = result["status"] == "approved" and any(
        item["publisher_id"] == result["publisher_id"]
        and item["signature"] == result["signature"]
        and item["manifest"] == result["manifest"]
        for item in catalog
    )
    return result


_SELECT = """
    SELECT s.id,s.publisher_id,s.owner_user_id,s.manifest,s.signature,s.status,
           s.review_note,s.reviewed_at,s.created_at,p.name AS publisher_name,p.public_key
    FROM extensions.marketplace_submissions s
    JOIN extensions.marketplace_publishers p ON p.id=s.publisher_id
"""


@router.get("/me")
def my_developer_state(response: Response, auth: Auth, session: DB) -> dict[str, Any]:
    uid = _human(auth)
    _private(response)
    publisher = _publisher(session, uid)
    rows = (
        session.execute(
            text(_SELECT + " WHERE s.owner_user_id=:uid ORDER BY s.created_at DESC,s.id"),
            _serialize_uid(uid),
        )
        .mappings()
        .all()
    )
    catalog, available = _catalog()
    return {
        "user_id": uid,
        "publisher": publisher,
        "submissions": [_submission(r, uid=uid, catalog=catalog) for r in rows],
        "can_review": _can_review(uid),
        "catalog_available": available,
        "quota": _QUOTA,
    }


@router.post("/publisher", status_code=201)
def register_publisher(
    body: PublisherRequest, response: Response, auth: WriteAuth, session: DB
) -> dict[str, Any]:
    uid = _human(auth)
    _private(response)
    try:
        if not body.name.strip() or body.name != body.name.strip():
            raise ValueError("publisher name must be trimmed")
        key = base64.b64decode(body.public_key, validate=True)
        if len(key) != 32 or base64.b64encode(key).decode() != body.public_key:
            raise ValueError("invalid public key")
        verify(
            body.public_key,
            {
                "purpose": "openvietgis.publisher.v1",
                "user_id": str(uid),
                "name": body.name,
                "public_key": body.public_key,
            },
            body.proof,
        )
    except (ValueError, TypeError, InvalidSignature) as exc:
        raise PlatformValidationError("invalid publisher key proof") from exc
    _lock_user(session, uid)
    if _publisher(session, uid):
        raise PlatformConflictError("publisher already registered")
    pid = "dev_" + uuid4().hex[:24]
    result = (
        session.execute(
            text("""
            INSERT INTO extensions.marketplace_publishers(id,owner_user_id,name,public_key)
            VALUES(:pid,:uid,:name,:key)
            RETURNING id,name,public_key,created_at
        """),
            {"pid": pid, "uid": uid, "name": body.name, "key": body.public_key},
        )
        .mappings()
        .one()
    )
    result_data = dict(result)
    session.commit()
    return result_data


@router.post("/submissions", status_code=201)
def submit_release(
    body: SubmissionRequest, response: Response, auth: WriteAuth, session: DB
) -> dict[str, Any]:
    uid = _human(auth)
    _private(response)
    _lock_user(session, uid)
    publisher = _publisher(session, uid)
    if publisher is None:
        raise PlatformForbiddenError("register publisher first")
    if publisher["id"] != body.publisher_id:
        raise PlatformForbiddenError("publisher ownership required")
    release = body.model_dump()
    try:
        if len(canonical(body.manifest)) > 24_000:
            raise ValueError("manifest too large")
        validate_catalog(
            {
                "schema": SCHEMA,
                "publishers": [{k: publisher[k] for k in ("id", "name", "public_key")}],
                "releases": [release],
            }
        )
    except (ValueError, TypeError, AttributeError, KeyError, InvalidSignature) as exc:
        raise PlatformValidationError("invalid signed submission") from exc
    count = session.execute(
        text("SELECT COUNT(*) FROM extensions.marketplace_submissions WHERE owner_user_id=:uid"),
        _serialize_uid(uid),
    ).scalar_one()
    if count >= _QUOTA:
        raise PlatformConflictError("submission quota reached; contact operator")
    mid = uuid4()
    inserted = session.execute(
        text("""
            INSERT INTO extensions.marketplace_submissions
              (id,publisher_id,owner_user_id,extension_key,version,manifest,signature)
            VALUES (:id,:pid,:uid,:key,:version,CAST(:manifest AS jsonb),:signature)
            ON CONFLICT(extension_key,version) DO NOTHING RETURNING id
        """),
        {
            "id": mid,
            "pid": publisher["id"],
            "uid": uid,
            "key": body.manifest["extension_key"],
            "version": body.manifest["version"],
            "manifest": json.dumps(body.manifest, ensure_ascii=False),
            "signature": body.signature,
        },
    ).scalar_one_or_none()
    if inserted is None:
        raise PlatformConflictError("version already submitted; use a new version")
    row = session.execute(text(_SELECT + " WHERE s.id=:id"), {"id": mid}).mappings().one()
    result = _submission(row, uid=uid, catalog=[])
    session.commit()
    return result


@router.post("/submissions/{submission_id}/withdraw")
def withdraw(
    submission_id: UUID, response: Response, auth: WriteAuth, session: DB
) -> dict[str, Any]:
    uid = _human(auth)
    _private(response)
    params = {"uid": uid, "id": submission_id}
    found = session.execute(
        text(
            "SELECT status FROM extensions.marketplace_submissions "
            "WHERE id=:id AND owner_user_id=:uid"
        ),
        params,
    ).scalar_one_or_none()
    if found is None:
        raise PlatformNotFoundError("submission not found")
    changed = session.execute(
        text("""
        UPDATE extensions.marketplace_submissions SET status='withdrawn'
        WHERE id=:id AND owner_user_id=:uid AND status='pending' RETURNING id
    """),
        params,
    ).scalar_one_or_none()
    if changed is None:
        raise PlatformConflictError("only pending submissions may be withdrawn")
    row = session.execute(text(_SELECT + " WHERE s.id=:id"), params).mappings().one()
    result = _submission(row, uid=uid, catalog=[])
    session.commit()
    return result


@router.get("/review-queue")
def review_queue(
    response: Response,
    auth: Auth,
    session: DB,
    state: Literal["pending", "approved", "rejected", "withdrawn"] = "pending",
    offset: Annotated[int, Query(ge=0, le=100000)] = 0,
) -> dict[str, Any]:
    uid = _human(auth)
    _private(response)
    _operator(session, uid)
    rows = (
        session.execute(
            text(
                _SELECT
                + """
            WHERE s.status=:state ORDER BY s.created_at,s.id LIMIT 101 OFFSET :offset
        """
            ),
            {"state": state, "offset": offset},
        )
        .mappings()
        .all()
    )
    catalog, available = _catalog()
    return {
        "items": [_submission(r, uid=uid, catalog=catalog) for r in rows[:100]],
        "next_offset": offset + 100 if len(rows) > 100 else None,
        "catalog_available": available,
    }


@router.post("/review-queue/{submission_id}/decision")
def review(
    submission_id: UUID,
    body: DecisionRequest,
    response: Response,
    auth: WriteAuth,
    session: DB,
) -> dict[str, Any]:
    uid = _human(auth)
    _private(response)
    _operator(session, uid)
    if not body.note.strip():
        raise PlatformValidationError("review note required")
    params = {"id": submission_id, "uid": uid, "decision": body.decision, "note": body.note.strip()}
    row = (
        session.execute(
            text(
                "SELECT owner_user_id,status FROM extensions.marketplace_submissions WHERE id=:id"
            ),
            params,
        )
        .mappings()
        .one_or_none()
    )
    if row is None:
        raise PlatformNotFoundError("submission not found")
    if row["owner_user_id"] == uid:
        raise PlatformForbiddenError("self-review forbidden")
    changed = session.execute(
        text("""
            UPDATE extensions.marketplace_submissions
            SET status=:decision,review_note=:note,reviewed_by=:uid,reviewed_at=now()
            WHERE id=:id AND status='pending' AND owner_user_id<>:uid RETURNING id
        """),
        params,
    ).scalar_one_or_none()
    if changed is None:
        raise PlatformConflictError("submission already decided")
    row = session.execute(text(_SELECT + " WHERE s.id=:id"), params).mappings().one()
    catalog, _ = _catalog()
    result = _submission(row, uid=uid, catalog=catalog)
    session.commit()
    return result
