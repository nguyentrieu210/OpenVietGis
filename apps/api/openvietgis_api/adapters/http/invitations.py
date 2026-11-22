from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import Annotated, Literal
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Response
from fastapi.responses import JSONResponse
from pydantic import AwareDatetime, BaseModel, ConfigDict, Field
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.invitation_mail import DeliveryStatus, deliver_invitation
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
    serialize_grant,
    serialize_membership,
)
from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlResourceGrantRepository,
)
from openvietgis_api.adapters.persistence.identity_repository import SqlIdentityRepository
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.config import Settings, get_settings
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.roles import ORGANIZATION_ROLE_PERMISSIONS
from openvietgis_api.domains.authorization.service import AuthorizationDeniedError
from openvietgis_api.domains.tenancy.domain import (
    InvitationTargetKind,
    MembershipRole,
    ResourceRole,
    ResourceScopeKind,
)
from openvietgis_api.domains.tenancy.service import (
    ExpiredInvitationError,
    InvalidInvitationError,
    InvitationNotFoundError,
    InvitationService,
    InvitationTargetMismatchError,
)

router = APIRouter(tags=["invitations"])


class EmailTarget(BaseModel):
    model_config = ConfigDict(extra="forbid")

    kind: Literal["email"]
    email: str = Field(min_length=3, max_length=254, pattern=r"^[^\s@]+@[^\s@]+$")


class UserTarget(BaseModel):
    model_config = ConfigDict(extra="forbid")

    kind: Literal["user"]
    user_id: UUID


class ExternalIdentityTarget(BaseModel):
    model_config = ConfigDict(extra="forbid")

    kind: Literal["external_identity"]
    issuer: str = Field(min_length=1, max_length=2048)
    subject: str = Field(min_length=1, max_length=1024)


class InvitationResourceGrant(BaseModel):
    model_config = ConfigDict(extra="forbid")

    scope_kind: ResourceScopeKind
    workspace_id: UUID | None = None
    project_id: UUID | None = None
    role: ResourceRole


class CreateInvitationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    target: Annotated[
        EmailTarget | UserTarget | ExternalIdentityTarget, Field(discriminator="kind")
    ]
    organization_role: MembershipRole
    resource_grant: InvitationResourceGrant | None = None
    expires_at: AwareDatetime


class EmailInvitationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    email: str = Field(min_length=3, max_length=254, pattern=r"^[^\s@]+@[^\s@]+$")
    organization_role: Literal["member", "guest", "admin"] = "member"
    resource_grant: InvitationResourceGrant | None = None


class EmailInvitationIssued(BaseModel):
    id: UUID
    organization_id: UUID
    status: Literal["pending"]
    delivery: DeliveryStatus


class AcceptInvitationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    organization_id: UUID
    invitation_id: UUID
    secret: str = Field(min_length=32, max_length=512, repr=False)


def _service(session: Session) -> InvitationService:
    return InvitationService(
        SqlTenancyRepository(session),
        SqlIdentityRepository(session),
        SqlResourceGrantRepository(session),
        SqlAuditRepository(session),
    )


@router.get("/organizations/{organization_id}/invitations")
def list_invitations(
    organization_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> list[dict[str, object]]:
    require_human_permission(session, authentication, organization_id, Permission.INVITATION_MANAGE)
    rows = session.execute(text("""
        SELECT id, target_email_normalized AS email, organization_role,
            resource_project_id AS project_id, resource_role,
            CASE WHEN status = 'pending' AND expires_at <= now() THEN 'expired' ELSE status END AS status,
            expires_at, created_at
        FROM org.invitations WHERE organization_id = :org AND target_kind = 'email'
        ORDER BY created_at DESC LIMIT 100
    """), {"org": organization_id}).mappings()
    return [dict(row) for row in rows]


@router.post("/organizations/{organization_id}/invitations", status_code=201)
def create_invitation(
    organization_id: UUID,
    payload: CreateInvitationRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    member = require_human_permission(
        session, authentication, organization_id, Permission.INVITATION_MANAGE, lock=True
    )
    if (
        payload.organization_role == MembershipRole.OWNER
        and Permission.ORGANIZATION_OWNER_TRANSFER not in ORGANIZATION_ROLE_PERMISSIONS[member.role]
    ):
        raise AuthorizationDeniedError("Owner authority is required to invite an Owner")
    target = payload.target
    resource = payload.resource_grant
    issued = _service(session).issue(
        actor=authentication.actor,
        organization_id=organization_id,
        target_kind=InvitationTargetKind(target.kind),
        organization_role=payload.organization_role,
        expires_at=payload.expires_at,
        target_email=target.email if isinstance(target, EmailTarget) else None,
        target_user_id=target.user_id if isinstance(target, UserTarget) else None,
        target_issuer=target.issuer if isinstance(target, ExternalIdentityTarget) else None,
        target_subject=target.subject if isinstance(target, ExternalIdentityTarget) else None,
        resource_scope_kind=resource.scope_kind if resource else None,
        resource_workspace_id=resource.workspace_id if resource else None,
        resource_project_id=resource.project_id if resource else None,
        resource_role=resource.role if resource else None,
    )
    return {
        "id": issued.invitation.id,
        "organization_id": issued.invitation.organization_id,
        "status": issued.invitation.status.value,
        "secret": issued.secret,
    }


@router.post(
    "/organizations/{organization_id}/invitations/email",
    status_code=201,
    response_model=EmailInvitationIssued,
)
def email_invitation(
    organization_id: UUID,
    payload: EmailInvitationRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    settings: Annotated[Settings, Depends(get_settings)],
) -> EmailInvitationIssued:
    require_human_permission(
        session, authentication, organization_id, Permission.INVITATION_MANAGE, lock=True
    )
    if not settings.resend_api_key or not settings.invitation_from_email:
        raise HTTPException(status_code=503, detail="invitation email is not configured")
    org = SqlTenancyRepository(session).get_organization(
        organization_id, context=context_for(authentication, organization_id=organization_id)
    )
    if org is None:
        raise HTTPException(status_code=404, detail="organization not found")
    issued = create_invitation(
        organization_id,
        CreateInvitationRequest(
            target=EmailTarget(kind="email", email=payload.email.strip().lower()),
            organization_role=MembershipRole(payload.organization_role),
            resource_grant=payload.resource_grant,
            expires_at=datetime.now(UTC) + timedelta(hours=48),
        ),
        authentication,
        session,
    )
    invitation_id = UUID(str(issued["id"]))
    # Never send a link to a database transaction that has not committed.
    # The raw secret is held only in this request and the email.
    session.commit()
    delivery = deliver_invitation(
        settings,
        recipient=payload.email.strip().lower(),
        organization_name=org.name,
        organization_id=organization_id,
        invitation_id=invitation_id,
        secret=str(issued["secret"]),
    )
    # On a provider rejection/uncertain delivery, the UI can revoke the exact
    # pending invitation before issuing another. Do not claim inbox delivery.
    return EmailInvitationIssued(
        id=invitation_id, organization_id=organization_id, status="pending", delivery=delivery
    )


@router.post("/organizations/{organization_id}/invitations/{invitation_id}/revoke", status_code=204)
def revoke_invitation(
    organization_id: UUID,
    invitation_id: UUID,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    human_tenant(session, authentication, organization_id, lock=True)
    context = context_for(authentication, organization_id=organization_id)
    if (
        SqlTenancyRepository(session).get_invitation(
            organization_id=organization_id, invitation_id=invitation_id, context=context
        )
        is None
    ):
        raise InvitationNotFoundError("invitation not found")
    require_human_permission(session, authentication, organization_id, Permission.INVITATION_MANAGE)
    _service(session).revoke(
        actor=authentication.actor,
        organization_id=organization_id,
        invitation_id=invitation_id,
    )
    return Response(status_code=204)


@router.post("/invitations/check")
def check_invitation(
    payload: AcceptInvitationRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, str]:
    return {"status": _service(session).check_recipient(
        actor=authentication.actor, user_id=human_actor_id(authentication),
        organization_id=payload.organization_id, invitation_id=payload.invitation_id,
        secret=payload.secret, identity_link_id=authentication.identity_link_id,
    )}


@router.post("/invitations/accept", response_model=None)
def accept_invitation(
    payload: AcceptInvitationRequest,
    authentication: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object] | JSONResponse:
    user_id = human_actor_id(authentication)
    # The invitation service owns the exact-ID pre-membership context and verifies
    # the one-time secret and target identity before granting membership.
    try:
        membership, grant = _service(session).accept(
            actor=authentication.actor,
            user_id=user_id,
            organization_id=payload.organization_id,
            invitation_id=payload.invitation_id,
            secret=payload.secret,
            identity_link_id=authentication.identity_link_id,
        )
    except ExpiredInvitationError:
        # Only verified expired invitations reach this path; commit terminal state.
        return JSONResponse(status_code=400, content={"detail": "invalid invitation"})
    except (
        InvitationNotFoundError,
        InvalidInvitationError,
        InvitationTargetMismatchError,
    ):
        # The same response covers absent, wrong-tenant, expired, revoked, reused and
        # wrong-recipient invitations. Do not expose recipient or secret information.
        raise HTTPException(status_code=400, detail="invalid invitation") from None
    return {
        "membership": serialize_membership(membership),
        "grant": serialize_grant(grant) if grant is not None else None,
    }
