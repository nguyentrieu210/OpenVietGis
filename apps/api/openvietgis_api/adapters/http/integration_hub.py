"""Authenticated project-scoped interoperability catalog.

No connection secrets, remote requests, file uploads or writes in slice 1.
"""

from __future__ import annotations

from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
)
from openvietgis_api.adapters.persistence.authorization_repository import (
    SqlEffectivePermissionProvider,
)
from openvietgis_api.adapters.persistence.scope_helper import prepare_project_scope
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import AuthorizationService
from openvietgis_api.domains.integration_hub.catalog import list_adapters

router = APIRouter(tags=["integration-hub"])


@router.get("/organizations/{organization_id}/projects/{project_id}/integration-hub/catalog")
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
    return {
        "schema_version": 1,
        "organization_id": str(organization_id),
        "project_id": str(project_id),
        "mode": "catalog_read_only",
        "default_write_policy": "deny",
        "adapters": list_adapters(permissions),
    }
