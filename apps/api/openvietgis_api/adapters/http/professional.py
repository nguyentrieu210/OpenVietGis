"""Revision-pinned GIS tools. Reads and previews never mutate stored geometry."""
from __future__ import annotations

import hashlib
import json
import math
from dataclasses import replace
from datetime import UTC, datetime
from typing import Annotated, Literal
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Query, Response
from pydantic import BaseModel, ConfigDict, Field, StringConstraints
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.http.security import AuthenticatedRequest, authenticate_request, get_db_session, require_csrf
from openvietgis_api.adapters.persistence.spatial_repository import SqlSpatialRepository
from openvietgis_api.adapters.persistence.gis_access import scoped
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.spatial.domain import (
    SourceKind, Lifecycle, NativeGeometry, SpatialValidationError, validate_ready_geometry,
    FieldType, SourceField, CatalogOperation, schema_revision_digest,
)
from openvietgis_api.domains.spatial.service import build_mutation_evidence, next_revision, validate_feature_id

router = APIRouter(tags=["professional-gis"])
BASE = "/organizations/{organization_id}/projects/{project_id}/pro"
Pin = Annotated[str, StringConstraints(pattern=r"^[a-f0-9]{64}$")]
FID = Annotated[str, StringConstraints(pattern=r"^[A-Za-z0-9_-]{1,128}$")]
Name = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=200)]


class Selection(BaseModel):
    model_config = ConfigDict(extra="forbid")
    layer_id: UUID
    content_revision: Pin
    schema_revision: Pin
    feature_ids: Annotated[list[FID], Field(max_length=100)] = Field(default_factory=list)


def scope(session, auth, org, project, edit=False):
    context = context_for(auth, organization_id=org, project_id=project)
    repo = SqlSpatialRepository(session)
    repo.prepare_scope(context=context, permission=Permission.PROJECT_EDIT if edit else Permission.PROJECT_READ, mutation=edit)
    session.execute(text("SET LOCAL statement_timeout = '12s'"))
    return context, repo


def pinned(repo, ctx, spec, editable=False):
    layer = repo.get_layer(context=ctx, layer_id=spec.layer_id, lock=True)
    if not layer or layer.lifecycle != Lifecycle.ACTIVE:
        raise HTTPException(404, "Layer not found")
    source = repo.get_source(context=ctx, source_id=layer.source_id, lock=True)
    if not source or not source.readable:
        raise HTTPException(422, "Source is not ready")
    if (source.content_revision, source.schema_revision) != (spec.content_revision, spec.schema_revision):
        raise HTTPException(409, "Source revision changed; reload")
    if source.stored_crs != 4326:
        raise HTTPException(422, "Tools currently require native EPSG:4326 XY")
    if editable and source.kind == SourceKind.REFERENCE_FIXTURE:
        raise HTTPException(403, "Reference source is read-only")
    if len(spec.feature_ids) != len(set(spec.feature_ids)):
        raise HTTPException(422, "Duplicate feature identifiers")
    return layer, source


def params(ctx, src, spec, suffix=""):
    return {
        "org":ctx.organization_id,"project":ctx.project_id,
        "src"+suffix:src.id,"ids"+suffix:spec.feature_ids,
    }


def selection_sql(suffix="", chosen=True):
    return "SELECT feature_id, geometry, attributes FROM spatial.reference_features WHERE organization_id=:org AND project_id=:project AND source_id=:src"+suffix+" AND geometry_state='present'"+(" AND feature_id=ANY(:ids"+suffix+")" if chosen else "")


def rows(session, sql, values):
    try:
        return session.execute(text(sql), values).mappings().all()
    except DBAPIError as exc:
        raise HTTPException(422, "Spatial operation failed or exceeded its time budget") from exc


class GeometryRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    operation: Literal["move_batch","copy","offset","rotate","scale","stretch","extend","trim","fillet","intersection","union","difference","centroid","hull","simplify","nearest","spatial_join"]
    primary: Selection
    secondary: Selection | None = None
    distance: Annotated[float, Field(ge=-100000, le=100000, allow_inf_nan=False)] = 0
    dx: Annotated[float, Field(ge=-100000, le=100000, allow_inf_nan=False)] = 0
    dy: Annotated[float, Field(ge=-100000, le=100000, allow_inf_nan=False)] = 0
    angle: Annotated[float, Field(ge=-360, le=360, allow_inf_nan=False)] = 0
    factor: Annotated[float, Field(gt=0, le=100, allow_inf_nan=False)] = 1
    vertex: Annotated[int, Field(ge=0, le=4096)] = 1


CAD = frozenset(("move_batch","copy","offset","rotate","scale","stretch","extend","trim","fillet"))
BINARY = frozenset(("trim","intersection","union","difference","nearest","spatial_join"))


def fillet_points(points, index, radius):
    """Tangent arc in projected metres. Only replaces a single interior vertex."""
    if not 0 < index < len(points)-1 or radius <= 0:
        raise HTTPException(422, "Fillet requires an interior vertex and positive radius")
    a,b,c = points[index-1:index+2]
    u=(a[0]-b[0],a[1]-b[1]); v=(c[0]-b[0],c[1]-b[1])
    lu,lv=math.hypot(*u),math.hypot(*v)
    if not lu or not lv:
        raise HTTPException(422, "Repeated vertices")
    u=(u[0]/lu,u[1]/lu); v=(v[0]/lv,v[1]/lv)
    theta=math.acos(max(-1,min(1,u[0]*v[0]+u[1]*v[1])))
    if theta<.01 or theta>math.pi-.01:
        raise HTTPException(422, "Fillet requires a valid corner")
    tangent=radius/math.tan(theta/2)
    if tangent>=min(lu,lv):
        raise HTTPException(422, "Radius exceeds adjacent segments")
    bis=(u[0]+v[0],u[1]+v[1]); norm=math.hypot(*bis)
    center=(b[0]+bis[0]/norm*radius/math.sin(theta/2), b[1]+bis[1]/norm*radius/math.sin(theta/2))
    p=(b[0]+u[0]*tangent,b[1]+u[1]*tangent); q=(b[0]+v[0]*tangent,b[1]+v[1]*tangent)
    start=math.atan2(p[1]-center[1],p[0]-center[0]); end=math.atan2(q[1]-center[1],q[0]-center[0])
    delta=(end-start+math.pi)%(2*math.pi)-math.pi
    count=max(4,math.ceil(abs(delta)/(math.pi/36)))
    arc=[[center[0]+radius*math.cos(start+delta*i/count),center[1]+radius*math.sin(start+delta*i/count)] for i in range(count+1)]
    return points[:index]+arc+points[index+1:]


@router.post(BASE+"/geometry")
def geometry_preview(organization_id: UUID, project_id: UUID, body: GeometryRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")]):
    op=body.operation
    ctx,repo=scope(session,auth,organization_id,project_id,op in CAD)
    layer,src=pinned(repo,ctx,body.primary,op in CAD)
    if not body.primary.feature_ids or (op in CAD and op!="move_batch" and len(body.primary.feature_ids)!=1):
        raise HTTPException(422,"Select a feature; CAD needs one feature")
    if op == "move_batch":
        if scoped(session, ctx):
            raise HTTPException(403, "Batch move requires project-wide edit access")
        if body.secondary or (body.dx == 0 and body.dy == 0):
            raise HTTPException(422, "Select a nonzero vector and one source layer")
    if op in BINARY and not body.secondary:
        raise HTTPException(422,"Select a second layer and feature(s)")
    p=params(ctx,src,body.primary)
    primary=selection_sql()
    if len(rows(session,"SELECT feature_id FROM ("+primary+") q",p))!=len(body.primary.feature_ids):
        raise HTTPException(404,"Selected feature missing")
    secondary=""
    if body.secondary:
        _,target=pinned(repo,ctx,body.secondary)
        p.update(params(ctx,target,body.secondary,"2"))
        if not body.secondary.feature_ids and (target.feature_count or 0)>2000 and op!="nearest":
            raise HTTPException(422,"Select target features; whole-layer operation limited to 2000")
        secondary=selection_sql("2",bool(body.secondary.feature_ids))
        if body.secondary.feature_ids and len(rows(session,"SELECT feature_id FROM ("+secondary+") q",p))!=len(body.secondary.feature_ids):
            raise HTTPException(404,"Target feature missing")
    p.update({"dx":body.dx,"dy":body.dy,"angle":body.angle,"factor":body.factor,"distance":body.distance,"vertex":body.vertex})
    line_ops={"offset","trim","extend","fillet","stretch"}
    if op in line_ops and src.geometry_family.value!="LineString":
        raise HTTPException(422,"This tool requires a LineString")
    if op in ("offset","simplify","extend","fillet") and (body.distance==0 or (op!="offset" and body.distance<0)):
        raise HTTPException(422,"Enter a nonzero valid distance")
    # Metric UTM based on the selected source geometry. Reject geographic extents
    # wider than a local UTM zone to avoid claiming incorrect CAD precision.
    cte="WITH a AS ("+primary+"), u AS (SELECT ST_Centroid(ST_Collect(geometry)) AS g, ST_XMax(ST_Extent(geometry))-ST_XMin(ST_Extent(geometry)) AS span FROM a), crs AS (SELECT (CASE WHEN ST_Y(g)>=0 THEN 32600 ELSE 32700 END)+LEAST(60,GREATEST(1,FLOOR((ST_X(g)+180)/6)::int+1)) AS srid FROM u WHERE span<=6 AND ST_Y(g) BETWEEN -80 AND 84), m AS (SELECT feature_id,ST_Transform(geometry,srid) AS g,srid FROM a CROSS JOIN crs)"
    if op=="move_batch":
        q=rows(session,cte+" , result AS (SELECT feature_id,ST_Transform(ST_Translate(g,:dx,:dy),4326) AS g FROM m) SELECT feature_id,ST_AsGeoJSON(g,15) AS geo,ST_IsValid(g) AS valid,ST_NPoints(g) AS n FROM result ORDER BY feature_id",p)
        if len(q)!=len(body.primary.feature_ids) or any(not x["valid"] or x["n"]>4096 for x in q):
            raise HTTPException(422,"Invalid or unsupported batch extent")
        items=[]
        for x in q:
            g=json.loads(x["geo"])
            if g["type"]!=src.geometry_family.value:
                raise HTTPException(422,"Batch move must preserve geometry type")
            try:
                validate_ready_geometry(NativeGeometry(type=g["type"],coordinates=g["coordinates"],srid=4326))
            except SpatialValidationError as exc:
                raise HTTPException(422,"Batch move result cannot be stored") from exc
            items.append({"feature_id":x["feature_id"],"geometry":g})
        return {"source_id":src.id,"content_revision":src.content_revision,"items":items}
    if op=="fillet":
        r=rows(session,cte+" SELECT ST_AsGeoJSON(g,9) AS g,srid AS srid FROM m",p)
        if len(r)!=1: raise HTTPException(422,"Unsupported metric extent")
        v=fillet_points(json.loads(r[0]["g"])["coordinates"],body.vertex,body.distance)
        p.update({"geo":json.dumps({"type":"LineString","coordinates":v}),"srid":r[0]["srid"]})
        q=rows(session,"SELECT ST_AsGeoJSON(ST_Transform(ST_SetSRID(ST_GeomFromGeoJSON(:geo),:srid),4326),15) AS geo",p)
        result=[json.loads(x["geo"]) for x in q]
    elif op in ("nearest","spatial_join"):
        relation="TRUE" if op=="nearest" else "ST_Intersects(a.geometry,b.geometry)"
        q=rows(session,"WITH a AS ("+primary+"), b AS ("+secondary+") SELECT a.feature_id AS source_fid,b.feature_id AS target_fid,ST_Distance(a.geometry::geography,b.geometry::geography) AS distance_m,ST_AsGeoJSON(a.geometry,15) AS geo FROM a CROSS JOIN LATERAL (SELECT b.* FROM b WHERE "+relation+" AND NOT (b.feature_id=a.feature_id AND :src=:src2) ORDER BY ST_Distance(a.geometry::geography,b.geometry::geography),b.feature_id LIMIT "+("1" if op=="nearest" else "101")+") b LIMIT 101",p)
        if len(q)>100: raise HTTPException(422,"More than 100 results; narrow the selection")
        return {"source_id":src.id,"content_revision":src.content_revision,"items":[{"geometry":json.loads(x["geo"]),"source_fid":x["source_fid"],"target_fid":x["target_fid"],"distance_m":x["distance_m"]} for x in q]}
    else:
        metric={
            "copy":"ST_Translate(g,:dx,:dy)",
            "offset":"ST_OffsetCurve(g,:distance)",
            "rotate":"ST_Rotate(g,radians(:angle),ST_Centroid(g))",
            "scale":"ST_Translate(ST_Scale(ST_Translate(g,-ST_X(ST_Centroid(g)),-ST_Y(ST_Centroid(g))),:factor,:factor),ST_X(ST_Centroid(g)),ST_Y(ST_Centroid(g)))",
            "stretch":"CASE WHEN :vertex<ST_NPoints(g) THEN ST_SetPoint(g,:vertex,ST_Translate(ST_PointN(g,:vertex+1),:dx,:dy)) END",
            "extend":"ST_AddPoint(g,ST_Translate(ST_EndPoint(g),:distance*cos(ST_Azimuth(ST_PointN(g,ST_NPoints(g)-1),ST_EndPoint(g))-pi()/2),:distance*sin(pi()/2-ST_Azimuth(ST_PointN(g,ST_NPoints(g)-1),ST_EndPoint(g)))))",
            "simplify":"ST_SimplifyPreserveTopology(g,:distance)",
        }
        if op=="extend":
            metric[op]="ST_SetPoint(g,ST_NPoints(g)-1,ST_Translate(ST_EndPoint(g),:distance*sin(ST_Azimuth(ST_PointN(g,ST_NPoints(g)-1),ST_EndPoint(g))),:distance*cos(ST_Azimuth(ST_PointN(g,ST_NPoints(g)-1),ST_EndPoint(g)))))"
        if op in metric:
            statement=cte+" , result AS (SELECT ST_Transform("+metric[op]+",4326) AS g FROM m)"
        elif op=="trim":
            statement="WITH a AS ("+primary+"), b AS ("+secondary+"), cut AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS cutter FROM b), pieces AS (SELECT (ST_Dump(ST_Split(a.geometry,CASE WHEN ST_Dimension(cutter)=2 THEN ST_Boundary(cutter) ELSE cutter END))).geom AS g,a.geometry AS original FROM a CROSS JOIN cut), result AS (SELECT g FROM pieces WHERE ST_GeometryType(g)='ST_LineString' AND (SELECT count(*) FROM pieces)>1 ORDER BY ST_Distance(g,ST_StartPoint(original)),ST_Length(g) DESC LIMIT 1)"
        elif op in ("intersection","difference","union"):
            fn={"intersection":"ST_Intersection","difference":"ST_Difference","union":"ST_Union"}[op]
            statement="WITH a AS ("+primary+"), b AS ("+secondary+"), u AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM a), v AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM b), result AS (SELECT "+fn+"(u.g,v.g) AS g FROM u CROSS JOIN v)"
        else:
            fn="ST_Centroid" if op=="centroid" else "ST_ConvexHull"
            statement="WITH a AS ("+primary+"), result AS (SELECT "+fn+"(ST_UnaryUnion(ST_Collect(geometry))) AS g FROM a)"
        q=rows(session,statement+", parts AS (SELECT (ST_Dump(g)).geom AS g FROM result WHERE g IS NOT NULL AND NOT ST_IsEmpty(g)) SELECT ST_AsGeoJSON(g,15) AS geo,ST_IsValid(g) AS valid,ST_NPoints(g) AS n FROM parts LIMIT 101",p)
        if len(q)>100 or any(not x["valid"] or x["n"]>4096 for x in q):
            raise HTTPException(422,"Invalid or oversized result; narrow your selection")
        result=[json.loads(x["geo"]) for x in q]
    if not result:
        raise HTTPException(422,"No geometry result")
    if op in CAD and (len(result)!=1 or result[0]["type"]!=src.geometry_family.value):
        raise HTTPException(422,"CAD result must be one geometry of the same type")
    for g in result:
        try:
            validate_ready_geometry(NativeGeometry(type=g["type"],coordinates=g["coordinates"],srid=4326))
        except SpatialValidationError as exc:
            raise HTTPException(422,"Result cannot be stored as a supported simple XY geometry") from exc
    return {"source_id":src.id,"content_revision":src.content_revision,"items":[{"geometry":g} for g in result]}


class TopologyRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    check: Literal["duplicate","invalid","overlap","dangling","outside","gap"]
    primary: Selection
    secondary: Selection | None = None


@router.post(BASE+"/topology")
def topology(organization_id: UUID, project_id: UUID, body: TopologyRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")]):
    ctx,repo=scope(session,auth,organization_id,project_id)
    _,src=pinned(repo,ctx,body.primary)
    p=params(ctx,src,body.primary)
    a=selection_sql(chosen=bool(body.primary.feature_ids))
    check=body.check
    if check in ("outside","gap"):
        if not body.secondary: raise HTTPException(422,"Reference boundary layer required")
        _,target=pinned(repo,ctx,body.secondary)
        if target.geometry_family.value!="Polygon" or (target.feature_count or 0)>5000:
            raise HTTPException(422,"Reference boundary must be polygon with <=5000 objects")
        p.update(params(ctx,target,body.secondary,"2")); b=selection_sql("2",bool(body.secondary.feature_ids))
    if check=="duplicate":
        sql="WITH a AS ("+a+"), g AS (SELECT ST_AsEWKB(geometry) AS k,min(feature_id) AS keeper,count(*) AS n FROM a GROUP BY ST_AsEWKB(geometry) HAVING count(*)>1) SELECT a.feature_id,g.keeper AS other,ST_AsGeoJSON(a.geometry,15) AS geo FROM a JOIN g ON ST_AsEWKB(a.geometry)=g.k WHERE a.feature_id<>g.keeper"
    elif check=="invalid":
        sql="WITH a AS ("+a+") SELECT feature_id,NULL::text AS other,ST_AsGeoJSON(geometry,15) AS geo,ST_IsValidReason(geometry) AS reason FROM a WHERE NOT ST_IsValid(geometry)"
    elif check=="overlap":
        if src.geometry_family.value!="Polygon": raise HTTPException(422,"Polygon required")
        # Use spatial index on the underlying source, not a Cartesian CTE.
        sql="WITH a AS ("+a+") SELECT a.feature_id,b.feature_id AS other,ST_AsGeoJSON(ST_Intersection(a.geometry,b.geometry),15) AS geo FROM a JOIN spatial.reference_features b ON b.organization_id=:org AND b.project_id=:project AND b.source_id=:src AND b.feature_id>a.feature_id AND b.geometry_state='present' AND b.geometry && a.geometry WHERE ST_Relate(a.geometry,b.geometry,'2********')"
    elif check=="dangling":
        if src.geometry_family.value!="LineString": raise HTTPException(422,"LineString required")
        sql="WITH a AS ("+a+"), ends AS (SELECT feature_id,ST_StartPoint(geometry) AS p FROM a UNION ALL SELECT feature_id,ST_EndPoint(geometry) FROM a) SELECT e.feature_id,NULL::text AS other,ST_AsGeoJSON(e.p,15) AS geo FROM ends e WHERE NOT EXISTS (SELECT 1 FROM spatial.reference_features b WHERE b.organization_id=:org AND b.project_id=:project AND b.source_id=:src AND b.feature_id<>e.feature_id AND b.geometry && e.p AND ST_Intersects(b.geometry,e.p))"
    elif check=="outside":
        sql="WITH a AS ("+a+"), b AS ("+b+"), mask AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM b) SELECT a.feature_id,NULL::text AS other,ST_AsGeoJSON(a.geometry,15) AS geo FROM a CROSS JOIN mask WHERE NOT ST_CoveredBy(a.geometry,mask.g)"
    else:
        if src.geometry_family.value!="Polygon" or (src.feature_count or 0)>5000: raise HTTPException(422,"Gap check needs <=5000 polygons")
        sql="WITH a AS ("+a+"), b AS ("+b+"), coverage AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM a), mask AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM b), parts AS (SELECT (ST_Dump(ST_Difference(mask.g,coverage.g))).geom AS g FROM coverage CROSS JOIN mask) SELECT NULL::text AS feature_id,NULL::text AS other,ST_AsGeoJSON(g,15) AS geo FROM parts WHERE NOT ST_IsEmpty(g)"
    q=rows(session,sql+" LIMIT 201",p)
    return {"source_id":src.id,"content_revision":src.content_revision,"complete":len(q)<=200,"items":[{"feature_id":x["feature_id"],"other":x["other"],"geometry":json.loads(x["geo"]),**({"reason":x["reason"]} if check=="invalid" else {})} for x in q[:200]]}


class SchemaRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    primary: Selection
    action: Literal["add","rename"]
    field_id: Annotated[str,StringConstraints(pattern=r"^[A-Za-z][A-Za-z0-9_]{0,63}$")]
    name: Name
    type: Literal["text","decimal","integer","boolean","timestamp"]="text"


@router.post(BASE+"/schema")
def schema_change(organization_id: UUID, project_id: UUID, body: SchemaRequest,
    auth: Annotated[AuthenticatedRequest, Depends(require_csrf)],
    session: Annotated[Session, Depends(get_db_session, scope="function")]):
    ctx,repo=scope(session,auth,organization_id,project_id,True)
    _,src=pinned(repo,ctx,body.primary,True)
    fields=list(repo.get_fields(context=ctx,source_id=src.id))
    existing=next((f for f in fields if f.field_id==body.field_id),None)
    if any(f.name==body.name and f.field_id!=body.field_id for f in fields): raise HTTPException(422,"Field name exists")
    if body.action=="add":
        if existing or len(fields)>=100: raise HTTPException(422,"Field exists or schema full")
        fields.append(SourceField(organization_id,project_id,src.id,body.field_id,body.name,len(fields),FieldType(body.type),True,src.schema_revision))
    else:
        if not existing or existing.name==body.name: raise HTTPException(422,"Field not found or unchanged")
        fields=[replace(f,name=body.name) if f.field_id==body.field_id else f for f in fields]
    schema=schema_revision_digest(fields,src.geometry_family,src.dimensions,schema_state=src.schema_state)
    content=hashlib.sha256((src.content_revision+schema+"schema-v1").encode()).hexdigest()
    now=datetime.now(UTC)
    new=replace(src,content_revision=content,schema_revision=schema,catalog_revision=next_revision(src.catalog_revision,src.catalog_revision),updated_at=now)
    p={"org":organization_id,"project":project_id,"src":src.id,"schema":schema,"content":content,"id":body.field_id,"name":body.name,"type":body.type,"ordinal":len(fields)-1,"rev":new.catalog_revision,"now":now}
    session.execute(text("UPDATE spatial.source_fields SET schema_revision=:schema WHERE organization_id=:org AND project_id=:project AND source_id=:src"),p)
    if body.action=="add":
        session.execute(text("INSERT INTO spatial.source_fields(organization_id,project_id,source_id,field_id,name,ordinal,type,nullable,schema_revision) VALUES (:org,:project,:src,:id,:name,:ordinal,:type,true,:schema)"),p)
        session.execute(text("UPDATE spatial.reference_features SET attributes=attributes||jsonb_build_object(CAST(:id AS text),NULL::text) WHERE organization_id=:org AND project_id=:project AND source_id=:src"),p)
    else:
        session.execute(text("UPDATE spatial.source_fields SET name=:name WHERE organization_id=:org AND project_id=:project AND source_id=:src AND field_id=:id"),p)
    session.execute(text("UPDATE spatial.reference_features SET schema_revision=:schema,content_revision=:content WHERE organization_id=:org AND project_id=:project AND source_id=:src"),p)
    session.execute(text("UPDATE spatial.data_sources SET schema_revision=:schema,content_revision=:content,catalog_revision=:rev,updated_at=:now WHERE organization_id=:org AND project_id=:project AND id=:src"),p)
    ev=build_mutation_evidence(context=ctx,prior=src,current=new,operation=CatalogOperation.UPDATE_SCHEMA,now=now)
    repo.append_change(ev.change,ev.audit,context=ctx)
    return {"source_id":src.id,"schema_revision":schema,"content_revision":content,"field_id":body.field_id}


@router.get(BASE+"/tiles/{layer_id}/{z}/{x}/{y}.mvt")
def tile(organization_id: UUID, project_id: UUID, layer_id: UUID, z: int, x: int, y: int,
    content_revision: Annotated[str,Query(pattern=r"^[a-f0-9]{64}$")],
    schema_revision: Annotated[str,Query(pattern=r"^[a-f0-9]{64}$")],
    auth: Annotated[AuthenticatedRequest,Depends(authenticate_request)],
    session: Annotated[Session,Depends(get_db_session,scope="function")]):
    if not 0<=z<=22 or not 0<=x<2**z or not 0<=y<2**z: raise HTTPException(422,"Invalid tile")
    ctx,repo=scope(session,auth,organization_id,project_id)
    spec=Selection(layer_id=layer_id,content_revision=content_revision,schema_revision=schema_revision)
    _,src=pinned(repo,ctx,spec)
    p=params(ctx,src,spec)|{"z":z,"x":x,"y":y}
    base="WITH bounds AS (SELECT ST_TileEnvelope(:z,:x,:y) AS b), source AS (SELECT feature_id,ST_Transform(geometry,3857) AS g,b FROM spatial.reference_features CROSS JOIN bounds WHERE organization_id=:org AND project_id=:project AND source_id=:src AND geometry_state='present' AND geometry && ST_Transform(b,4326))"
    if src.geometry_family.value=="Point" and z<17:
        sql=base+", clusters AS (SELECT ST_SnapToGrid(g,(ST_XMax(b)-ST_XMin(b))/32) AS p,ST_Centroid(ST_Collect(g)) AS g,count(*) AS count,min(feature_id) AS fid,b FROM source GROUP BY p,b), tile AS (SELECT CASE WHEN count=1 THEN fid ELSE '' END AS feature_id,count AS cluster_count,ST_AsMVTGeom(g,b,4096,64,true) AS geom FROM clusters)"
    else:
        sql=base+", tile AS (SELECT feature_id,1 AS cluster_count,ST_AsMVTGeom(ST_SimplifyPreserveTopology(g,(ST_XMax(b)-ST_XMin(b))/4096),b,4096,64,true) AS geom FROM source ORDER BY feature_id LIMIT 20000)"
    q=rows(session,sql+" SELECT ST_AsMVT(tile,'features',4096,'geom') AS tile FROM tile",p)
    data=bytes(q[0]["tile"] or b"")
    return Response(data,media_type="application/vnd.mapbox-vector-tile",headers={"Cache-Control":"private, no-store"})
