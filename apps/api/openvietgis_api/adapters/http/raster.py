"""Project-scoped, read-only analysis of caller-uploaded rasters."""

import base64
import json
import os
import shutil
import stat
import subprocess
import tempfile
import threading
from datetime import UTC, datetime
from pathlib import Path
from typing import Annotated, Literal, TypedDict
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, HTTPException, Response, UploadFile
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.gis_access import deny, scoped
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["raster-analysis"])
BASE = "/organizations/{organization_id}/projects/{project_id}/raster"
WORKER = Path(__file__).resolve().parents[1] / "formats" / "raster_worker.py"
LIMIT = 32 * 1024 * 1024
gate = threading.BoundedSemaphore(1)


class RasterWorkerOptions(TypedDict, total=False):
    user: int
    group: int
    extra_groups: list[int]


def stage(upload: UploadFile, target: Path, limit: int = LIMIT) -> None:
    count = 0
    with target.open("wb") as out:
        magic = upload.file.read(4)
        if magic not in (b"II*\x00", b"MM\x00*", b"II+\x00", b"MM\x00+"):
            raise HTTPException(422, "Only local GeoTIFF files are accepted")
        out.write(magic)
        count += 4
        while chunk := upload.file.read(1024 * 1024):
            count += len(chunk)
            if count > limit:
                raise HTTPException(413, f"Each GeoTIFF is limited to {limit // (1024 * 1024)} MiB")
            out.write(chunk)
    target.chmod(0o644)


@router.post(BASE + "/analyze")
def analyze_raster(
    organization_id: UUID,
    project_id: UUID,
    file: Annotated[UploadFile, File()],
    method: Annotated[Literal["band", "ndvi", "ndwi", "ndbi", "hillshade"], Form()],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    band_a: Annotated[int, Form(ge=1, le=16)] = 1,
    band_b: Annotated[int, Form(ge=1, le=16)] = 2,
    second: Annotated[UploadFile | None, File()] = None,
):
    ctx = context_for(auth, organization_id=organization_id, project_id=project_id)
    prepare_project_scope(session, ctx, permission=Permission.PROJECT_READ)
    if scoped(session, ctx):
        deny()
    if second is not None and method not in ("ndvi", "ndwi", "ndbi"):
        raise HTTPException(422, "Second file is supported for index calculations only")
    if not gate.acquire(blocking=False):
        raise HTTPException(429, "Raster worker busy; retry shortly")
    try:
        with tempfile.TemporaryDirectory(prefix="ovg-raster-") as directory:
            root = Path(directory)
            path = root / "source.tif"
            stage(file, path)
            other = root / "second.tif"
            if second is not None:
                stage(second, other)
            output = root / "result.tif"
            worker = root / "worker.py"
            shutil.copyfile(WORKER, worker)
            worker.chmod(0o644)
            opts: RasterWorkerOptions = {}
            if os.geteuid() == 0:
                import pwd

                nobody = pwd.getpwnam("nobody")
                os.chown(root, nobody.pw_uid, nobody.pw_gid)
                opts = {"user": nobody.pw_uid, "group": nobody.pw_gid, "extra_groups": []}
            try:
                p = subprocess.run(
                    [
                        "/usr/bin/python3",
                        str(worker),
                        str(path),
                        str(other) if second is not None else "-",
                        method,
                        str(band_a),
                        str(band_b),
                        str(output),
                    ],
                    cwd=root,
                    env={
                        "PATH": "/usr/bin:/bin",
                        "LANG": "C.UTF-8",
                        "PROJ_NETWORK": "OFF",
                        "GDAL_NUM_THREADS": "1",
                        "OPENBLAS_NUM_THREADS": "1",
                        "HOME": str(root),
                    },
                    capture_output=True,
                    text=True,
                    timeout=28,
                    umask=0o077,
                    **opts,
                )
            except (subprocess.TimeoutExpired, OSError) as exc:
                raise HTTPException(
                    422, "Raster worker unavailable or resource limit exceeded"
                ) from exc
            try:
                result = json.loads(p.stdout)
            except (ValueError, TypeError):
                raise HTTPException(422, "Unable to process this GeoTIFF") from None
            if p.returncode != 0 or "error" in result:
                raise HTTPException(
                    422,
                    str(result.get("error", "Raster processing failed")).replace(
                        directory, "<raster>"
                    )[:300],
                )
            try:
                fd = os.open(output, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                with os.fdopen(fd, "rb") as stream:
                    info = os.fstat(stream.fileno())
                    if (
                        not stat.S_ISREG(info.st_mode)
                        or info.st_size > 16 * 1024 * 1024
                        or info.st_nlink != 1
                    ):
                        raise HTTPException(422, "Invalid raster output")
                    content = stream.read(16 * 1024 * 1024 + 1)
            except OSError as exc:
                raise HTTPException(422, "Invalid raster output") from exc
            if len(content) > 16 * 1024 * 1024:
                raise HTTPException(422, "Raster output exceeded its limit")
            return result | {
                "method": method,
                "tiff": base64.b64encode(content).decode("ascii"),
                "persisted": False,
            }
    finally:
        gate.release()


# Persistent storage is a separate raster catalog; vector feature ACLs are not
# implicitly extended to imagery. Scoped accounts cannot access these assets.

PERSIST = WORKER.with_name("persistent_raster_worker.py")
MAX_UPLOAD = 64 * 1024 * 1024
MAX_PROJECT = 512 * 1024 * 1024
NO_CACHE = {"Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff"}
COLS = "id,name,metadata,byte_size,created_at"
WHERE = "organization_id=:o AND project_id=:p"


def raster_scope(session, auth, o, p, permission):
    ctx = context_for(auth, organization_id=o, project_id=p)
    prepare_project_scope(session, ctx, permission=permission)
    if scoped(session, ctx):
        deny()
    return ctx


def wire(row):
    return {
        "id": str(row["id"]),
        "name": row["name"],
        **row["metadata"],
        "byte_size": row["byte_size"],
        "created_at": row["created_at"].isoformat(),
    }


def read_safe(path, limit):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, "rb") as stream:
            info = os.fstat(stream.fileno())
            if (
                not stat.S_ISREG(info.st_mode)
                or info.st_nlink != 1
                or not 0 < info.st_size <= limit
            ):
                raise HTTPException(422, "Invalid raster output size")
            data = stream.read(limit + 1)
        if len(data) > limit:
            raise HTTPException(422, "Invalid raster output size")
        return data
    except OSError as exc:
        raise HTTPException(422, "Invalid raster output") from exc


@router.get(BASE + "/layers")
def raster_layers(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = session.execute(
        text(f"SELECT {COLS} FROM raster.layers WHERE {WHERE} ORDER BY created_at DESC,id"),
        {"o": organization_id, "p": project_id},
    ).mappings()
    return {"items": [wire(r) for r in rows]}


@router.post(BASE + "/layers", status_code=201)
def store_raster(
    organization_id: UUID,
    project_id: UUID,
    method: Annotated[
        Literal[
            "band",
            "ndvi",
            "ndwi",
            "ndbi",
            "hillshade",
            "calculator",
            "rgb",
            "clip",
            "change",
            "slope",
            "aspect",
            "mosaic",
            "resample",
        ],
        Form(),
    ],
    name: Annotated[str, Form(min_length=1, max_length=160)],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    file: Annotated[UploadFile | None, File()] = None,
    band_a: Annotated[int, Form(ge=1, le=16)] = 1,
    band_b: Annotated[int, Form(ge=1, le=16)] = 2,
    band_c: Annotated[int, Form(ge=1, le=16)] = 3,
    second: Annotated[UploadFile | None, File()] = None,
    third: Annotated[UploadFile | None, File()] = None,
    source_id: Annotated[UUID | None, Form()] = None,
    second_id: Annotated[UUID | None, Form()] = None,
    third_id: Annotated[UUID | None, Form()] = None,
    formula: Annotated[str | None, Form(max_length=200)] = None,
    clip_geojson: Annotated[str | None, Form(max_length=150000)] = None,
    factor: Annotated[float | None, Form(ge=0.1, le=10)] = None,
    resampling: Annotated[Literal["near", "bilinear"] | None, Form()] = None,
    sentinel_scene: Annotated[
        str | None, Form(pattern=r"^S2[A-D]_MSIL2A_[A-Z0-9_]+$", max_length=100)
    ] = None,
    sentinel_baseline: Annotated[str | None, Form(pattern=r"^\d{2}\.\d{2}$")] = None,
):
    name = name.strip()
    if not name:
        raise HTTPException(422, "Raster name is required")
    if (file is None) == (source_id is None):
        raise HTTPException(422, "Provide exactly one source file or project raster")
    if second is not None and second_id is not None or third is not None and third_id is not None:
        raise HTTPException(422, "Choose one source per input")
    if (second is not None or second_id is not None) and method not in (
        "ndvi",
        "ndwi",
        "ndbi",
        "calculator",
        "rgb",
        "change",
        "mosaic",
    ):
        raise HTTPException(422, "Second raster is not used by this operation")
    if (third is not None or third_id is not None) and method != "rgb":
        raise HTTPException(422, "Third raster is only for RGB")
    if (formula is not None) != (method == "calculator") or (clip_geojson is not None) != (
        method == "clip"
    ):
        raise HTTPException(
            422, "Formula or clip geometry is required only for its corresponding operation"
        )
    if method == "mosaic" and second is None and second_id is None:
        raise HTTPException(422, "Mosaic requires a second raster")
    if (factor is not None) != (method == "resample") or (resampling is not None) != (
        method == "resample"
    ):
        raise HTTPException(422, "Resampling factor and method are required only for resample")
    if (sentinel_scene is None) != (sentinel_baseline is None):
        raise HTTPException(422, "Sentinel scene and processing baseline are required")
    if sentinel_scene and (
        method not in ("ndvi", "ndwi", "ndbi")
        or file is None
        or second is not None
        or second_id is not None
        or band_a != 1
        or band_b != 2
    ):
        raise HTTPException(422, "Sentinel analysis uses a three-band spectral crop A/B/SCL")

    def invalid_constant(_):
        raise ValueError("Non-finite JSON value")

    try:
        clip = (
            json.loads(clip_geojson, parse_constant=invalid_constant)
            if clip_geojson is not None
            else None
        )
        json.dumps(clip, allow_nan=False)
    except (ValueError, RecursionError):
        raise HTTPException(422, "Invalid clip GeoJSON") from None
    args = {"o": organization_id, "p": project_id}
    raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)

    def quota(extra=0):
        count, size = session.execute(
            text(f"SELECT count(*),coalesce(sum(byte_size),0) FROM raster.layers WHERE {WHERE}"),
            args,
        ).one()
        if count >= 32 or size + extra > MAX_PROJECT:
            raise HTTPException(413, "Project raster quota: 32 layers and 512 MiB")

    quota()
    if not gate.acquire(blocking=False):
        raise HTTPException(429, "Raster worker busy; retry shortly")
    try:
        with tempfile.TemporaryDirectory(prefix="ovg-raster-store-") as directory:
            root = Path(directory)
            total_input = 0

            def input_file(upload, rid, path):
                nonlocal total_input
                if upload is None and rid is None:
                    return "-"
                if rid is not None:
                    data = session.execute(
                        text(f"SELECT cog FROM raster.layers WHERE {WHERE} AND id=:id"),
                        args | {"id": rid},
                    ).scalar_one_or_none()
                    if data is None:
                        raise HTTPException(404, "Input raster not found in this project")
                    if len(data) > MAX_UPLOAD:
                        raise HTTPException(413, "Input raster exceeds 64 MiB")
                    path.write_bytes(bytes(data))
                    path.chmod(0o644)
                else:
                    stage(upload, path, MAX_UPLOAD)
                total_input += path.stat().st_size
                if total_input > 96 * 1024 * 1024:
                    raise HTTPException(413, "Combined rasters limited to 96 MiB")
                return str(path)

            src = input_file(file, source_id, root / "source.tif")
            src2 = input_file(second, second_id, root / "second.tif")
            src3 = input_file(third, third_id, root / "third.tif")
            # Release project locks during GDAL processing.
            # Revalidate under write lock before committing.
            session.rollback()
            config = root / "config.json"
            config.write_text(
                json.dumps(
                    {
                        "formula": formula,
                        "clip": clip,
                        "third": src3,
                        "band_c": band_c,
                        "factor": factor,
                        "resampling": resampling,
                        "sentinel_baseline": sentinel_baseline,
                    },
                    allow_nan=False,
                )
            )
            config.chmod(0o644)
            for script in (WORKER, PERSIST):
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
                    [
                        "/usr/bin/python3",
                        str(root / PERSIST.name),
                        src,
                        src2,
                        method,
                        str(band_a),
                        str(band_b),
                        str(root),
                        str(config),
                    ],
                    cwd=root,
                    env={
                        "PATH": "/usr/bin:/bin",
                        "HOME": str(root),
                        "LANG": "C.UTF-8",
                        "PROJ_NETWORK": "OFF",
                        "GDAL_NUM_THREADS": "1",
                        "OPENBLAS_NUM_THREADS": "1",
                        "OVG_RASTER_PERSIST": "1",
                        "PYTHONDONTWRITEBYTECODE": "1",
                    },
                    capture_output=True,
                    text=True,
                    timeout=65,
                    umask=0o077,
                    **opts,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                raise HTTPException(
                    422, "Raster worker unavailable or resource limit exceeded"
                ) from exc
            try:
                meta = json.loads(proc.stdout)
            except (ValueError, TypeError):
                raise HTTPException(422, "Unable to process GeoTIFF") from None
            if proc.returncode or "error" in meta:
                raise HTTPException(
                    422,
                    str(meta.get("error", "Raster processing failed")).replace(
                        directory, "<raster>"
                    )[:300],
                )
            manifest = meta.pop("tiles", [])
            if not 1 <= len(manifest) <= 192:
                raise HTTPException(422, "Invalid raster tile manifest")
            cog = read_safe(root / "result.tif", MAX_UPLOAD)
            if cog[:4] not in (b"II*\x00", b"MM\x00*", b"II+\x00", b"MM\x00+"):
                raise HTTPException(422, "Invalid GeoTIFF result")
            assets = []
            size = len(cog)
            seen = set()
            for entry in manifest:
                if (
                    not isinstance(entry, list)
                    or len(entry) != 3
                    or not all(type(v) is int for v in entry)
                ):
                    raise HTTPException(422, "Invalid raster tile")
                z, x, y = entry
                if not (0 <= z <= 20 and 0 <= x < 1 << z and 0 <= y < 1 << z) or (z, x, y) in seen:
                    raise HTTPException(422, "Invalid raster tile")
                seen.add((z, x, y))
                png = read_safe(root / f"{z}-{x}-{y}.png", 400000)
                if png[:8] != b"\x89PNG\r\n\x1a\n":
                    raise HTTPException(422, "Invalid raster PNG")
                size += len(png)
                if size > 128 * 1024 * 1024:
                    raise HTTPException(413, "Raster and tiles exceed 128 MiB")
                assets.append({"z": z, "x": x, "y": y, "png": png})
            ctx = raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
            quota(size)
            rid = uuid4()
            meta["tile_count"] = len(assets)
            for input_id in (source_id, second_id, third_id):
                if (
                    input_id is not None
                    and session.execute(
                        text(f"SELECT id FROM raster.layers WHERE {WHERE} AND id=:id"),
                        args | {"id": input_id},
                    ).scalar_one_or_none()
                    is None
                ):
                    raise HTTPException(409, "Input raster was removed during processing")
            meta["inputs"] = {
                "a": str(source_id) if source_id else None,
                "b": str(second_id) if second_id else None,
                "c": str(third_id) if third_id else None,
                "band_a": band_a,
                "band_b": band_b,
                "band_c": band_c,
            }
            if sentinel_scene:
                meta["sentinel"] = {
                    "scene": sentinel_scene,
                    "baseline": sentinel_baseline,
                    "mask": "SCL 4,5,6",
                    "provider": "Microsoft Planetary Computer",
                    "input": "client crop",
                }
            if factor is not None:
                meta["factor"] = factor
                meta["resampling"] = resampling
            if formula is not None:
                meta["formula"] = formula
            if clip is not None:
                meta["clip_geojson"] = clip
            row = (
                session.execute(
                    text(
                        "INSERT INTO raster.layers(organization_id,project_id,id"
                        ",name,metadata,cog,byte_size)\n              "
                        "VALUES(:o,:p,:id,:name,CAST(:meta AS jsonb),:cog,:size)"
                        " RETURNING "
                        f"{COLS}"
                    ),
                    args
                    | {
                        "id": rid,
                        "name": name,
                        "meta": json.dumps(meta, allow_nan=False),
                        "cog": cog,
                        "size": size,
                    },
                )
                .mappings()
                .one()
            )
            session.execute(
                text(
                    "INSERT INTO raster.tiles(organization_id,project_id,lay"
                    "er_id,z,x,y,png) VALUES(:o,:p,:id,:z,:x,:y,:png)"
                ),
                [args | {"id": rid} | a for a in assets],
            )
            append_project_audit(
                session,
                ctx,
                audit_id=uuid4(),
                action="raster.layer.create",
                resource_type="raster_layer",
                resource_id=rid,
                metadata={"name": name, "method": method, "tiles": len(assets), "byte_size": size},
                created_at=datetime.now(UTC),
            )
            result = wire(row)
            session.commit()
            return result
    finally:
        gate.release()


@router.post(BASE + "/reports")
def raster_report(
    organization_id: UUID,
    project_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
    method: Annotated[Literal["zonal", "contour"], Form()],
    source_id: Annotated[UUID, Form()],
    band: Annotated[int, Form(ge=1, le=16)] = 1,
    zones_json: Annotated[str | None, Form(max_length=750000)] = None,
    interval: Annotated[float | None, Form(gt=0)] = None,
):
    if (zones_json is not None) != (method == "zonal") or (interval is not None) != (
        method == "contour"
    ):
        raise HTTPException(422, "Provide zones or contour interval")
    try:
        zones = json.loads(zones_json) if zones_json is not None else None
        if zones is not None and (
            not isinstance(zones, list)
            or not 1 <= len(zones) <= 100
            or not all(isinstance(z, dict) for z in zones)
        ):
            raise ValueError()
        cfg = json.dumps({"zones": zones, "interval": interval}, allow_nan=False)
    except (ValueError, RecursionError):
        raise HTTPException(422, "Invalid report parameters") from None
    args = {"o": organization_id, "p": project_id, "id": source_id}
    raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    if not gate.acquire(blocking=False):
        raise HTTPException(429, "Raster worker busy; retry shortly")
    try:
        data = session.execute(
            text(f"SELECT cog FROM raster.layers WHERE {WHERE} AND id=:id"), args
        ).scalar_one_or_none()
        if data is None:
            raise HTTPException(404, "Input raster not found in this project")
        if len(data) > MAX_UPLOAD:
            raise HTTPException(413, "Input raster exceeds 64 MiB")
        with tempfile.TemporaryDirectory(prefix="ovg-raster-report-") as directory:
            root = Path(directory)
            src = root / "source.tif"
            src.write_bytes(bytes(data))
            src.chmod(0o644)
            session.rollback()
            config = root / "config.json"
            config.write_text(cfg)
            config.chmod(0o644)
            for script in (WORKER, PERSIST):
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
                    [
                        "/usr/bin/python3",
                        str(root / PERSIST.name),
                        str(src),
                        "-",
                        method,
                        str(band),
                        "1",
                        str(root),
                        str(config),
                    ],
                    cwd=root,
                    env={
                        "PATH": "/usr/bin:/bin",
                        "HOME": str(root),
                        "LANG": "C.UTF-8",
                        "PROJ_NETWORK": "OFF",
                        "GDAL_NUM_THREADS": "1",
                        "OPENBLAS_NUM_THREADS": "1",
                        "OVG_RASTER_PERSIST": "1",
                        "PYTHONDONTWRITEBYTECODE": "1",
                    },
                    capture_output=True,
                    text=True,
                    timeout=65,
                    umask=0o077,
                    **opts,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                raise HTTPException(422, "Raster report resource limit exceeded") from exc
            if len(proc.stdout) > 4_000_000:
                raise HTTPException(422, "Raster report too large")
            try:
                result = json.loads(proc.stdout)
            except (ValueError, TypeError):
                raise HTTPException(422, "Unable to calculate raster report") from None
            if proc.returncode or "error" in result:
                raise HTTPException(
                    422,
                    str(result.get("error", "Report failed")).replace(directory, "<raster>")[:300],
                )
            raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
            if (
                session.execute(
                    text(f"SELECT id FROM raster.layers WHERE {WHERE} AND id=:id"), args
                ).scalar_one_or_none()
                is None
            ):
                raise HTTPException(409, "Input raster removed during processing")
            return result
    finally:
        gate.release()


@router.get(BASE + "/layers/{layer_id}/tiles/{z}/{x}/{y}.png")
def raster_tile(
    organization_id: UUID,
    project_id: UUID,
    layer_id: UUID,
    z: int,
    x: int,
    y: int,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    if not (0 <= z <= 20 and 0 <= x < 1 << z and 0 <= y < 1 << z):
        raise HTTPException(404, "Tile not found")
    png = session.execute(
        text(
            "SELECT png FROM raster.tiles WHERE "
            f"{WHERE}"
            " AND layer_id=:id AND z=:z AND x=:x AND y=:y"
        ),
        {"o": organization_id, "p": project_id, "id": layer_id, "z": z, "x": x, "y": y},
    ).scalar_one_or_none()
    if png is None:
        raise HTTPException(404, "Tile not found")
    return Response(content=bytes(png), media_type="image/png", headers=NO_CACHE)


@router.get(BASE + "/layers/{layer_id}/file")
def raster_file(
    organization_id: UUID,
    project_id: UUID,
    layer_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    data = session.execute(
        text(f"SELECT cog FROM raster.layers WHERE {WHERE} AND id=:id"),
        {"o": organization_id, "p": project_id, "id": layer_id},
    ).scalar_one_or_none()
    if data is None:
        raise HTTPException(404, "Raster layer not found")
    return Response(
        content=bytes(data),
        media_type="image/tiff",
        headers=NO_CACHE | {"Content-Disposition": f'attachment; filename="raster-{layer_id}.tif"'},
    )


@router.delete(BASE + "/layers/{layer_id}", status_code=204)
def delete_raster(
    organization_id: UUID,
    project_id: UUID,
    layer_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = raster_scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    row = session.execute(
        text(f"DELETE FROM raster.layers WHERE {WHERE} AND id=:id RETURNING name"),
        {"o": organization_id, "p": project_id, "id": layer_id},
    ).scalar_one_or_none()
    if row is None:
        raise HTTPException(404, "Raster layer not found")
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="raster.layer.delete",
        resource_type="raster_layer",
        resource_id=layer_id,
        metadata={"name": row},
        created_at=datetime.now(UTC),
    )
    session.commit()
    return Response(status_code=204)
