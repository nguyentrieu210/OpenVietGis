"""Bounded, immutable OGC 3D Tiles 1.1 region/static GLB subset."""

import hashlib
import io
import json
import math
import re
import zipfile
import zlib
from datetime import UTC, datetime
from typing import Annotated
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, HTTPException, Response, UploadFile
from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.model_vault import HEADERS, LIMIT, _scope, _validate
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import append_project_audit
from openvietgis_api.domains.authorization.permissions import Permission

router = APIRouter(tags=["3d-tiles"])
BASE = "/organizations/{organization_id}/projects/{project_id}/3d/tilesets"
MIB = 1024 * 1024
URI = re.compile(r"(?:[A-Za-z0-9_-]+/)*[A-Za-z0-9_-]+\.glb")
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]


def check(value, message="Unsupported tileset structure"):
    if not value:
        raise HTTPException(422, message)


def number(v, low, high):
    return type(v) in (int, float) and low <= v <= high


def manifest_check(doc):
    check(isinstance(doc, dict) and set(doc) <= {"asset", "geometricError", "root"})
    check(
        isinstance(doc.get("asset"), dict)
        and doc["asset"].get("version") == "1.1"
        and set(doc["asset"]) <= {"version", "tilesetVersion"}
        and isinstance(doc["asset"].get("tilesetVersion", ""), str)
        and re.fullmatch(r"[ -~]{0,160}", doc["asset"].get("tilesetVersion", ""))
    )
    check(number(doc.get("geometricError"), 0, 1e7))
    seen = set()
    count = 0

    def walk(node, parent_error, parent_region, parent_matrix, depth):
        nonlocal count
        count += 1
        check(
            count <= 256
            and depth <= 8
            and isinstance(node, dict)
            and set(node)
            <= {"boundingVolume", "geometricError", "refine", "transform", "content", "children"}
        )
        check(node.get("refine", "REPLACE") == "REPLACE")
        e = node.get("geometricError")
        check(number(e, 0, parent_error))
        b = node.get("boundingVolume")
        check(isinstance(b, dict) and set(b) == {"region"})
        r = b["region"]
        check(isinstance(r, list) and len(r) == 6 and all(number(v, -12000, 100000) for v in r))
        w, s, x, n, h, H = r
        check(
            -math.pi <= w <= x <= math.pi
            and x - w <= math.pi
            and -math.radians(85) <= s <= n <= math.radians(85)
            and -12000 <= h <= H <= 100000
        )
        if parent_region:
            check(
                all(r[i] >= parent_region[i] for i in (0, 1, 4))
                and all(r[i] <= parent_region[i] for i in (2, 3, 5))
            )
        if "transform" in node:
            t = node["transform"]
            check(
                isinstance(t, list)
                and len(t) == 16
                and all(number(v, -1e7, 1e7) for v in t)
                and [t[i] for i in (3, 7, 11, 15)] == [0, 0, 0, 1]
            )
        local = node.get("transform", IDENTITY)
        matrix = [
            sum(parent_matrix[k * 4 + row] * local[col * 4 + k] for k in range(4))
            for col in range(4)
            for row in range(4)
        ]
        check(all(number(v, -1e9, 1e9) for v in matrix))
        if "content" in node:
            c = node["content"]
            check(isinstance(c, dict) and set(c) == {"uri"})
            uri = c["uri"]
            check(
                isinstance(uri, str) and len(uri) <= 180 and URI.fullmatch(uri) and uri not in seen
            )
            seen.add(uri)
        children = node.get("children", [])
        check(isinstance(children, list) and (children or ("content" in node and e == 0)))
        for child in children:
            walk(child, e, r, matrix, depth + 1)

    check(isinstance(doc.get("root"), dict) and doc["root"].get("refine") == "REPLACE")
    walk(doc["root"], doc["geometricError"], None, IDENTITY, 0)
    check(0 < len(seen) <= 128)
    return seen


def read_archive(payload):
    if len(payload) > 64 * MIB:
        raise HTTPException(413, "ZIP exceeds 64 MiB")
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            infos = archive.infolist()
            check(2 <= len(infos) <= 129)
            names = {i.filename for i in infos}
            check(len(names) == len(infos) and "tileset.json" in names)
            if sum(i.file_size for i in infos) > 128 * MIB:
                raise HTTPException(413, "Extracted ZIP exceeds 128 MiB")
            for i in infos:
                check(
                    not i.is_dir()
                    and i.filename == i.orig_filename
                    and i.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                    and (i.external_attr >> 16 & 0o170000) != 0o120000
                    and not i.flag_bits & 1
                    and (
                        i.filename == "tileset.json"
                        or (len(i.filename) <= 180 and URI.fullmatch(i.filename))
                    )
                )
                if i.file_size > (MIB if i.filename == "tileset.json" else LIMIT):
                    raise HTTPException(413, "Resource size limit")
            doc = json.loads(archive.read("tileset.json"))
            uris = manifest_check(doc)
            check(names == uris | {"tileset.json"}, "ZIP must contain exactly referenced resources")
            resources = {}
            for uri in sorted(uris):
                data = archive.read(uri)
                _validate(uri.rsplit("/", 1)[-1], data)
                resources[uri] = data
            return doc, resources
    except (
        zipfile.BadZipFile,
        zlib.error,
        ValueError,
        TypeError,
        KeyError,
        RuntimeError,
        NotImplementedError,
        RecursionError,
        OverflowError,
    ):
        raise HTTPException(422, "Invalid ZIP or tileset") from None


def items(session, args):
    rows = session.execute(
        text("""SELECT id,name,byte_size,tile_count,created_at
      FROM spatial.tilesets WHERE organization_id=:org AND project_id=:proj
      ORDER BY created_at DESC,id"""),
        args,
    ).mappings()
    return [dict(r) | {"id": str(r["id"]), "created_at": r["created_at"].isoformat()} for r in rows]


@router.get(BASE)
def list_tilesets(
    organization_id: UUID,
    project_id: UUID,
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    return {"items": items(session, {"org": organization_id, "proj": project_id})}


@router.post(BASE, status_code=201)
def create_tileset(
    organization_id: UUID,
    project_id: UUID,
    name: Annotated[str, Form(max_length=160)],
    file: Annotated[UploadFile, File()],
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    name = name.strip()
    check(name and name.isprintable() and (file.filename or "").lower().endswith(".zip"))
    doc, resources = read_archive(file.file.read(64 * MIB + 1))
    manifest = json.dumps(doc, allow_nan=False, separators=(",", ":"))
    size = len(manifest.encode()) + sum(len(v) for v in resources.values())
    args = {"org": organization_id, "proj": project_id, "id": uuid4()}
    session.execute(
        text("SELECT pg_advisory_xact_lock(hashtextextended(:key,0))"),
        {"key": "tilesets:" + str(project_id)},
    )
    count, total = session.execute(
        text("""SELECT count(*),coalesce(sum(byte_size),0)
      FROM spatial.tilesets WHERE organization_id=:org AND project_id=:proj"""),
        args,
    ).one()
    if count >= 16 or total + size > 256 * MIB:
        raise HTTPException(413, "Project tileset quota exceeded")
    session.execute(
        text("""INSERT INTO spatial.tilesets
       (organization_id,project_id,id,name,manifest,byte_size,tile_count,actor_id)
       VALUES(:org,:proj,:id,:name,CAST(:manifest AS jsonb),:size,:count,:actor)"""),
        args
        | {
            "name": name,
            "manifest": manifest,
            "size": size,
            "count": len(resources),
            "actor": ctx.actor.actor_id,
        },
    )
    for uri, data in resources.items():
        session.execute(
            text("""INSERT INTO spatial.tileset_contents
          (organization_id,project_id,tileset_id,uri,sha256,payload)
          VALUES(:org,:proj,:id,:uri,:sha,:payload)"""),
            args | {"uri": uri, "sha": hashlib.sha256(data).hexdigest(), "payload": data},
        )
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action="3d.tileset.import",
        resource_type="tileset",
        resource_id=args["id"],
        metadata={"tile_count": len(resources), "byte_size": size, "crs": "EPSG:4979/EPSG:4978"},
        created_at=datetime.now(UTC),
    )
    result = next(r for r in items(session, args) if r["id"] == str(args["id"]))
    session.commit()
    return result


@router.get(BASE + "/{tileset_id}/tileset.json")
def get_manifest(
    organization_id: UUID,
    project_id: UUID,
    tileset_id: UUID,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    doc = session.execute(
        text("""SELECT manifest FROM spatial.tilesets
      WHERE organization_id=:org AND project_id=:proj AND id=:id"""),
        {"org": organization_id, "proj": project_id, "id": tileset_id},
    ).scalar_one_or_none()
    if doc is None:
        raise HTTPException(404, "Tileset not found")
    return Response(
        json.dumps(doc, allow_nan=False), media_type="application/json", headers=HEADERS
    )


@router.get(BASE + "/{tileset_id}/{uri:path}")
def get_tile(
    organization_id: UUID,
    project_id: UUID,
    tileset_id: UUID,
    uri: str,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    if len(uri) > 180 or not URI.fullmatch(uri):
        raise HTTPException(404, "Tile not found")
    row = session.execute(
        text("""SELECT sha256,payload FROM spatial.tileset_contents
       WHERE organization_id=:org AND project_id=:proj AND tileset_id=:id AND uri=:uri"""),
        {"org": organization_id, "proj": project_id, "id": tileset_id, "uri": uri},
    ).one_or_none()
    if row is None:
        raise HTTPException(404, "Tile not found")
    data = bytes(row.payload)
    if hashlib.sha256(data).hexdigest() != row.sha256:
        raise HTTPException(409, "Tile integrity failure")
    return Response(data, media_type="model/gltf-binary", headers=HEADERS)
