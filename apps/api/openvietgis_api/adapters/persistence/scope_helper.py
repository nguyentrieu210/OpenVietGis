"""Shared RLS scope binding & audit event helper for platform repositories (R006+)."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID

from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.rls import (
    bind_authenticated_actor,
    bind_authenticated_service_actor,
)
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.tenancy.domain import ActorType


def prepare_project_scope(
    session: Session,
    context: AuthorizationContext,
    *,
    permission: Permission,
    mutation: bool | None = None,
) -> None:
    """Re-bind authenticated actor if a new DB transaction started, then enforce RLS + authz."""
    if context.actor.actor_type == "human":
        bind_authenticated_actor(session, context.actor)
    elif context.actor.actor_type == "service_principal" and context.credential_id is not None:
        assert context.organization_id is not None
        bind_authenticated_service_actor(
            session,
            context.actor,
            organization_id=context.organization_id,
            credential_id=context.credential_id,
        )
    SqlSpatialRepository(session).prepare_scope(
        context=context,
        permission=permission,
        mutation=(permission != Permission.PROJECT_READ) if mutation is None else mutation,
    )


def append_project_audit(
    session: Session,
    context: AuthorizationContext,
    *,
    audit_id: UUID,
    action: str,
    resource_type: str,
    resource_id: UUID,
    metadata: dict[str, Any],
    created_at: datetime,
) -> None:
    SqlAuditRepository(session).append(
        AuditEvent(
            id=audit_id,
            organization_id=context.organization_id,
            actor_type=ActorType(context.actor.actor_type),
            actor_id=context.actor.actor_id,
            actor_display_snapshot=context.actor.display_name,
            action=action,
            resource_type=resource_type,
            resource_id=resource_id,
            outcome=AuditOutcome.SUCCESS,
            metadata=metadata,
            created_at=created_at,
        )
    )
