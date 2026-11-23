"""HTTP authorization for human-only organization administration."""

from uuid import UUID

from fastapi import HTTPException
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for, require_permission
from openvietgis_api.adapters.http.security import AuthenticatedRequest
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import (
    AuthorizationDeniedError,
    TenantResourceNotFoundError,
)
from openvietgis_api.domains.tenancy.domain import (
    MembershipRole,
    Organization,
    OrganizationMembership,
    OrganizationStatus,
)


def human_actor_id(authentication: AuthenticatedRequest) -> UUID:
    actor = authentication.actor
    if actor.actor_type != "human" or actor.actor_id is None:
        raise HTTPException(status_code=401, detail="human session required")
    return actor.actor_id


def human_tenant(
    session: Session,
    authentication: AuthenticatedRequest,
    organization_id: UUID,
    *,
    lock: bool = False,
    allow_inactive_owner: bool = False,
) -> tuple[Organization, OrganizationMembership]:
    """Resolve current membership before accessing any organization-owned resource."""
    repository = SqlTenancyRepository(session)
    user_id = human_actor_id(authentication)
    context = context_for(authentication, organization_id=organization_id)
    membership = repository.get_active_membership(
        context=context, organization_id=organization_id, user_id=user_id
    )
    if membership is None:
        raise TenantResourceNotFoundError("organization not found")
    organization = repository.get_organization(organization_id, for_update=lock, context=context)
    if organization is None:
        raise TenantResourceNotFoundError("organization not found")
    if lock:
        # Revalidate after the organization lock to avoid racing member removal/role changes.
        membership = repository.get_active_membership(
            context=context, organization_id=organization_id, user_id=user_id
        )
        if membership is None:
            raise TenantResourceNotFoundError("organization not found")
    if organization.status != OrganizationStatus.ACTIVE and not (
        allow_inactive_owner and membership.role == MembershipRole.OWNER
    ):
        raise AuthorizationDeniedError("organization is not active")
    return organization, membership


def require_human_permission(
    session: Session,
    authentication: AuthenticatedRequest,
    organization_id: UUID,
    permission: Permission,
    *,
    lock: bool = False,
) -> OrganizationMembership:
    _, membership = human_tenant(session, authentication, organization_id, lock=lock)
    require_permission(
        session, context_for(authentication, organization_id=organization_id), permission
    )
    return membership


def require_tenant_access(
    session: Session,
    authentication: AuthenticatedRequest,
    organization_id: UUID,
    *,
    lock: bool = False,
) -> Organization:
    """Resolve an active tenant for either a human session or verified service credential."""
    if authentication.actor.actor_type == "human":
        organization, _ = human_tenant(
            session,
            authentication,
            organization_id,
            lock=lock,
        )
        return organization
    if authentication.actor.actor_type != "service_principal":
        raise HTTPException(status_code=401, detail="authentication required")
    if authentication.organization_id != organization_id or authentication.credential_id is None:
        raise TenantResourceNotFoundError("organization not found")
    context = context_for(authentication, organization_id=organization_id)
    service_organization = SqlTenancyRepository(session).get_organization(
        organization_id,
        context=context,
        for_update=lock,
    )
    if service_organization is None:
        raise TenantResourceNotFoundError("organization not found")
    if service_organization.status != OrganizationStatus.ACTIVE:
        raise AuthorizationDeniedError("organization is not active")
    return service_organization
