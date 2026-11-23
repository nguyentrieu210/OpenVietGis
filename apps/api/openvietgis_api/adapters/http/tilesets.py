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

from fastapi import APIRouter, Depends, File, Form, HTTPException, Query, Response, UploadFile
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.cad_documents import _linked_features
from openvietgis_api.adapters.http.model_vault import HEADERS, LIMIT, _scope, _validate
from openvietgis_api.adapters.http.security import (
    AuthenticatedRequest,
    authenticate_request,
    get_db_session,
    require_csrf,
)
from openvietgis_api.adapters.persistence.scope_helper import append_project_audit
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.spatial.tiles_generator import (
    MAX_FEATURES,
    MAX_VERTICES,
    TileGenerationError,
    build_tiles,
)

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
        text("""SELECT id,name,byte_size,tile_count,created_at,provenance
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
    return persist(
        session, ctx, organization_id, project_id, name, doc, resources, {}, "3d.tileset.import"
    )


def persist(
    session,
    ctx,
    organization_id,
    project_id,
    name,
    doc,
    resources,
    provenance,
    action,
    indices=None,
):
    indices = indices or {}
    manifest = json.dumps(doc, allow_nan=False, separators=(",", ":"))
    size = (
        len(manifest.encode())
        + sum(len(v) for v in resources.values())
        + sum(len(json.dumps(v, separators=(",", ":")).encode()) for v in indices.values())
    )
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
       (organization_id,project_id,id,name,manifest,byte_size,tile_count,actor_id,provenance)
       VALUES(:org,:proj,:id,:name,CAST(:manifest AS jsonb),:size,:count,:actor,
        CAST(:provenance AS jsonb))"""),
        args
        | {
            "name": name,
            "manifest": manifest,
            "size": size,
            "count": len(resources),
            "actor": ctx.actor.actor_id,
            "provenance": json.dumps(provenance, allow_nan=False),
        },
    )
    for uri, data in resources.items():
        session.execute(
            text("""INSERT INTO spatial.tileset_contents
          (organization_id,project_id,tileset_id,uri,sha256,payload,feature_index)
          VALUES(:org,:proj,:id,:uri,:sha,:payload,CAST(:index AS jsonb))"""),
            args
            | {
                "uri": uri,
                "sha": hashlib.sha256(data).hexdigest(),
                "payload": data,
                "index": json.dumps(indices.get(uri, []), separators=(",", ":")),
            },
        )
    append_project_audit(
        session,
        ctx,
        audit_id=uuid4(),
        action=action,
        resource_type="tileset",
        resource_id=args["id"],
        metadata={"tile_count": len(resources), "byte_size": size, "crs": "EPSG:4979/EPSG:4978"}
        | provenance,
        created_at=datetime.now(UTC),
    )
    result = next(r for r in items(session, args) if r["id"] == str(args["id"]))
    session.commit()
    return result


class GenerateTileset(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)
    name: str = Field(min_length=1, max_length=160)
    layer_id: UUID
    expected_layer_revision: int = Field(ge=1, le=9007199254740991)
    expected_content_revision: str = Field(pattern=r"^[a-f0-9]{64}$")
    expected_schema_revision: str = Field(pattern=r"^[a-f0-9]{64}$")
    height_field: str | None = Field(default=None, pattern=r"^[A-Za-z][A-Za-z0-9_]{0,63}$")
    uniform_height_m: float | None = Field(default=None, ge=0.01, le=3000, strict=True)
    base_ellipsoid_m: float = Field(ge=-1000, le=10000, strict=True)
    acknowledge_ellipsoid: bool = Field(strict=True)


@router.post(BASE + "/generate", status_code=201)
def generate_tileset(
    organization_id: UUID,
    project_id: UUID,
    body: GenerateTileset,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_EDIT)
    name = body.name.strip()
    check(
        name and name.isprintable() and body.acknowledge_ellipsoid,
        "Explicit ellipsoidal base and name required",
    )
    check(
        (body.height_field is None) != (body.uniform_height_m is None),
        "Choose exactly one height method",
    )
    args = {
        "org": organization_id,
        "proj": project_id,
        "layer": body.layer_id,
        "field": body.height_field,
        "uniform": body.uniform_height_m,
    }
    try:
        session.execute(text("SET LOCAL statement_timeout = '20s'"))
        source = (
            session.execute(
                text("""
          SELECT s.id,s.content_revision,s.schema_revision,s.stored_crs,s.feature_count,
                 s.geometry_family,s.dimensions,s.validation_state,l.layer_revision
          FROM spatial.layers l JOIN spatial.data_sources s ON
            s.organization_id=l.organization_id AND s.project_id=l.project_id AND s.id=l.source_id
          WHERE l.organization_id=:org AND l.project_id=:proj AND l.id=:layer
            AND l.lifecycle='active' AND s.lifecycle='active'
          FOR SHARE OF s,l
        """),
                args,
            )
            .mappings()
            .one_or_none()
        )
        if source is None:
            raise HTTPException(404, "Layer not found")
        if (
            source["layer_revision"] != body.expected_layer_revision
            or source["content_revision"] != body.expected_content_revision
            or source["schema_revision"] != body.expected_schema_revision
        ):
            raise HTTPException(409, "Source revision changed; reload the layer")
        check(
            source["validation_state"] == "ready"
            and source["geometry_family"] == "Polygon"
            and source["dimensions"] == "XY"
            and source["stored_crs"] in (4326, 3857),
            "Only ready XY Polygon EPSG:4326 / EPSG:3857 supported",
        )
        if not source["feature_count"] or source["feature_count"] > MAX_FEATURES:
            raise HTTPException(413, "Layer must have 1–3000 features")
        args |= {
            "src": source["id"],
            "content": source["content_revision"],
            "schema": source["schema_revision"],
            "crs": source["stored_crs"],
        }
        field = session.execute(
            text("""
          SELECT type FROM spatial.source_fields WHERE organization_id=:org AND project_id=:proj
            AND source_id=:src AND field_id=:field AND schema_revision=:schema
        """),
            args,
        ).scalar_one_or_none()
        if body.height_field is not None:
            check(field in ("integer", "decimal"), "Height field must be numeric")
        args["field_type"] = "number" if field == "integer" else "string"
        stats = (
            session.execute(
                text("""
          SELECT count(*) AS n, coalesce(sum(ST_NPoints(geometry)),0) AS vertices,
            bool_and(coalesce(geometry_state='present' AND ST_SRID(geometry)=:crs
              AND ST_GeometryType(geometry)='ST_Polygon' AND ST_NDims(geometry)=2
              AND ST_IsValid(geometry) AND NOT ST_IsEmpty(geometry)
              AND (CAST(:uniform AS double precision) IS NOT NULL
                OR jsonb_typeof(attributes -> CAST(:field AS text))=:field_type),false)) AS valid
          FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj
            AND source_id=:src AND content_revision=:content AND schema_revision=:schema
        """),
                args,
            )
            .mappings()
            .one()
        )
        if stats["n"] != source["feature_count"]:
            raise HTTPException(409, "Feature count changed")
        if stats["vertices"] > MAX_VERTICES:
            raise HTTPException(413, "60,000 vertex budget exceeded")
        check(stats["valid"], "All features require valid XY polygons and numeric heights")
        rows = (
            session.execute(
                text("""
          WITH polygons AS MATERIALIZED (
            SELECT feature_id,COALESCE(to_jsonb(CAST(:uniform AS double precision)),
                attributes -> CAST(:field AS text)) AS height,
              CASE WHEN :crs=4326 THEN geometry ELSE ST_Transform(geometry,4326) END AS geom
            FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj
              AND source_id=:src AND content_revision=:content AND schema_revision=:schema
          )
          SELECT feature_id,height,ST_AsGeoJSON(geom,15)::jsonb AS geometry,
            ST_AsGeoJSON(ST_TriangulatePolygon(geom),15)::jsonb AS triangles
          FROM polygons ORDER BY feature_id
        """),
                args,
            )
            .mappings()
            .all()
        )
        try:
            doc, resources, total, indices = build_tiles(rows, body.base_ellipsoid_m)
        except TileGenerationError as exc:
            raise HTTPException(422, str(exc)) from None
        manifest_check(doc)
        for uri, payload in resources.items():
            _validate(uri.rsplit("/", 1)[-1], payload)
        provenance = {
            "kind": "gis_extrusion",
            "feature_picking": True,
            "source_id": str(source["id"]),
            "layer_id": str(body.layer_id),
            "layer_revision": source["layer_revision"],
            "content_revision": source["content_revision"],
            "schema_revision": source["schema_revision"],
            "source_crs": source["stored_crs"],
            "height_field": body.height_field,
            "uniform_height_m": body.uniform_height_m,
            "base_ellipsoid_m": body.base_ellipsoid_m,
            "feature_count": len(rows),
            "triangle_count": total,
        }
        return persist(
            session,
            ctx,
            organization_id,
            project_id,
            name,
            doc,
            resources,
            provenance,
            "3d.tileset.generate",
            indices,
        )
    except DBAPIError as exc:
        if getattr(exc.orig, "sqlstate", None) in ("57014", "42883"):
            raise HTTPException(503, "Tile generation unavailable or timed out") from None
        raise


@router.get(BASE + "/{tileset_id}/feature")
def get_tile_feature(
    organization_id: UUID,
    project_id: UUID,
    tileset_id: UUID,
    uri: Annotated[str, Query(min_length=1, max_length=180)],
    triangle: Annotated[int, Query(ge=0, lt=100000)],
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    ctx = _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    response.headers.update(HEADERS)
    if not URI.fullmatch(uri):
        raise HTTPException(404, "Tile not found")
    args = {"org": organization_id, "proj": project_id, "id": tileset_id, "uri": uri}
    tile = (
        session.execute(
            text("""
        SELECT t.provenance,c.feature_index FROM spatial.tilesets t
        JOIN spatial.tileset_contents c ON c.organization_id=t.organization_id
          AND c.project_id=t.project_id AND c.tileset_id=t.id
        WHERE t.organization_id=:org AND t.project_id=:proj AND t.id=:id AND c.uri=:uri
    """),
            args,
        )
        .mappings()
        .one_or_none()
    )
    if tile is None or tile["provenance"].get("feature_picking") is not True:
        raise HTTPException(404, "Feature picking unavailable")
    provenance = tile["provenance"]
    cursor = 0
    match = None
    entries = tile["feature_index"]
    if not 0 < len(entries) <= MAX_FEATURES:
        raise HTTPException(409, "Feature index integrity failure")
    for entry in entries:
        if not isinstance(entry, dict):
            raise HTTPException(409, "Feature index integrity failure")
        first, count, fid = (
            entry.get("first_triangle"),
            entry.get("triangle_count"),
            entry.get("feature_id"),
        )
        if (
            type(first) is not int
            or type(count) is not int
            or first != cursor
            or count <= 0
            or cursor + count > 100000
            or not isinstance(fid, str)
            or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", fid)
        ):
            raise HTTPException(409, "Feature index integrity failure")
        if first <= triangle < first + count:
            match = entry
        cursor += count
    if match is None:
        raise HTTPException(404, "Triangle not indexed")
    identity = {
        "tileset_id": str(tileset_id),
        "uri": uri,
        **match,
        "source_id": provenance["source_id"],
        "layer_id": provenance["layer_id"],
        "layer_revision": provenance["layer_revision"],
        "content_revision": provenance["content_revision"],
        "schema_revision": provenance["schema_revision"],
    }
    args |= {
        "src": identity["source_id"],
        "layer": identity["layer_id"],
        "fid": identity["feature_id"],
        "content": identity["content_revision"],
        "schema": identity["schema_revision"],
    }
    # Hold the source/layer version stable across feature and CAD provenance reads.
    current = (
        session.execute(
            text("""
       SELECT l.name AS layer_name,l.layer_revision,s.content_revision,s.schema_revision
       FROM spatial.layers l JOIN spatial.data_sources s ON
         s.organization_id=l.organization_id AND s.project_id=l.project_id AND s.id=l.source_id
       WHERE l.organization_id=:org AND l.project_id=:proj AND l.id=:layer
         AND s.id=:src AND l.lifecycle='active' AND s.lifecycle='active'
         AND s.validation_state='ready'
       FOR SHARE OF l,s
    """),
            args,
        )
        .mappings()
        .one_or_none()
    )
    if current is None or any(
        current[k] != identity[k] for k in ("layer_revision", "content_revision", "schema_revision")
    ):
        return identity | {"state": "stale"}
    attrs = session.execute(
        text("""
       SELECT attributes FROM spatial.reference_features
       WHERE organization_id=:org AND project_id=:proj AND source_id=:src
          AND feature_id=:fid AND content_revision=:content AND schema_revision=:schema
          AND geometry_state='present'
    """),
        args,
    ).scalar_one_or_none()
    if attrs is None:
        return identity | {"state": "stale"}
    names = session.execute(
        text("""
       SELECT field_id,name FROM spatial.source_fields
       WHERE organization_id=:org AND project_id=:proj AND source_id=:src
          AND schema_revision=:schema ORDER BY ordinal
    """),
        args,
    ).all()
    links = _linked_features(
        session, ctx, source_id=UUID(identity["source_id"]), feature_id=identity["feature_id"]
    )
    return identity | {
        "state": "current",
        "layer_name": current["layer_name"],
        "attributes": attrs,
        "field_names": dict(names),
        "cad_links": links,
    }


@router.get(BASE + "/{tileset_id}/feature-by-id")
def locate_tile_feature(
    organization_id: UUID,
    project_id: UUID,
    tileset_id: UUID,
    source_id: UUID,
    layer_id: UUID,
    feature_id: Annotated[str, Query(pattern=r"^[A-Za-z0-9_-]{1,128}$")],
    response: Response,
    auth: Annotated[AuthenticatedRequest, Depends(authenticate_request)],
    session: Annotated[Session, Depends(get_db_session, scope="function")],
):
    _scope(session, auth, organization_id, project_id, Permission.PROJECT_READ)
    rows = (
        session.execute(
            text("""
        SELECT c.uri, e.item->'first_triangle' AS triangle
        FROM spatial.tilesets t JOIN spatial.tileset_contents c ON
          c.organization_id=t.organization_id AND c.project_id=t.project_id AND c.tileset_id=t.id
        CROSS JOIN LATERAL jsonb_array_elements(c.feature_index) e(item)
        WHERE t.organization_id=:org AND t.project_id=:proj AND t.id=:id
          AND t.provenance->>'feature_picking'='true'
          AND t.provenance->>'source_id'=:src AND t.provenance->>'layer_id'=:layer
          AND e.item->>'feature_id'=:fid LIMIT 2
        """),
            {
                "org": organization_id,
                "proj": project_id,
                "id": tileset_id,
                "src": str(source_id),
                "layer": str(layer_id),
                "fid": feature_id,
            },
        )
        .mappings()
        .all()
    )
    if not rows:
        raise HTTPException(404, "Feature not indexed in this tileset")
    if len(rows) != 1 or type(rows[0]["triangle"]) is not int:
        raise HTTPException(409, "Feature index integrity failure")
    result = get_tile_feature(
        organization_id,
        project_id,
        tileset_id,
        rows[0]["uri"],
        rows[0]["triangle"],
        response,
        auth,
        session,
    )
    if result["feature_id"] != feature_id:
        raise HTTPException(409, "Feature index integrity failure")
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
