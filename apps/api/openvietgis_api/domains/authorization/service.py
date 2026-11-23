from __future__ import annotations

from dataclasses import dataclass, replace
from datetime import UTC, datetime
from uuid import UUID, uuid4

from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.audit.ports import AuditRepository
from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.domain import (
    ResourceGrant,
    ServiceCredential,
    ServicePrincipal,
    ServicePrincipalStatus,
)
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.ports import (
    EffectivePermissionProvider,
    ResourceGrantRepository,
    ServicePrincipalRepository,
)
from openvietgis_api.domains.authorization.roles import RESOURCE_ROLE_PERMISSIONS
from openvietgis_api.domains.tenancy.domain import (
    ActorType,
    ResourceRole,
    ResourceScopeKind,
)
from openvietgis_api.domains.tenancy.ports import TenancyRepository
from openvietgis_api.security.secrets import (
    digest_secret,
    generate_service_credential_secret,
    safe_prefix,
)


class AuthorizationDeniedError(PermissionError):
    pass


class TenantResourceNotFoundError(LookupError):
    pass


@dataclass(slots=True)
class AuthorizationService:
    permission_provider: EffectivePermissionProvider

    def effective_permissions(self, context: AuthorizationContext) -> frozenset[Permission]:
        return self.permission_provider.resolve(context)

    def is_allowed(self, context: AuthorizationContext, permission: Permission) -> bool:
        return permission in self.effective_permissions(context)

    def require(self, context: AuthorizationContext, permission: Permission) -> None:
        if not self.is_allowed(context, permission):
            raise AuthorizationDeniedError(f"missing permission: {permission.value}")


class InvalidGrantError(ValueError):
    pass


class GrantNotFoundError(LookupError):
    pass


class InvalidServiceCredentialError(ValueError):
    pass


def apply_permission_ceiling(
    granted: frozenset[Permission],
    ceiling: frozenset[Permission] | None,
) -> frozenset[Permission]:
    if ceiling is None:
        return granted
    if not ceiling <= granted:
        raise InvalidServiceCredentialError(
            "permission ceiling contains permission principal cannot receive"
        )
    return granted & ceiling


@dataclass(slots=True)
class GrantService:
    repository: ResourceGrantRepository
    tenancy_repository: TenancyRepository
    audit_repository: AuditRepository

    def create_membership_grant(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        membership_id: UUID,
        scope_kind: ResourceScopeKind,
        role: ResourceRole,
        workspace_id: UUID | None = None,
        project_id: UUID | None = None,
        now: datetime | None = None,
    ) -> ResourceGrant:
        context = AuthorizationContext(actor, organization_id)
        membership = self.tenancy_repository.get_membership(
            context=context,
            organization_id=organization_id,
            membership_id=membership_id,
        )
        if membership is None:
            raise InvalidGrantError("membership does not belong to organization")

        if scope_kind == ResourceScopeKind.ORGANIZATION:
            if workspace_id is not None or project_id is not None:
                raise InvalidGrantError("organization scope cannot include resource id")
        elif scope_kind == ResourceScopeKind.WORKSPACE:
            if workspace_id is None or project_id is not None:
                raise InvalidGrantError("workspace scope requires workspace_id only")
            if (
                self.tenancy_repository.get_workspace(
                    context=context,
                    organization_id=organization_id,
                    workspace_id=workspace_id,
                )
                is None
            ):
                raise InvalidGrantError("workspace does not belong to organization")
        elif scope_kind == ResourceScopeKind.PROJECT:
            if project_id is None or workspace_id is not None:
                raise InvalidGrantError("project scope requires project_id only")
            if (
                self.tenancy_repository.get_project(
                    context=context,
                    organization_id=organization_id,
                    project_id=project_id,
                )
                is None
            ):
                raise InvalidGrantError("project does not belong to organization")

        now = now or datetime.now(UTC)
        grant = ResourceGrant(
            id=uuid4(),
            organization_id=organization_id,
            subject_kind="membership",
            membership_id=membership_id,
            service_principal_id=None,
            scope_kind=scope_kind,
            workspace_id=workspace_id,
            project_id=project_id,
            role=role,
            created_at=now,
            created_by_actor_type=ActorType(actor.actor_type),
            created_by_actor_id=actor.actor_id,
            revoked_at=None,
        )
        self.repository.create(grant, context=context)
        self.audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=ActorType(actor.actor_type),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="authorization.grant.create",
                resource_type="resource_grant",
                resource_id=grant.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={
                    "scope_kind": scope_kind.value,
                    "role": role.value,
                    "membership_id": str(membership_id),
                },
                created_at=now,
            )
        )
        return grant

    def revoke(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        grant_id: UUID,
        now: datetime | None = None,
    ) -> ResourceGrant:
        context = AuthorizationContext(actor, organization_id)
        grant = self.repository.get(
            context=context,
            organization_id=organization_id,
            grant_id=grant_id,
            for_update=True,
        )
        if grant is None:
            raise GrantNotFoundError("grant not found")
        if grant.revoked_at is not None:
            return grant

        now = now or datetime.now(UTC)
        revoked = replace(grant, revoked_at=now)
        self.repository.update(revoked, context=context)
        self.audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=ActorType(actor.actor_type),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="authorization.grant.revoke",
                resource_type="resource_grant",
                resource_id=grant.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={},
                created_at=now,
            )
        )
        return revoked


class ServicePrincipalNotFoundError(LookupError):
    pass


class ServiceCredentialNotFoundError(LookupError):
    pass


class InvalidServicePrincipalError(ValueError):
    pass


@dataclass(slots=True)
class ServicePrincipalService:
    repository: ServicePrincipalRepository
    grant_repository: ResourceGrantRepository
    tenancy_repository: TenancyRepository
    audit_repository: AuditRepository

    def create_principal(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        name: str,
        description: str | None = None,
        now: datetime | None = None,
    ) -> ServicePrincipal:
        cleaned_name = name.strip()
        if not cleaned_name:
            raise InvalidServicePrincipalError("service principal name is required")
        now = now or datetime.now(UTC)
        context = AuthorizationContext(actor, organization_id)
        principal = ServicePrincipal(
            id=uuid4(),
            organization_id=organization_id,
            name=cleaned_name,
            description=description,
            status=ServicePrincipalStatus.ACTIVE,
            created_at=now,
            created_by_actor_type=ActorType(actor.actor_type),
            created_by_actor_id=actor.actor_id,
            revoked_at=None,
        )
        self.repository.create_principal(principal, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="service_principal.create",
            resource_type="service_principal",
            resource_id=principal.id,
            now=now,
        )
        return principal

    def revoke_principal(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        service_principal_id: UUID,
        now: datetime | None = None,
    ) -> ServicePrincipal:
        context = AuthorizationContext(actor, organization_id)
        principal = self.repository.get_principal(
            context=context,
            organization_id=organization_id,
            service_principal_id=service_principal_id,
            for_update=True,
        )
        if principal is None:
            raise ServicePrincipalNotFoundError("service principal not found")
        if principal.status == ServicePrincipalStatus.REVOKED:
            return principal
        now = now or datetime.now(UTC)
        revoked = replace(
            principal,
            status=ServicePrincipalStatus.REVOKED,
            revoked_at=now,
        )
        self.repository.update_principal(revoked, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="service_principal.revoke",
            resource_type="service_principal",
            resource_id=principal.id,
            now=now,
        )
        return revoked

    def create_service_grant(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        service_principal_id: UUID,
        scope_kind: ResourceScopeKind,
        role: ResourceRole,
        workspace_id: UUID | None = None,
        project_id: UUID | None = None,
        now: datetime | None = None,
    ) -> ResourceGrant:
        context = AuthorizationContext(actor, organization_id)
        principal = self.repository.get_principal(
            context=context,
            organization_id=organization_id,
            service_principal_id=service_principal_id,
        )
        if principal is None or principal.status != ServicePrincipalStatus.ACTIVE:
            raise InvalidGrantError("service principal does not belong to organization")
        self._validate_scope(
            context=context,
            organization_id=organization_id,
            scope_kind=scope_kind,
            workspace_id=workspace_id,
            project_id=project_id,
        )
        now = now or datetime.now(UTC)
        grant = ResourceGrant(
            id=uuid4(),
            organization_id=organization_id,
            subject_kind="service_principal",
            membership_id=None,
            service_principal_id=service_principal_id,
            scope_kind=scope_kind,
            workspace_id=workspace_id,
            project_id=project_id,
            role=role,
            created_at=now,
            created_by_actor_type=ActorType(actor.actor_type),
            created_by_actor_id=actor.actor_id,
            revoked_at=None,
        )
        self.grant_repository.create(grant, context=context)
        self.audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=ActorType(actor.actor_type),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action="authorization.grant.create",
                resource_type="resource_grant",
                resource_id=grant.id,
                outcome=AuditOutcome.SUCCESS,
                metadata={
                    "scope_kind": scope_kind.value,
                    "role": role.value,
                    "service_principal_id": str(service_principal_id),
                },
                created_at=now,
            )
        )
        return grant

    def issue_credential(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        service_principal_id: UUID,
        name: str,
        permission_ceiling: frozenset[Permission] | None,
        expires_at: datetime | None,
        now: datetime | None = None,
    ) -> tuple[ServiceCredential, str]:
        context = AuthorizationContext(actor, organization_id)
        principal = self.repository.get_principal(
            context=context,
            organization_id=organization_id,
            service_principal_id=service_principal_id,
            for_update=True,
        )
        if principal is None or principal.status != ServicePrincipalStatus.ACTIVE:
            raise ServicePrincipalNotFoundError("service principal not found")
        now = now or datetime.now(UTC)
        if expires_at is not None and expires_at <= now:
            raise InvalidServiceCredentialError("credential expiry must be in the future")

        granted: set[Permission] = set()
        for grant in self.grant_repository.list_for_organization(
            organization_id,
            context=context,
        ):
            if (
                grant.service_principal_id == service_principal_id
                and grant.subject_kind == "service_principal"
                and grant.revoked_at is None
            ):
                granted.update(RESOURCE_ROLE_PERMISSIONS[grant.role])
        apply_permission_ceiling(frozenset(granted), permission_ceiling)

        credential_id = uuid4()
        secret = generate_service_credential_secret(credential_id)
        ceiling_values = (
            tuple(sorted(permission.value for permission in permission_ceiling))
            if permission_ceiling is not None
            else None
        )
        credential = ServiceCredential(
            id=credential_id,
            organization_id=organization_id,
            service_principal_id=service_principal_id,
            name=name.strip(),
            prefix=safe_prefix(secret, visible_chars=20),
            secret_hash=digest_secret(secret),
            permission_ceiling=ceiling_values,
            created_at=now,
            expires_at=expires_at,
            last_used_at=None,
            revoked_at=None,
        )
        if not credential.name:
            raise InvalidServiceCredentialError("credential name is required")
        self.repository.create_credential(credential, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="service_credential.issue",
            resource_type="service_credential",
            resource_id=credential.id,
            now=now,
            metadata={"service_principal_id": str(service_principal_id)},
        )
        return credential, secret

    def revoke_credential(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        service_principal_id: UUID,
        credential_id: UUID,
        now: datetime | None = None,
    ) -> ServiceCredential:
        context = AuthorizationContext(actor, organization_id)
        credential = self.repository.get_credential(
            context=context,
            organization_id=organization_id,
            service_principal_id=service_principal_id,
            credential_id=credential_id,
            for_update=True,
        )
        if credential is None:
            raise ServiceCredentialNotFoundError("service credential not found")
        if credential.revoked_at is not None:
            return credential
        now = now or datetime.now(UTC)
        revoked = replace(credential, revoked_at=now)
        self.repository.mark_credential_revoked(revoked, context=context)
        self._audit(
            actor=actor,
            organization_id=organization_id,
            action="service_credential.revoke",
            resource_type="service_credential",
            resource_id=credential.id,
            now=now,
            metadata={"service_principal_id": str(service_principal_id)},
        )
        return revoked

    def _validate_scope(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        scope_kind: ResourceScopeKind,
        workspace_id: UUID | None,
        project_id: UUID | None,
    ) -> None:
        if scope_kind == ResourceScopeKind.ORGANIZATION:
            if workspace_id is not None or project_id is not None:
                raise InvalidGrantError("organization scope cannot include resource id")
            return
        if scope_kind == ResourceScopeKind.WORKSPACE:
            if workspace_id is None or project_id is not None:
                raise InvalidGrantError("workspace scope requires workspace_id only")
            if (
                self.tenancy_repository.get_workspace(
                    context=context,
                    organization_id=organization_id,
                    workspace_id=workspace_id,
                )
                is None
            ):
                raise InvalidGrantError("workspace does not belong to organization")
            return
        if project_id is None or workspace_id is not None:
            raise InvalidGrantError("project scope requires project_id only")
        if (
            self.tenancy_repository.get_project(
                context=context,
                organization_id=organization_id,
                project_id=project_id,
            )
            is None
        ):
            raise InvalidGrantError("project does not belong to organization")

    def _audit(
        self,
        *,
        actor: Actor,
        organization_id: UUID,
        action: str,
        resource_type: str,
        resource_id: UUID,
        now: datetime,
        metadata: dict[str, str] | None = None,
    ) -> None:
        self.audit_repository.append(
            AuditEvent(
                id=uuid4(),
                organization_id=organization_id,
                actor_type=ActorType(actor.actor_type),
                actor_id=actor.actor_id,
                actor_display_snapshot=actor.display_name,
                action=action,
                resource_type=resource_type,
                resource_id=resource_id,
                outcome=AuditOutcome.SUCCESS,
                metadata=metadata or {},
                created_at=now,
            )
        )
