from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum
from uuid import UUID

from openvietgis_api.domains.tenancy.domain import ActorType, ResourceRole, ResourceScopeKind


@dataclass(frozen=True, slots=True)
class ResourceGrant:
    id: UUID
    organization_id: UUID
    subject_kind: str
    membership_id: UUID | None
    service_principal_id: UUID | None
    scope_kind: ResourceScopeKind
    workspace_id: UUID | None
    project_id: UUID | None
    role: ResourceRole
    created_at: datetime
    created_by_actor_type: ActorType
    created_by_actor_id: UUID | None
    revoked_at: datetime | None


class ServicePrincipalStatus(StrEnum):
    ACTIVE = "active"
    REVOKED = "revoked"


@dataclass(frozen=True, slots=True)
class ServicePrincipal:
    id: UUID
    organization_id: UUID
    name: str
    description: str | None
    status: ServicePrincipalStatus
    created_at: datetime
    created_by_actor_type: ActorType
    created_by_actor_id: UUID | None
    revoked_at: datetime | None


@dataclass(frozen=True, slots=True)
class ServiceCredential:
    id: UUID
    organization_id: UUID
    service_principal_id: UUID
    name: str
    prefix: str
    secret_hash: bytes
    permission_ceiling: tuple[str, ...] | None
    created_at: datetime
    expires_at: datetime | None
    last_used_at: datetime | None
    revoked_at: datetime | None


def service_credential_is_usable(
    credential: ServiceCredential,
    now: datetime,
) -> bool:
    return credential.revoked_at is None and (
        credential.expires_at is None or credential.expires_at > now
    )
