"""Authenticated project-scoped interoperability catalog and read-only adapter profiles.

No connection secrets, remote requests, file uploads, or writes are exposed here.
"""

from __future__ import annotations

from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Response
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
)
from openvietgis_api.adapters.persistence import gis_access
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.scope_helper import prepare_project_scope
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import AuthorizationService
from openvietgis_api.domains.integration_hub.catalog import get_adapter, list_adapters
from openvietgis_api.domains.integration_hub.profiles import (
    PROFILE_ENABLED_ADAPTER_KEYS,
    LayerAccessSnapshot,
    build_adapter_profile,
    is_explicit_readable_crs,
)

router = APIRouter(tags=["integration-hub"])


def _set_no_store_headers(response: Response) -> None:
    response.headers["Cache-Control"] = "no-store"

_ACTIVE_LAYERS_SQL = text(
    """
    SELECT
        l.id AS layer_id,
        l.source_id AS source_id,
        l.name AS name,
        s.geometry_family AS geometry_family,
        s.dimensions AS dimensions,
        s.validation_state AS validation_state,
        s.source_crs AS source_crs_code,
        s.stored_crs AS stored_crs_code,
        l.layer_revision AS layer_revision,
        s.content_revision AS content_revision,
        s.schema_revision AS schema_revision
    FROM spatial.layers l
    JOIN spatial.data_sources s
      ON s.organization_id = l.organization_id
     AND s.project_id = l.project_id
     AND s.id = l.source_id
    WHERE l.organization_id = :org
      AND l.project_id = :proj
      AND l.lifecycle = 'active'
      AND s.lifecycle = 'active'
    ORDER BY l.name ASC, l.id ASC
    LIMIT 1000
    """
)


def _is_scoped(session: Session, context: AuthorizationContext) -> bool:
    return bool(gis_access.scoped(session, context))


def _source_allowed(
    session: Session,
    context: AuthorizationContext,
    source_id: object,
    action: str,
) -> bool:
    return bool(gis_access.source_allowed(session, context, source_id, action))


def _load_accessible_layers(
    session: Session,
    context: AuthorizationContext,
    *,
    organization_id: UUID,
    project_id: UUID,
    is_scoped: bool,
    permissions: frozenset[Permission],
) -> list[LayerAccessSnapshot]:
    rows = (
        session.execute(
            _ACTIVE_LAYERS_SQL,
            {"org": organization_id, "proj": project_id},
        )
        .mappings()
        .all()
    )
    snapshots: list[LayerAccessSnapshot] = []
    for row in rows:
        source_id = row["source_id"]
        validation_state = str(row["validation_state"])
        source_crs_raw = row["source_crs_code"]
        stored_crs_raw = row["stored_crs_code"]
        stored_crs_code = int(stored_crs_raw) if stored_crs_raw is not None else None
        crs_explicit = is_explicit_readable_crs(stored_crs_code, validation_state)
        if is_scoped:
            if not _source_allowed(session, context, source_id, "read"):
                continue
            export_permitted = (
                _source_allowed(session, context, source_id, "export") and crs_explicit
            )
        else:
            export_permitted = Permission.PROJECT_READ in permissions and crs_explicit
        snapshots.append(
            LayerAccessSnapshot(
                layer_id=str(row["layer_id"]),
                source_id=str(source_id),
                name=str(row["name"]),
                geometry_family=str(row["geometry_family"]),
                dimensions=str(row["dimensions"]),
                validation_state=validation_state,
                source_crs_code=int(source_crs_raw) if source_crs_raw is not None else None,
                stored_crs_code=stored_crs_code,
                layer_revision=int(row["layer_revision"]),
                content_revision=str(row["content_revision"]),
                schema_revision=str(row["schema_revision"]),
                export_permitted=export_permitted,
            )
        )
    return snapshots


@router.get(
    "/organizations/{organization_id}/projects/{project_id}/integration-hub/catalog",
    dependencies=[Depends(_set_no_store_headers)],
)
def read_integration_catalog(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)
    permissions = AuthorizationService(
        SqlEffectivePermissionProvider(session)
    ).effective_permissions(context)
    is_scoped = _is_scoped(session, context)
    return {
        "schema_version": 1,
        "organization_id": str(organization_id),
        "project_id": str(project_id),
        "mode": "catalog_read_only",
        "scoped_access": is_scoped,
        "default_write_policy": "deny",
        "adapters": list_adapters(permissions),
    }


@router.get(
    "/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}",
    dependencies=[Depends(_set_no_store_headers)],
)
def read_integration_adapter_profile(
    organization_id: UUID,
    project_id: UUID,
    adapter_key: str,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, object]:
    context = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, context, permission=Permission.PROJECT_READ)
    adapter = get_adapter(adapter_key)
    if adapter is None:
        raise HTTPException(status_code=404, detail="Integration adapter not found.")
    permissions = AuthorizationService(
        SqlEffectivePermissionProvider(session)
    ).effective_permissions(context)
    is_scoped = _is_scoped(session, context)
    layers = (
        _load_accessible_layers(
            session,
            context,
            organization_id=organization_id,
            project_id=project_id,
            is_scoped=is_scoped,
            permissions=permissions,
        )
        if adapter.key in PROFILE_ENABLED_ADAPTER_KEYS
        else []
    )
    return build_adapter_profile(
        adapter,
        permissions,
        organization_id=str(organization_id),
        project_id=str(project_id),
        scoped_access=is_scoped,
        layers=layers,
    )
