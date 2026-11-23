from __future__ import annotations

import hmac
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from uuid import UUID, uuid4

from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.audit.ports import AuditRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.bootstrap.ports import PlatformRepository
from openvietgis_api.domains.tenancy.domain import (
    ActorType,
    MembershipRole,
    MembershipStatus,
    Organization,
    OrganizationMembership,
    OrganizationStatus,
)
from openvietgis_api.domains.tenancy.ports import TenancyRepository

_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


class BootstrapAuthorizationError(PermissionError):
    pass


class BootstrapAlreadyInitializedError(RuntimeError):
    pass


class InvalidBootstrapInputError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class BootstrapResult:
    organization_id: UUID
    owner_membership_id: UUID


class BootstrapService:
    def __init__(
        self,
        platform_repository: PlatformRepository,
        tenancy_repository: TenancyRepository,
        audit_repository: AuditRepository,
    ) -> None:
        self._platform_repository = platform_repository
        self._tenancy_repository = tenancy_repository
        self._audit_repository = audit_repository

    def initialize(
        self,
        *,
        actor: Actor,
        presented_authority: str,
        configured_authority: str,
        organization_name: str,
        organization_slug: str,
        now: datetime | None = None,
    ) -> BootstrapResult:
        if actor.actor_type != "human" or actor.actor_id is None:
            raise BootstrapAuthorizationError("authenticated human required")
        if (
            not presented_authority
            or not configured_authority
            or not hmac.compare_digest(
                presented_authority.encode(),
                configured_authority.encode(),
            )
        ):
            raise BootstrapAuthorizationError("invalid bootstrap authority")

        state = self._platform_repository.get_installation_state(for_update=True)
        if state.initialized_at is not None:
            raise BootstrapAlreadyInitializedError("installation already initialized")

        name = organization_name.strip()
        slug = organization_slug.strip().lower()
        if not name:
            raise InvalidBootstrapInputError("organization name is required")
        if not _SLUG_RE.fullmatch(slug):
            raise InvalidBootstrapInputError("invalid organization slug")

        now = now or datetime.now(UTC)
        organization = Organization(
            id=uuid4(),
            name=name,
            slug=slug,
            status=OrganizationStatus.ACTIVE,
            created_by_actor_type=ActorType.HUMAN,
            created_by_actor_id=actor.actor_id,
            created_at=now,
            updated_at=now,
            deleted_at=None,
        )
        membership = OrganizationMembership(
            id=uuid4(),
            organization_id=organization.id,
            user_id=actor.actor_id,
            role=MembershipRole.OWNER,
            status=MembershipStatus.ACTIVE,
            created_at=now,
            activated_at=now,
            suspended_at=None,
            revoked_at=None,
        )
        context = AuthorizationContext(actor, organization.id)
        self._tenancy_repository.create_organization_with_owner(
            organization,
            membership,
            context=context,
        )
        self._audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization.id,
                actor_type=ActorType.HUMAN,
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="installation.bootstrap.initialize",
                resource_type="organization",
                resource_id=organization.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={"owner_membership_id": str(membership.id)},
                created_at=now,
            )
        )
        self._platform_repository.mark_initialized(
            organization_id=organization.id,
            user_id=actor.actor_id,
            now=now,
        )
        return BootstrapResult(
            organization_id=organization.id,
            owner_membership_id=membership.id,
        )
