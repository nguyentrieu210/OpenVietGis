from __future__ import annotations

from typing import Protocol
from uuid import UUID

from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.domain import (
    ResourceGrant,
    ServiceCredential,
    ServicePrincipal,
)
from openvietgis_api.domains.authorization.permissions import Permission


class EffectivePermissionProvider(Protocol):
    def resolve(self, context: AuthorizationContext) -> frozenset[Permission]: ...


class ResourceGrantRepository(Protocol):
    def list_for_organization(
        self, organization_id: UUID, *, context: AuthorizationContext
    ) -> list[ResourceGrant]: ...

    def create(self, grant: ResourceGrant, *, context: AuthorizationContext) -> None: ...

    def get(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        grant_id: UUID,
        for_update: bool = False,
    ) -> ResourceGrant | None: ...

    def update(self, grant: ResourceGrant, *, context: AuthorizationContext) -> None: ...


class ServicePrincipalRepository(Protocol):
    def list_principals(
        self,
        organization_id: UUID,
        *,
        context: AuthorizationContext,
    ) -> list[ServicePrincipal]: ...

    def get_principal(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        service_principal_id: UUID,
        for_update: bool = False,
    ) -> ServicePrincipal | None: ...

    def create_principal(
        self,
        principal: ServicePrincipal,
        *,
        context: AuthorizationContext,
    ) -> None: ...

    def update_principal(
        self,
        principal: ServicePrincipal,
        *,
        context: AuthorizationContext,
    ) -> None: ...

    def create_credential(
        self,
        credential: ServiceCredential,
        *,
        context: AuthorizationContext,
    ) -> None: ...

    def get_credential(
        self,
        *,
        context: AuthorizationContext,
        organization_id: UUID,
        service_principal_id: UUID,
        credential_id: UUID,
        for_update: bool = False,
    ) -> ServiceCredential | None: ...

    def mark_credential_revoked(
        self,
        credential: ServiceCredential,
        *,
        context: AuthorizationContext,
    ) -> None: ...

    def record_credential_use(
        self,
        credential: ServiceCredential,
        *,
        context: AuthorizationContext,
    ) -> bool: ...

    def get_credential_for_authentication(
        self,
        *,
        organization_id: UUID,
        credential_id: UUID,
    ) -> tuple[ServicePrincipal, ServiceCredential] | None: ...
