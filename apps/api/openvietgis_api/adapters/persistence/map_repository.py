"""Map persistence uses the existing verified Project proof in one request transaction."""

from __future__ import annotations

import json
from typing import Any
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.rls import require_project_context
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.domains.audit.domain import AuditEvent, AuditOutcome
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import TenantResourceNotFoundError
from openvietgis_api.domains.maps.domain import (
    Configuration,
    LayerReference,
    MapOperation,
    MapRevision,
    MapState,
    Style,
    Viewpoint,
)
from openvietgis_api.domains.maps.ports import (
    MapReferenceUnavailableError,
    MapRevisionConflictError,
)
from openvietgis_api.domains.spatial.domain import Lifecycle
from openvietgis_api.domains.spatial.service import source_render_supported


def _configuration(value: dict[str, Any]) -> Configuration:
    viewpoint = value["default_viewpoint"]
    return Configuration(
        tuple(
            LayerReference(
                UUID(item["layer_id"]), item["visible"], Style(**item["style"]), item.get("group")
            )
            for item in value["layers"]
        ),
        value["basemap"],
        Viewpoint(**viewpoint) if viewpoint is not None else None,
    )


def _operation(row: RowMapping) -> MapOperation:
    return MapOperation(
        row["operation_id"],
        row["map_id"],
        row["actor_type"],
        row["actor_id"],
        row["request_hash"],
        row["committed_revision"],
        row["created_at"],
    )


def _revision(row: RowMapping) -> MapRevision:
    return MapRevision(
        row["map_id"],
        row["operation_id"],
        row["configuration_revision"],
        row["prior_revision"],
        row["actor_type"],
        row["actor_id"],
        row["actor_display_snapshot"],
        row["created_at"],
        _configuration(row["before_configuration"]) if row["before_configuration"] else None,
        _configuration(row["after_configuration"]),
        row["audit_event_id"],
    )


class SqlMapRepository:
    def __init__(self, session: Session) -> None:
        self._session = session
        self._spatial = SqlSpatialRepository(session)

    def prepare_scope(
        self,
        *,
        context: AuthorizationContext,
        permission: Permission,
        mutation: bool,
    ) -> None:
        # Locks Organization then Project and verifies current grants/credential
        # ceiling, lifecycle and transaction/savepoint-bound RLS proof.
        self._spatial.prepare_scope(context=context, permission=permission, mutation=mutation)

    def _scope(
        self,
        context: AuthorizationContext,
        permission: Permission | None = None,
    ) -> dict[str, UUID]:
        require_project_context(self._session, context, permission=permission)
        if context.project_id is None:
            raise PermissionError("explicit Project required")
        return {"organization": context.organization_id, "project": context.project_id}

    def get_map(self, *, context: AuthorizationContext, lock: bool = False) -> MapState | None:
        scope = self._scope(context, Permission.PROJECT_EDIT if lock else None)
        row = (
            self._session.execute(
                text(
                    "SELECT * FROM maps.project_maps WHERE organization_id=:organization "
                    "AND project_id=:project" + (" FOR UPDATE" if lock else "")
                ),
                scope,
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            return None
        return MapState(
            row["id"],
            row["organization_id"],
            row["project_id"],
            row["configuration_revision"],
            _configuration(row["configuration"]),
            row["updated_at"],
        )

    def get_operation(
        self,
        *,
        context: AuthorizationContext,
        operation_id: UUID,
    ) -> MapOperation | None:
        scope = self._scope(context, Permission.PROJECT_EDIT)
        row = (
            self._session.execute(
                text(
                    "SELECT * FROM maps.map_operations WHERE organization_id=:organization "
                    "AND project_id=:project AND operation_id=:operation"
                ),
                scope | {"operation": operation_id},
            )
            .mappings()
            .one_or_none()
        )
        return _operation(row) if row is not None else None

    def validate_references(
        self,
        *,
        context: AuthorizationContext,
        configuration: Configuration,
    ) -> None:
        self._scope(context, Permission.PROJECT_EDIT)
        # The Project exclusive lock serializes its material changes. Source
        # locks still precede Layer locks, matching the catalog lock order.
        for reference in sorted(configuration.layers, key=lambda item: item.layer_id):
            layer = self._spatial.get_layer(context=context, layer_id=reference.layer_id)
            if layer is None or layer.lifecycle != Lifecycle.ACTIVE:
                raise TenantResourceNotFoundError("resource not found")
            source = self._spatial.get_source(context=context, source_id=layer.source_id, lock=True)
            locked_layer = self._spatial.get_layer(context=context, layer_id=layer.id, lock=True)
            if locked_layer != layer:
                raise MapReferenceUnavailableError("map Layer is unavailable")
            if (
                source is None
                or source.lifecycle != Lifecycle.ACTIVE
                or not source.readable
                or not source_render_supported(source)
            ):
                raise MapReferenceUnavailableError("map Layer is unavailable")

    def commit_save(
        self,
        *,
        context: AuthorizationContext,
        state: MapState,
        operation: MapOperation,
        revision: MapRevision,
        audit: AuditEvent,
        expected_revision: int,
    ) -> None:
        scope = self._scope(context, Permission.PROJECT_EDIT)
        if (
            (state.organization_id, state.project_id)
            != (context.organization_id, context.project_id)
            or state.configuration_revision != expected_revision + 1
            or (
                operation.map_id,
                operation.committed_revision,
                operation.actor_type,
                operation.actor_id,
            )
            != (
                state.map_id,
                state.configuration_revision,
                context.actor.actor_type,
                context.actor.actor_id,
            )
            or (
                revision.map_id,
                revision.operation_id,
                revision.configuration_revision,
                revision.prior_revision,
                revision.actor_type,
                revision.actor_id,
                revision.actor_display_snapshot,
                revision.after_configuration,
            )
            != (
                state.map_id,
                operation.operation_id,
                state.configuration_revision,
                expected_revision,
                context.actor.actor_type,
                context.actor.actor_id,
                context.actor.display_name,
                state.configuration,
            )
            or (
                audit.id,
                audit.organization_id,
                audit.actor_type.value,
                audit.actor_id,
                audit.actor_display_snapshot,
                audit.action,
                audit.resource_type,
                audit.resource_id,
                audit.outcome,
                audit.created_at,
            )
            != (
                revision.audit_event_id,
                context.organization_id,
                context.actor.actor_type,
                context.actor.actor_id,
                context.actor.display_name,
                "map.configuration.saved",
                "project_map",
                state.map_id,
                AuditOutcome.SUCCESS,
                revision.created_at,
            )
            or audit.metadata
            != {
                "project_id": str(context.project_id),
                "operation_id": str(operation.operation_id),
                "configuration_revision": state.configuration_revision,
            }
            or state.updated_at != revision.created_at
            or operation.created_at != revision.created_at
        ):
            raise PermissionError("map mutation and evidence do not match verified scope")
        prior = self.get_map(context=context, lock=True)
        if (
            prior.configuration_revision if prior else 0
        ) != expected_revision or revision.before_configuration != (
            prior.configuration if prior else None
        ):
            raise MapRevisionConflictError("configuration revision changed")
        params = scope | {
            "map": state.map_id,
            "revision": state.configuration_revision,
            "configuration": json.dumps(state.configuration.to_wire(), allow_nan=False),
            "created": state.updated_at,
        }
        if expected_revision == 0:
            self._session.execute(
                text("""
                INSERT INTO maps.project_maps(
                    id,organization_id,project_id,configuration_revision,configuration,updated_at)
                VALUES(:map,:organization,:project,:revision,CAST(:configuration AS jsonb),:created)
            """),
                params,
            )
        else:
            changed = self._session.execute(
                text("""
                UPDATE maps.project_maps SET configuration_revision=:revision,
                    configuration=CAST(:configuration AS jsonb),updated_at=:created
                WHERE organization_id=:organization AND project_id=:project
                    AND id=:map AND configuration_revision=:expected RETURNING id
            """),
                params | {"expected": expected_revision},
            ).scalar_one_or_none()
            if changed is None:
                raise MapRevisionConflictError("configuration revision changed")
        SqlAuditRepository(self._session).append(audit)
        self._scope(context, Permission.PROJECT_EDIT)
        self._session.execute(
            text("""
            INSERT INTO maps.map_operations(
                organization_id,project_id,operation_id,map_id,actor_type,actor_id,
                request_hash,committed_revision,created_at)
            VALUES(:organization,:project,:operation,:map,:actor_type,:actor_id,
                   :hash,:revision,:created)
        """),
            scope
            | {
                "operation": operation.operation_id,
                "map": operation.map_id,
                "actor_type": operation.actor_type,
                "actor_id": operation.actor_id,
                "hash": operation.request_hash,
                "revision": operation.committed_revision,
                "created": operation.created_at,
            },
        )
        self._session.execute(
            text("""
            INSERT INTO maps.map_revisions(
                organization_id,project_id,map_id,configuration_revision,prior_revision,
                operation_id,actor_type,actor_id,actor_display_snapshot,created_at,
                before_configuration,after_configuration,audit_event_id)
            VALUES(:organization,:project,:map,:revision,:prior,:operation,:actor_type,
                :actor_id,:display,:created,CAST(:before AS jsonb),CAST(:after AS jsonb),:audit)
        """),
            scope
            | {
                "map": revision.map_id,
                "revision": revision.configuration_revision,
                "prior": revision.prior_revision,
                "operation": revision.operation_id,
                "actor_type": revision.actor_type,
                "actor_id": revision.actor_id,
                "display": revision.actor_display_snapshot,
                "created": revision.created_at,
                "before": json.dumps(revision.before_configuration.to_wire(), allow_nan=False)
                if revision.before_configuration
                else None,
                "after": json.dumps(revision.after_configuration.to_wire(), allow_nan=False),
                "audit": revision.audit_event_id,
            },
        )

    def list_history(
        self,
        *,
        context: AuthorizationContext,
        after_revision: int | None,
        limit: int,
    ) -> list[MapRevision]:
        if type(limit) is not int or not 1 <= limit <= 101:
            raise ValueError("history limit outside bounds")
        rows = self._session.execute(
            text("""
            SELECT * FROM maps.map_revisions
            WHERE organization_id=:organization AND project_id=:project
                AND (CAST(:after AS bigint) IS NULL OR configuration_revision<:after)
            ORDER BY configuration_revision DESC LIMIT :limit
        """),
            self._scope(context, Permission.PROJECT_READ)
            | {
                "after": after_revision,
                "limit": limit,
            },
        ).mappings()
        return [_revision(row) for row in rows]
