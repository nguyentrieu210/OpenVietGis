from __future__ import annotations

from datetime import datetime
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Response
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.http.tenant_access import require_human_permission
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlResourceGrantRepository,
    SqlServicePrincipalRepository,
)
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import (
    InvalidServiceCredentialError,
    ServicePrincipalService,
)

router = APIRouter(tags=["service-principals"])


class CreateServicePrincipalRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str = Field(min_length=1, max_length=200)
    description: str | None = None


class IssueServiceCredentialRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str = Field(min_length=1, max_length=200)
    permission_ceiling: list[str] | None = None
    expires_at: datetime | None = None


def _service(session: Session) -> ServicePrincipalService:
    return ServicePrincipalService(
        SqlServicePrincipalRepository(session),
        SqlResourceGrantRepository(session),
        SqlTenancyRepository(session),
        SqlAuditRepository(session),
    )


def _serialize_principal(principal) -> dict[str, object]:
    return {
        "id": principal.id,
        "organization_id": principal.organization_id,
        "name": principal.name,
        "description": principal.description,
        "status": principal.status.value,
    }


@router.get("/organizations/{organization_id}/service-principals")
def list_service_principals(
    organization_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> list[dict[str, object]]:
    require_human_permission(
        session,
        authentication,
        organization_id,
        Permission.SERVICE_PRINCIPAL_MANAGE,
    )
    context = context_for(authentication, organization_id=organization_id)
    return [
        _serialize_principal(principal)
        for principal in SqlServicePrincipalRepository(session).list_principals(
            organization_id,
            context=context,
        )
    ]


@router.post("/organizations/{organization_id}/service-principals", status_code=201)
def create_service_principal(
    organization_id: UUID,
    payload: CreateServicePrincipalRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    require_human_permission(
        session,
        authentication,
        organization_id,
        Permission.SERVICE_PRINCIPAL_MANAGE,
        lock=True,
    )
    try:
        principal = _service(session).create_principal(
            actor=authentication.actor,
            organization_id=organization_id,
            name=payload.name,
            description=payload.description,
        )
    except IntegrityError as exc:
        raise HTTPException(
            status_code=409, detail="service principal name already exists"
        ) from exc
    return _serialize_principal(principal)


@router.post(
    "/organizations/{organization_id}/service-principals/{service_principal_id}/revoke",
    status_code=204,
)
def revoke_service_principal(
    organization_id: UUID,
    service_principal_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    require_human_permission(
        session,
        authentication,
        organization_id,
        Permission.SERVICE_PRINCIPAL_MANAGE,
        lock=True,
    )
    _service(session).revoke_principal(
        actor=authentication.actor,
        organization_id=organization_id,
        service_principal_id=service_principal_id,
    )
    return Response(status_code=204)


class ServiceCredentialMetadataResponse(BaseModel):
    id: UUID
    name: str
    prefix: str
    permission_ceiling: list[str] | None
    created_at: datetime
    expires_at: datetime | None
    last_used_at: datetime | None
    revoked_at: datetime | None


@router.get(
    "/organizations/{organization_id}/service-principals/"
    "{service_principal_id}/credentials",
    response_model=list[ServiceCredentialMetadataResponse],
)
def list_service_credentials(
    organization_id: UUID,
    service_principal_id: UUID,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> list[dict[str, object]]:
    require_human_permission(
        session, authentication, organization_id, Permission.SERVICE_PRINCIPAL_MANAGE
    )
    context = context_for(authentication, organization_id=organization_id)
    repo = SqlServicePrincipalRepository(session)
    if repo.get_principal(
        context=context, organization_id=organization_id,
        service_principal_id=service_principal_id,
    ) is None:
        raise HTTPException(status_code=404, detail="service principal not found")
    response.headers["Cache-Control"] = "no-store"
    return repo.list_credential_metadata(
        context=context, organization_id=organization_id,
        service_principal_id=service_principal_id,
    )


@router.post(
    "/organizations/{organization_id}/service-principals/"
    "{service_principal_id}/credentials",
    status_code=201,
)
def issue_service_credential(
    organization_id: UUID,
    service_principal_id: UUID,
    payload: IssueServiceCredentialRequest,
    response: Response,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    require_human_permission(
        session,
        authentication,
        organization_id,
        Permission.SERVICE_PRINCIPAL_MANAGE,
        lock=True,
    )
    try:
        ceiling = (
            frozenset(Permission(value) for value in payload.permission_ceiling)
            if payload.permission_ceiling is not None
            else None
        )
    except ValueError as exc:
        raise InvalidServiceCredentialError("unknown permission in credential ceiling") from exc
    credential, secret = _service(session).issue_credential(
        actor=authentication.actor,
        organization_id=organization_id,
        service_principal_id=service_principal_id,
        name=payload.name,
        permission_ceiling=ceiling,
        expires_at=payload.expires_at,
    )
    response.headers["Cache-Control"] = "no-store"
    return {
        "id": credential.id,
        "prefix": credential.prefix,
        "secret": secret,
        "permission_ceiling": list(credential.permission_ceiling)
        if credential.permission_ceiling is not None
        else None,
        "expires_at": credential.expires_at,
    }


@router.post(
    "/organizations/{organization_id}/service-principals/"
    "{service_principal_id}/credentials/{credential_id}/revoke",
    status_code=204,
)
def revoke_service_credential(
    organization_id: UUID,
    service_principal_id: UUID,
    credential_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    require_human_permission(
        session,
        authentication,
        organization_id,
        Permission.SERVICE_PRINCIPAL_MANAGE,
        lock=True,
    )
    _service(session).revoke_credential(
        actor=authentication.actor,
        organization_id=organization_id,
        service_principal_id=service_principal_id,
        credential_id=credential_id,
    )
    return Response(status_code=204)