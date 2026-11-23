from __future__ import annotations

import base64
from io import BytesIO
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Response
from PIL import Image
from sqlalchemy import text
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.identity_repository import SqlIdentityRepository
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.identity.service import (
    CannotUnlinkLastIdentityError,
    IdentityConflictError,
    IdentityService,
)

router = APIRouter(tags=["identity"])


class UnlinkIdentityRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    identity_link_id: UUID


def _user_id(authentication: AuthenticatedRequest) -> UUID:
    actor_id = authentication.actor.actor_id
    if actor_id is None:
        raise HTTPException(status_code=401, detail="human identity required")
    return actor_id


@router.get("/me")
def get_current_user(
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    response.headers["Cache-Control"] = "no-store, private"
    user = SqlIdentityRepository(session).get_user(_user_id(authentication))
    if user is None:
        raise HTTPException(status_code=401, detail="user not found")
    return {
        "id": user.id,
        "display_name": user.display_name,
        "email_snapshot": user.email_snapshot,
        "status": user.status.value,
    }


@router.get("/me/organizations")
def list_my_organizations(
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> list[dict[str, object]]:
    items = SqlTenancyRepository(session).list_active_organizations_for_user(
        _user_id(authentication), actor=authentication.actor
    )
    return [
        {
            "id": organization.id,
            "name": organization.name,
            "slug": organization.slug,
            "role": membership.role.value,
            "status": organization.status.value,
        }
        for organization, membership in items
    ]


@router.get("/me/identity-links")
def list_identity_links(
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> list[dict[str, object]]:
    links = SqlIdentityRepository(session).list_identity_links(_user_id(authentication))
    return [
        {
            "id": link.id,
            "issuer": link.issuer,
            "subject": link.subject,
            "email_snapshot": link.email_snapshot,
            "email_verified": link.email_verified,
        }
        for link in links
    ]


@router.delete("/me/identity-links", status_code=204)
def unlink_identity(
    payload: UnlinkIdentityRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> None:
    service = IdentityService(
        SqlIdentityRepository(session),
        SqlAuditRepository(session),
    )
    try:
        service.unlink_identity(
            user_id=_user_id(authentication),
            identity_link_id=payload.identity_link_id,
        )
    except CannotUnlinkLastIdentityError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except IdentityConflictError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


class AvatarRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    data_url: str = Field(max_length=150000)

def _human_avatar_id(authentication: AuthenticatedRequest) -> UUID:
    if authentication.actor.actor_type != "human":
        raise HTTPException(status_code=403,detail="human identity required")
    return _user_id(authentication)

@router.get("/me/avatar")
def get_avatar(response: Response, authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
               session: Annotated[Session, Depends(get_db_session, scope="function")]) -> dict:
    uid=_human_avatar_id(authentication)
    response.headers["Cache-Control"]="private, no-store"
    value=session.execute(text("SELECT data_url FROM identity.user_avatars WHERE user_id=:uid"),{"uid":uid}).scalar_one_or_none()
    return {"data_url":value}

@router.put("/me/avatar")
def put_avatar(payload: AvatarRequest, authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
               session: Annotated[Session, Depends(get_db_session, scope="function")]) -> dict:
    uid=_human_avatar_id(authentication)
    try:
        raw=base64.b64decode(payload.data_url.removeprefix("data:image/jpeg;base64,"), validate=True)
        if not payload.data_url.startswith("data:image/jpeg;base64,"): raise ValueError()
        image=Image.open(BytesIO(raw),formats=["JPEG"])
        if image.width!=192 or image.height!=192: raise ValueError()
        out=BytesIO()
        image.convert("RGB").save(out,format="JPEG",quality=85)
        value="data:image/jpeg;base64,"+base64.b64encode(out.getvalue()).decode("ascii")
    except (ValueError, OSError, Image.DecompressionBombError):
        raise HTTPException(status_code=422,detail="avatar must be a valid 192x192 JPEG")
    session.execute(text("""INSERT INTO identity.user_avatars(user_id,data_url) VALUES(:uid,:data)
        ON CONFLICT(user_id) DO UPDATE SET data_url=excluded.data_url,updated_at=now()"""),{"uid":uid,"data":value})
    session.commit()
    return {"data_url":value}

@router.delete("/me/avatar",status_code=204)
def remove_avatar(authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
                  session: Annotated[Session, Depends(get_db_session, scope="function")]) -> None:
    uid=_human_avatar_id(authentication)
    session.execute(text("DELETE FROM identity.user_avatars WHERE user_id=:uid"),{"uid":uid})
    session.commit()
