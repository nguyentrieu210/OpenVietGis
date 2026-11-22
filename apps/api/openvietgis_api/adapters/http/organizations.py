from __future__ import annotations

from datetime import UTC, datetime
from typing import Annotated, Self
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import JSONResponse
from pydantic import BaseModel, ConfigDict, Field, model_validator
from sqlalchemy import text
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.http.tenant_access import (
    human_actor_id,
    human_tenant,
    require_human_permission,
)
from openvietgis_api.adapters.http.tenant_serialization import (
    serialize_membership,
    serialize_organization,
)
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.platform_repository import SqlPlatformRepository
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.roles import ORGANIZATION_ROLE_PERMISSIONS
from openvietgis_api.domains.authorization.service import (
    AuthorizationDeniedError,
    AuthorizationService,
    TenantResourceNotFoundError,
)
from openvietgis_api.domains.tenancy.domain import (
    ActorType,
    MembershipRole,
    MembershipStatus,
    OrganizationStatus,
)
from openvietgis_api.domains.tenancy.service import (
    InstallationNotInitializedError,
    InvalidOrganizationInputError,
    MembershipNotFoundError,
    TenancyService,
)

router = APIRouter(tags=["organizations"])


class CreateOrganizationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str = Field(min_length=1, max_length=200)
    slug: str = Field(min_length=1, max_length=120)


class UpdateOrganizationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str | None = Field(default=None, min_length=1, max_length=200)
    status: OrganizationStatus | None = None

    @model_validator(mode="after")
    def require_changes(self) -> Self:
        if self.name is None and self.status is None:
            raise ValueError("name or status is required")
        return self


class UpdateMembershipRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    role: MembershipRole | None = None
    status: MembershipStatus | None = None

    @model_validator(mode="after")
    def require_changes(self) -> Self:
        if self.role is None and self.status is None:
            raise ValueError("role or status is required")
        return self


class TransferOwnershipRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    target_membership_id: UUID


def _service(session: Session) -> TenancyService:
    return TenancyService(
        SqlTenancyRepository(session),
        SqlPlatformRepository(session),
        SqlAuditRepository(session),
    )


@router.post("/organizations", status_code=201)
def create_organization(
    payload: CreateOrganizationRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    human_actor_id(authentication)
    try:
        organization, membership = _service(session).create_organization(
            actor=authentication.actor,
            name=payload.name,
            slug=payload.slug,
        )
    except InstallationNotInitializedError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    except InvalidOrganizationInputError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except IntegrityError as exc:
        raise HTTPException(status_code=409, detail="organization slug already exists") from exc

    return {
        "organization": serialize_organization(organization),
        "owner_membership": serialize_membership(membership),
    }


@router.get("/organizations/{organization_id}")
def get_organization(
    organization_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    organization, membership = human_tenant(
        session, authentication, organization_id, allow_inactive_owner=True
    )
    if Permission.ORGANIZATION_READ not in ORGANIZATION_ROLE_PERMISSIONS[membership.role]:
        raise AuthorizationDeniedError("missing permission: organization.read")
    return serialize_organization(organization)


@router.patch("/organizations/{organization_id}")
def update_organization(
    organization_id: UUID,
    payload: UpdateOrganizationRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    organization, membership = human_tenant(
        session, authentication, organization_id, lock=True, allow_inactive_owner=True
    )
    permissions = ORGANIZATION_ROLE_PERMISSIONS[membership.role]
    if payload.name is not None and (
        organization.status != OrganizationStatus.ACTIVE
        or Permission.ORGANIZATION_MANAGE_SETTINGS not in permissions
    ):
        raise AuthorizationDeniedError("organization settings update is not allowed")
    if payload.status is not None:
        required = (
            Permission.ORGANIZATION_DELETE
            if payload.status == OrganizationStatus.DELETED
            else Permission.ORGANIZATION_LIFECYCLE
        )
        if required not in permissions:
            raise AuthorizationDeniedError(f"missing permission: {required.value}")
    updated = _service(session).update_organization(
        actor=authentication.actor,
        organization_id=organization_id,
        name=payload.name,
        status=payload.status,
    )
    return serialize_organization(updated)


@router.get("/organizations/{organization_id}/members")
def list_organization_members(
    organization_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> list[dict[str, object]]:
    require_human_permission(
        session, authentication, organization_id, Permission.ORGANIZATION_MANAGE_MEMBERS
    )
    items = SqlTenancyRepository(session).list_memberships(
        organization_id, context=context_for(authentication, organization_id=organization_id)
    )
    profiles = {
        row["id"]: dict(row)
        for row in session.execute(
            text("""SELECT u.id,u.display_name,u.email_snapshot
        FROM identity.users u JOIN org.organization_memberships m ON m.user_id=u.id
        WHERE m.organization_id=:org"""),
            {"org": organization_id},
        ).mappings()
    }
    return [
        serialize_membership(item)
        | {
            "display_name": profiles.get(item.user_id, {}).get("display_name"),
            "email_snapshot": profiles.get(item.user_id, {}).get("email_snapshot"),
        }
        for item in items
    ]


@router.patch("/organizations/{organization_id}/members/{membership_id}")
def update_organization_membership(
    organization_id: UUID,
    membership_id: UUID,
    payload: UpdateMembershipRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    _, acting_member = human_tenant(session, authentication, organization_id, lock=True)
    target = SqlTenancyRepository(session).get_membership(
        context=context_for(authentication, organization_id=organization_id),
        organization_id=organization_id,
        membership_id=membership_id,
    )
    if target is None:
        raise MembershipNotFoundError("membership not found")
    require_human_permission(
        session, authentication, organization_id, Permission.ORGANIZATION_MANAGE_MEMBERS
    )
    if (
        target.role == MembershipRole.OWNER or payload.role == MembershipRole.OWNER
    ) and Permission.ORGANIZATION_OWNER_TRANSFER not in ORGANIZATION_ROLE_PERMISSIONS[
        acting_member.role
    ]:
        raise AuthorizationDeniedError("Owner authority is required for an Owner change")
    updated = _service(session).update_membership(
        actor=authentication.actor,
        organization_id=organization_id,
        membership_id=membership_id,
        role=payload.role,
        status=payload.status,
    )
    return serialize_membership(updated)


@router.post("/organizations/{organization_id}/ownership-transfer", response_model=None)
def transfer_organization_ownership(
    organization_id: UUID,
    payload: TransferOwnershipRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object] | JSONResponse:
    _, member = human_tenant(session, authentication, organization_id, lock=True)
    context = context_for(authentication, organization_id=organization_id)
    target = SqlTenancyRepository(session).get_membership(
        organization_id=organization_id,
        membership_id=payload.target_membership_id,
        context=context,
    )
    if target is None:
        raise MembershipNotFoundError("membership not found")
    if Permission.ORGANIZATION_OWNER_TRANSFER not in ORGANIZATION_ROLE_PERMISSIONS[member.role]:
        # Known tenant, known actor, no target identifiers or request secrets in the denial.
        SqlAuditRepository(session).append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=ActorType.HUMAN,
                actor_id=authentication.actor.actor_id,
                actor_display_snapshot=authentication.actor.display_name,
                action="organization.owner.transfer",
                resource_type="organization",
                resource_id=None,
                outcome=AuditOutcome.DENIED,
                metadata={"reason": "insufficient_owner_authority"},
                created_at=datetime.now(UTC),
            )
        )
        # No mutation has occurred; normal response allows the denial audit to commit.
        return JSONResponse(status_code=403, content={"detail": "Owner authority required"})
    previous, new = _service(session).transfer_ownership(
        actor=authentication.actor,
        organization_id=organization_id,
        target_membership_id=payload.target_membership_id,
    )
    return {
        "previous_owner": serialize_membership(previous),
        "new_owner": serialize_membership(new),
    }


@router.get("/organizations/{organization_id}/effective-permissions")
def get_effective_permissions(
    organization_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    workspace_id: UUID | None = None,
    project_id: UUID | None = None,
) -> dict[str, list[str]]:
    human_tenant(session, authentication, organization_id)
    repository = SqlTenancyRepository(session)
    if (
        workspace_id is not None
        and repository.get_workspace(
            context=context_for(authentication, organization_id=organization_id),
            organization_id=organization_id,
            workspace_id=workspace_id,
        )
        is None
    ):
        raise TenantResourceNotFoundError("workspace not found")
    if project_id is not None:
        project = repository.get_project(
            context=context_for(authentication, organization_id=organization_id),
            organization_id=organization_id,
            project_id=project_id,
        )
        if project is None or (workspace_id is not None and project.workspace_id != workspace_id):
            raise TenantResourceNotFoundError("project not found")
    context = context_for(
        authentication,
        organization_id=organization_id,
        workspace_id=workspace_id,
        project_id=project_id,
    )
    permissions = AuthorizationService(
        SqlEffectivePermissionProvider(session)
    ).effective_permissions(context)
    return {"permissions": sorted(permission.value for permission in permissions)}
