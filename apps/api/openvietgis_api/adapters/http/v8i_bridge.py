"""R025 V8i bridge: project-authorized capability discovery only.

No DGN is accepted here. Device pairing, network sync and write-back MUST NOT
be exposed until scoped device credentials, RLS and source CRS gates pass.
"""

from __future__ import annotations

from typing import Annotated, Any
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
)
from openvietgis_api.adapters.persistence.gis_access import scoped
from openvietgis_api.adapters.persistence.scope_helper import prepare_project_scope
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["integrations", "cad"])
_BASE = "/organizations/{organization_id}/projects/{project_id}/integrations/v8i"


def capability_status() -> dict[str, Any]:
    """Honest static capability registry: never infer sync from a local COM probe."""
    return {
        "connector": "microstation-v8i",
        "contract": "ovg.v8i.snapshot.v1",
        "status": "experimental",
        "connection_status": "not_paired",
        "windows_bridge_available": True,
        "com_probe_verified_on_windows": False,
        "geometry_scan_verified_on_windows": False,
        "cadastral_semantics_verified": False,
        "coordinate_units_verified": False,
        "capabilities": {
            "local_com_probe": "experimental",
            "local_snapshot_validation": "verified_fixture_only",
            "local_scan": "experimental_unverified",
            "remote_preview": "unavailable",
            "sync_to_openvietgis": "unavailable",
            "write_back_to_dgn": "unavailable",
        },
        "requires": [
            "licensed_running_microstation_v8i",
            "windows_pywin32",
            "explicit_crs",
            "cad_unit_control_point_verification",
            "authorized_device_pairing_before_remote_sync",
        ],
        "read_only": True,
        "live_connection": False,
    }


@router.get(f"{_BASE}/capabilities")
def read_v8i_capabilities(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> dict[str, Any]:
    if auth.actor.actor_type != "human":
        raise HTTPException(
            status_code=403, detail="V8i bridge is limited to human project members"
        )
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, ctx, permission=Permission.PROJECT_READ)
    if scoped(session, ctx):
        raise HTTPException(
            status_code=403, detail="V8i bridge requires unrestricted project access"
        )
    return capability_status()
