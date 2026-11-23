"""Project-scoped DEM preview: bounded local sampling, no persistent mutation."""

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Annotated, Literal
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Response
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.raster import (
    MAX_UPLOAD,
    WHERE,
    WORKER,
    RasterWorkerOptions,
    gate,
    raster_scope,
)
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    get_db_session,
    require_csrf,
)
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["terrain-preview"])
BASE = "/organizations/{organization_id}/projects/{project_id}/raster/layers"
TERRAIN = WORKER.with_name("terrain_worker.py")


class TerrainProfile(BaseModel):
    model_config = ConfigDict(extra="forbid")
    # The file's source may omit vertical metadata. This is explicit user declaration.
    elevation_unit: Literal["metre"]
    vertical_reference: Literal["local", "orthometric", "ellipsoidal"]
    reference_label: str = Field(min_length=1, max_length=120)
    acknowledge_dem: Literal[True]


@router.post(BASE + "/{layer_id}/terrain-preview")
def terrain_preview(
    organization_id: UUID,
    project_id: UUID,
    layer_id: UUID,
    body: TerrainProfile,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
) -> Response:
    ctx = raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    if ctx.actor.actor_type != "human":
        raise HTTPException(403, "Terrain preview requires human project access")
    label = body.reference_label.strip()
    if not label:
        raise HTTPException(422, "Declare the vertical reference")
    args = {"o": organization_id, "p": project_id, "id": layer_id}
    if not gate.acquire(blocking=False):
        raise HTTPException(429, "Raster worker busy; retry shortly")
    try:
        row = (
            session.execute(
                text(f"SELECT name,metadata,cog FROM raster.layers WHERE {WHERE} AND id=:id"),
                args,
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            raise HTTPException(404, "DEM layer not found")
        # Only original single-band extraction, not NDVI, hillshade or derivative indices.
        if row["metadata"].get("method") != "band" or row["metadata"].get("output_bands", 1) != 1:
            raise HTTPException(422, "Select an imported single elevation band, not an index")
        data = bytes(row["cog"])
        name = row["name"]
        if len(data) > MAX_UPLOAD:
            raise HTTPException(413, "Raster input exceeds limit")
        session.rollback()
        with tempfile.TemporaryDirectory(prefix="ovg-terrain-") as directory:
            root = Path(directory)
            src = root / "source.tif"
            src.write_bytes(data)
            src.chmod(0o644)
            for script in (WORKER, TERRAIN):
                shutil.copyfile(script, root / script.name)
                (root / script.name).chmod(0o644)
            opts: RasterWorkerOptions = {}
            if os.geteuid() == 0:
                import pwd

                nobody = pwd.getpwnam("nobody")
                os.chown(root, nobody.pw_uid, nobody.pw_gid)
                opts = {"user": nobody.pw_uid, "group": nobody.pw_gid, "extra_groups": []}
            try:
                proc = subprocess.run(
                    ["/usr/bin/python3", str(root / TERRAIN.name), str(src)],
                    cwd=root,
                    env={
                        "PATH": "/usr/bin:/bin",
                        "HOME": str(root),
                        "LANG": "C.UTF-8",
                        "PROJ_NETWORK": "OFF",
                        "GDAL_NUM_THREADS": "1",
                        "OPENBLAS_NUM_THREADS": "1",
                        "PYTHONDONTWRITEBYTECODE": "1",
                    },
                    capture_output=True,
                    text=True,
                    timeout=28,
                    umask=0o077,
                    **opts,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                raise HTTPException(422, "Terrain worker unavailable or timed out") from exc
            if len(proc.stdout) > 1_000_000:
                raise HTTPException(422, "Terrain preview exceeds result limit")
            try:
                result = json.loads(proc.stdout)
            except (TypeError, ValueError) as exc:
                raise HTTPException(422, "Terrain preview failed") from exc
            if proc.returncode or "error" in result:
                raise HTTPException(
                    422,
                    str(result.get("error", "Terrain preview failed")).replace(directory, "<dem>")[
                        :300
                    ],
                )
        # A user may lose access or the immutable raster may be deleted during processing.
        raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
        if (
            session.execute(
                text(f"SELECT id FROM raster.layers WHERE {WHERE} AND id=:id"), args
            ).scalar_one_or_none()
            is None
        ):
            raise HTTPException(409, "DEM was deleted during sampling")
        result.update(
            source_id=str(layer_id),
            name=name,
            elevation_unit="metre",
            vertical_reference=body.vertical_reference,
            reference_label=label,
            render_origin_m=result["min_m"],
            declared_by_user=True,
        )
        return Response(
            content=json.dumps(result, allow_nan=False),
            media_type="application/json",
            headers={"Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff"},
        )
    finally:
        gate.release()
