"""Public reviewed catalog and tenant-admin installation of exact signed manifests."""

from typing import Annotated, Any
from uuid import UUID

from fastapi import APIRouter, Depends, Response
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.extensions import _manifest_wire
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.extensions_repository import SqlExtensionsRepository
from openvietgis_api.config import get_settings
from openvietgis_api.domains.extensions.marketplace import load_catalog
from openvietgis_api.domains.platform_errors import PlatformNotFoundError

router = APIRouter(tags=["marketplace"])


def _catalog() -> list[dict[str, Any]]:
    settings = get_settings()
    return load_catalog(settings.marketplace_catalog_path, settings.marketplace_operator_public_key)


@router.get("/marketplace/catalog")
def marketplace_catalog(response: Response) -> dict[str, Any]:
    response.headers["Cache-Control"] = "no-store"
    return {"items": _catalog()}


@router.post(
    "/organizations/{organization_id}/marketplace/install/{publisher_id}/{extension_key}/{version}",
    status_code=201,
)
def install_marketplace_release(
    organization_id: UUID,
    publisher_id: str,
    extension_key: str,
    version: str,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    release = next(
        (
            r
            for r in _catalog()
            if r["publisher_id"] == publisher_id
            and r["manifest"]["extension_key"] == extension_key
            and r["manifest"]["version"] == version
        ),
        None,
    )
    if release is None:
        raise PlatformNotFoundError("reviewed marketplace release not found")
    ctx = context_for(auth, organization_id=organization_id)
    return _manifest_wire(
        SqlExtensionsRepository(session).register_manifest(context=ctx, **release["manifest"])
    )
