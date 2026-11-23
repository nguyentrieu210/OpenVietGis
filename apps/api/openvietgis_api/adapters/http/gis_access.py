"""Project-specific allow rules for GIS access. Unconfigured members retain existing roles."""
from __future__ import annotations
from datetime import datetime,UTC
from typing import Annotated,Literal,Any
from uuid import UUID,uuid4
import json,csv,io
from fastapi import APIRouter,Depends,HTTPException,Response,Query
from pydantic import BaseModel,Field,ConfigDict,model_validator
from sqlalchemy import text
from sqlalchemy.orm import Session
from openvietgis_api.adapters.http.authorization import context_for,require_permission
from openvietgis_api.adapters.http.security import AuthenticatedRequest,authenticate_request,require_csrf,get_db_session
from openvietgis_api.adapters.http.tenant_access import require_human_permission
from openvietgis_api.adapters.persistence.scope_helper import prepare_project_scope,append_project_audit
from openvietgis_api.adapters.persistence.gis_access import scoped,source_allowed,feature_allowed
from openvietgis_api.domains.authorization.permissions import Permission

router=APIRouter(tags=["gis_access"])
BASE="/organizations/{organization_id}/projects/{project_id}/access"
Read=Annotated[AuthenticatedRequest,Depends(authenticate_request)]
Write=Annotated[AuthenticatedRequest,Depends(require_csrf)]
DB=Annotated[Session,Depends(get_db_session,scope="function")]
Actions=Literal["read","create","geometry","attributes","delete","export","approve"]

def scope(db,auth,org,proj,admin=False,write=False):
    if admin: require_human_permission(db,auth,org,Permission.AUTHORIZATION_MANAGE_GRANTS,lock=write)
    ctx=context_for(auth,organization_id=org,project_id=proj)
    prepare_project_scope(db,ctx,permission=Permission.PROJECT_MANAGE if admin and write else Permission.PROJECT_READ)
    return ctx,{"org":org,"proj":proj}

def rows(db,sql,params):
    return [dict(r) for r in db.execute(text(sql),params).mappings()]

def log(db,ctx,action,id,details):
    append_project_audit(db,ctx,audit_id=uuid4(),action="gis_access."+action,resource_type="gis_access",resource_id=id,metadata={"project_id":str(ctx.project_id),**details},created_at=datetime.now(UTC))

def restrict(db,p,uid):
    member=db.execute(text("SELECT role,status FROM org.organization_memberships WHERE organization_id=:org AND user_id=:uid"),p|{"uid":uid}).mappings().first()
    if not member or member["status"]!="active" or member["role"] in ("owner","admin"): raise HTTPException(422,"Only active non-admin organization members may be scoped")
    db.execute(text("INSERT INTO gis_access.scoped_members(organization_id,project_id,user_id) VALUES(:org,:proj,:uid) ON CONFLICT DO NOTHING"),p|{"uid":uid})

class GroupBody(BaseModel):
    model_config=ConfigDict(extra="forbid")
    name:str=Field(min_length=1,max_length=160)
    user_ids:list[UUID]=Field(default_factory=list,max_length=500)

class RuleBody(BaseModel):
    model_config=ConfigDict(extra="forbid")
    user_id:UUID|None=None
    group_id:UUID|None=None
    layer_id:UUID
    actions:list[Actions]=Field(min_length=1,max_length=7)
    feature_ids:list[str]|None=Field(default=None,max_length=50000)
    attribute_filter:dict[str,Any]|None=None
    region:dict[str,Any]|None=None
    @model_validator(mode="after")
    def validate_rule(self):
        if (self.user_id is None)==(self.group_id is None):raise ValueError("choose one subject")
        if "read" not in self.actions:raise ValueError("read permission required")
        if self.feature_ids is not None and (not self.feature_ids or any(not f or len(f)>128 for f in self.feature_ids)):raise ValueError("invalid feature IDs")
        if self.attribute_filter is not None and (not self.attribute_filter or len(json.dumps(self.attribute_filter))>10000):raise ValueError("invalid attribute filter")
        if self.region is not None and (self.region.get("type") not in ("Polygon","MultiPolygon") or len(json.dumps(self.region))>100000):raise ValueError("region must be polygon")
        return self

@router.get(BASE)
def get_access(organization_id:UUID,project_id:UUID,auth:Read,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id,admin=True)
    return {
        "members":rows(db,"""SELECT m.id,m.user_id,m.role,m.status,u.display_name,u.email_snapshot,
          EXISTS(SELECT 1 FROM gis_access.scoped_members sm WHERE sm.organization_id=:org AND sm.project_id=:proj AND sm.user_id=m.user_id) AS restricted
          FROM org.organization_memberships m JOIN identity.users u ON u.id=m.user_id WHERE m.organization_id=:org ORDER BY u.display_name""",p),
        "groups":rows(db,"""SELECT g.id,g.name,ARRAY(SELECT gm.user_id FROM gis_access.group_members gm WHERE gm.organization_id=:org AND gm.project_id=:proj AND gm.group_id=g.id) AS user_ids
          FROM gis_access.groups g WHERE organization_id=:org AND project_id=:proj ORDER BY name""",p),
        "layers":rows(db,"""SELECT l.id,l.name,l.source_id,COALESCE((SELECT jsonb_agg(jsonb_build_object('id',f.field_id,'name',f.name,'type',f.type) ORDER BY f.ordinal) FROM spatial.source_fields f WHERE f.organization_id=:org AND f.project_id=:proj AND f.source_id=l.source_id),'[]'::jsonb) AS fields FROM spatial.layers l WHERE l.organization_id=:org AND l.project_id=:proj AND l.lifecycle='active' ORDER BY l.name""",p),
        "rules":rows(db,"""SELECT id,user_id,group_id,layer_id,actions,feature_ids,attribute_filter,CAST(ST_AsGeoJSON(region) AS jsonb) AS region,created_at
          FROM gis_access.rules WHERE organization_id=:org AND project_id=:proj AND revoked_at IS NULL ORDER BY created_at DESC""",p),
        "audit":rows(db,"""SELECT id,action,actor_display_snapshot,metadata,created_at FROM audit.audit_events
          WHERE organization_id=:org AND action LIKE 'gis_access.%' AND metadata->>'project_id'=:project_text ORDER BY created_at DESC LIMIT 100""",p|{"project_text":str(project_id)})}

@router.get(BASE+"/mine")
def mine(organization_id:UUID,project_id:UUID,auth:Read,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id)
    limited=scoped(db,ctx)
    return {"restricted":limited,"rules":rows(db,"""SELECT l.id AS layer_id,l.name,r.actions,r.feature_ids,r.attribute_filter,CAST(ST_AsGeoJSON(r.region) AS jsonb) AS region FROM gis_access.subject_rules(:org,:proj) r JOIN spatial.layers l ON l.organization_id=:org AND l.project_id=:proj AND l.id=r.layer_id""",p) if limited else []}

@router.post(BASE+"/groups",status_code=201)
def create_group(organization_id:UUID,project_id:UUID,body:GroupBody,auth:Write,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id,admin=True,write=True)
    gid=uuid4()
    if db.execute(text("SELECT 1 FROM gis_access.groups WHERE organization_id=:org AND project_id=:proj AND name=:name"),p|{"name":body.name.strip()}).scalar():raise HTTPException(409,"group already exists")
    db.execute(text("INSERT INTO gis_access.groups(organization_id,project_id,id,name) VALUES(:org,:proj,:gid,:name)"),p|{"gid":gid,"name":body.name.strip()})
    for uid in set(body.user_ids):
        restrict(db,p,uid)
        db.execute(text("INSERT INTO gis_access.group_members(organization_id,project_id,group_id,user_id) VALUES(:org,:proj,:gid,:uid)"),p|{"gid":gid,"uid":uid})
    log(db,ctx,"group_created",gid,{"name":body.name,"members":len(body.user_ids)})
    return {"id":gid}

@router.put(BASE+"/groups/{group_id}")
def replace_group(organization_id:UUID,project_id:UUID,group_id:UUID,body:GroupBody,auth:Write,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id,admin=True,write=True)
    p=p|{"gid":group_id}
    if not db.execute(text("SELECT 1 FROM gis_access.groups WHERE organization_id=:org AND project_id=:proj AND id=:gid"),p).scalar():raise HTTPException(404,"group not found")
    if db.execute(text("SELECT 1 FROM gis_access.groups WHERE organization_id=:org AND project_id=:proj AND name=:name AND id<>:gid"),p|{"name":body.name.strip()}).scalar():raise HTTPException(409,"group name already exists")
    db.execute(text("UPDATE gis_access.groups SET name=:name WHERE organization_id=:org AND project_id=:proj AND id=:gid"),p|{"name":body.name.strip()})
    db.execute(text("DELETE FROM gis_access.group_members WHERE organization_id=:org AND project_id=:proj AND group_id=:gid"),p)
    for uid in set(body.user_ids):
        restrict(db,p,uid)
        db.execute(text("INSERT INTO gis_access.group_members(organization_id,project_id,group_id,user_id) VALUES(:org,:proj,:gid,:uid)"),p|{"uid":uid})
    log(db,ctx,"group_updated",group_id,{"name":body.name,"members":len(body.user_ids)})
    return {"id":group_id}

def _insert_rule(db,ctx,p,body:RuleBody,*,skip_identical=False):
    # A batch uses the exact same validation and audit path as a single grant.
    sid=db.execute(text("SELECT source_id FROM spatial.layers WHERE organization_id=:org AND project_id=:proj AND id=:lid AND lifecycle='active'"),p|{"lid":body.layer_id}).scalar_one_or_none()
    if not sid:raise HTTPException(404,"layer not found")
    if body.user_id:restrict(db,p,body.user_id)
    if body.group_id and not db.execute(text("SELECT 1 FROM gis_access.groups WHERE organization_id=:org AND project_id=:proj AND id=:gid"),p|{"gid":body.group_id}).scalar():raise HTTPException(404,"group not found")
    if body.feature_ids:
        count=db.execute(text("SELECT count(*) FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj AND source_id=:sid AND feature_id=ANY(CAST(:ids AS text[]))"),p|{"sid":sid,"ids":body.feature_ids}).scalar()
        if count!=len(set(body.feature_ids)):raise HTTPException(422,"feature IDs missing from layer")
    if body.attribute_filter:
        available=set(db.execute(text("SELECT field_id FROM spatial.source_fields WHERE organization_id=:org AND project_id=:proj AND source_id=:sid"),p|{"sid":sid}).scalars())
        if not set(body.attribute_filter).issubset(available):raise HTTPException(422,"attribute filter field missing")
    rid=uuid4()
    p=p|{"id":rid,"uid":body.user_id,"gid":body.group_id,"lid":body.layer_id,"sid":sid,"actions":list(set(body.actions)),"ids":body.feature_ids,"attrs":json.dumps(body.attribute_filter) if body.attribute_filter is not None else None,"region":json.dumps(body.region) if body.region is not None else None}
    if body.region:
        try:
            valid=db.execute(text("""SELECT ST_IsValid(g) AND NOT ST_IsEmpty(g) AND ST_CoveredBy(g,ST_MakeEnvelope(-180,-90,180,90,4326))
              FROM (SELECT ST_SetSRID(ST_GeomFromGeoJSON(:region),4326) AS g) v"""),p).scalar()
        except Exception:raise HTTPException(422,"invalid region")
        if not valid:raise HTTPException(422,"invalid region")
    if skip_identical:
        existing=db.execute(text("""SELECT id FROM gis_access.rules
          WHERE organization_id=:org AND project_id=:proj AND layer_id=:lid AND revoked_at IS NULL
          AND user_id IS NOT DISTINCT FROM CAST(:uid AS uuid) AND group_id IS NOT DISTINCT FROM CAST(:gid AS uuid)
          AND actions @> CAST(:actions AS text[]) AND actions <@ CAST(:actions AS text[])
          AND ((feature_ids IS NULL AND CAST(:ids AS text[]) IS NULL)
               OR (feature_ids @> CAST(:ids AS text[]) AND feature_ids <@ CAST(:ids AS text[])))
          AND attribute_filter IS NOT DISTINCT FROM CAST(:attrs AS jsonb)
          AND ((region IS NULL AND CAST(:region AS text) IS NULL)
               OR (region IS NOT NULL AND CAST(:region AS text) IS NOT NULL AND
                   ST_Equals(region,ST_SetSRID(ST_GeomFromGeoJSON(CAST(:region AS text)),4326))))
          LIMIT 1"""),p).scalar_one_or_none()
        if existing:return existing,False
    db.execute(text("""INSERT INTO gis_access.rules(organization_id,project_id,id,user_id,group_id,layer_id,source_id,actions,feature_ids,attribute_filter,region)
      VALUES(:org,:proj,:id,:uid,:gid,:lid,:sid,CAST(:actions AS text[]),CAST(:ids AS text[]),CAST(:attrs AS jsonb),
      CASE WHEN CAST(:region AS text) IS NULL THEN NULL ELSE ST_Multi(ST_SetSRID(ST_GeomFromGeoJSON(CAST(:region AS text)),4326)) END)"""),p)
    log(db,ctx,"rule_created",rid,{"layer_id":str(body.layer_id),"actions":body.actions,"user_id":str(body.user_id) if body.user_id else None,"group_id":str(body.group_id) if body.group_id else None})
    return rid,True

@router.post(BASE+"/rules",status_code=201)
def create_rule(organization_id:UUID,project_id:UUID,body:RuleBody,auth:Write,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id,admin=True,write=True)
    rid,_=_insert_rule(db,ctx,p,body)
    return {"id":rid}

class BatchRulesBody(BaseModel):
    model_config=ConfigDict(extra="forbid")
    rules:list[RuleBody]=Field(default_factory=list,max_length=200)
    revoke_ids:list[UUID]=Field(default_factory=list,max_length=200)
    @model_validator(mode="after")
    def validate_batch(self):
        if not self.rules and not self.revoke_ids:raise ValueError("select at least one rule")
        if self.rules and self.revoke_ids:raise ValueError("grant and revoke separately")
        if len(self.model_dump_json())>2_000_000:raise ValueError("batch payload too large")
        return self

@router.post(BASE+"/rules/batch")
def batch_rules(organization_id:UUID,project_id:UUID,body:BatchRulesBody,auth:Write,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id,admin=True,write=True)
    created=0;unchanged=0
    for rule in body.rules:
        _,added=_insert_rule(db,ctx,p,rule,skip_identical=True)
        created+=int(added);unchanged+=int(not added)
    for rid in dict.fromkeys(body.revoke_ids):
        _revoke_rule(db,ctx,p,rid)
    return {"created":created,"unchanged":unchanged,"revoked":len(set(body.revoke_ids))}

def _revoke_rule(db,ctx,p,rule_id):
    updated=db.execute(text("UPDATE gis_access.rules SET revoked_at=now() WHERE organization_id=:org AND project_id=:proj AND id=:id AND revoked_at IS NULL"),p|{"id":rule_id})
    if updated.rowcount!=1:raise HTTPException(404,"rule not found")
    log(db,ctx,"rule_revoked",rule_id,{})

@router.delete(BASE+"/rules/{rule_id}",status_code=204)
def revoke_rule(organization_id:UUID,project_id:UUID,rule_id:UUID,auth:Write,db:DB):
    ctx,p=scope(db,auth,organization_id,project_id,admin=True,write=True)
    _revoke_rule(db,ctx,p,rule_id)

@router.get(BASE+"/export/{layer_id}")
def export_layer(organization_id:UUID,project_id:UUID,layer_id:UUID,auth:Read,db:DB,format:Literal["geojson","csv"]="geojson"):
    ctx,p=scope(db,auth,organization_id,project_id)
    sid=db.execute(text("SELECT source_id FROM spatial.layers WHERE organization_id=:org AND project_id=:proj AND id=:lid"),p|{"lid":layer_id}).scalar_one_or_none()
    if not sid:raise HTTPException(404,"layer not found")
    if not source_allowed(db,ctx,sid,"export"):raise HTTPException(403,"export not granted")
    data=rows(db,"""SELECT feature_id,attributes,CAST(ST_AsGeoJSON(ST_Transform(geometry,4326)) AS jsonb) AS geometry
        FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj AND source_id=:sid
        AND gis_access.feature_allowed(:org,:proj,:sid,feature_id,attributes,geometry,'export') LIMIT 50001""",p|{"sid":sid})
    if len(data)>50000:raise HTTPException(422,"download limit 50000 features")
    if format=="geojson":
        result=json.dumps({"type":"FeatureCollection","features":[{"type":"Feature","id":r["feature_id"],"properties":r["attributes"],"geometry":r["geometry"]} for r in data]},ensure_ascii=False)
        return Response(result,media_type="application/geo+json",headers={"Cache-Control":"no-store","Content-Disposition":"attachment; filename=features.geojson"})
    keys=sorted(set().union(*(r["attributes"].keys() for r in data))) if data else []
    def safe(v):
        v=str(v) if v is not None else ""
        return "'"+v if v.startswith(("=","+","-","@","\t","\r")) else v
    out=io.StringIO()
    writer=csv.writer(out)
    writer.writerow(["feature_id",*[safe(k) for k in keys],"geometry"])
    for r in data:writer.writerow([safe(r["feature_id"]),*[safe(r["attributes"].get(k)) for k in keys],json.dumps(r["geometry"])])
    return Response(out.getvalue(),media_type="text/csv",headers={"Cache-Control":"no-store","Content-Disposition":"attachment; filename=features.csv"})

# Review applies to already committed changesets, never grants an edit permission.
REVIEW_CHECK="""NOT EXISTS(SELECT 1 FROM editing.changeset_operations o WHERE o.organization_id=:org AND o.project_id=:proj AND o.changeset_id=c.id AND (
  (o.op_type<>'create' AND NOT (gis_access.feature_allowed(:org,:proj,c.source_id,o.feature_id,o.before_properties,ST_GeomFromEWKB(o.before_geometry_wkb),'approve') AND gis_access.feature_allowed(:org,:proj,c.source_id,o.feature_id,o.before_properties,ST_GeomFromEWKB(o.before_geometry_wkb),'read')))
  OR (o.op_type<>'delete' AND NOT (gis_access.feature_allowed(:org,:proj,c.source_id,o.feature_id,o.after_properties,ST_GeomFromEWKB(o.after_geometry_wkb),'approve') AND gis_access.feature_allowed(:org,:proj,c.source_id,o.feature_id,o.after_properties,ST_GeomFromEWKB(o.after_geometry_wkb),'read')))
))"""
def review_context(db,auth,org,proj):
    ctx,p=scope(db,auth,org,proj)
    if auth.actor.actor_type!="human":raise HTTPException(403,"human reviewer required")
    if not scoped(db,ctx):require_permission(db,ctx,Permission.PROJECT_MANAGE)
    return ctx,p

@router.get(BASE+"/reviews")
def list_reviews(organization_id:UUID,project_id:UUID,auth:Read,db:DB):
    ctx,p=review_context(db,auth,organization_id,project_id)
    return {"items":rows(db,"""SELECT c.id,c.source_id,c.summary,c.actor_display_snapshot,c.operation_count,c.created_at,
       ds.name AS source_name,
       (SELECT jsonb_agg(jsonb_build_object('decision',r.decision,'note',r.note,'created_at',r.created_at,'reviewer_id',r.reviewer_id) ORDER BY r.created_at DESC)
         FROM gis_access.reviews r WHERE r.organization_id=:org AND r.project_id=:proj AND r.changeset_id=c.id) AS decisions
       FROM editing.changesets c JOIN spatial.data_sources ds ON ds.organization_id=:org AND ds.project_id=:proj AND ds.id=c.source_id
       WHERE c.organization_id=:org AND c.project_id=:proj AND """+REVIEW_CHECK+" ORDER BY c.created_at DESC LIMIT 100",p)}

class DecisionBody(BaseModel):
    model_config=ConfigDict(extra="forbid")
    decision:Literal["approved","rejected"]
    note:str=Field(default="",max_length=500)

@router.post(BASE+"/reviews/{changeset_id}",status_code=201)
def decide(organization_id:UUID,project_id:UUID,changeset_id:UUID,body:DecisionBody,auth:Write,db:DB):
    ctx,p=review_context(db,auth,organization_id,project_id)
    p=p|{"cid":changeset_id}
    rec=db.execute(text("SELECT c.id,c.actor_id FROM editing.changesets c WHERE c.organization_id=:org AND c.project_id=:proj AND c.id=:cid AND "+REVIEW_CHECK),p).mappings().first()
    if not rec:raise HTTPException(403,"changeset outside review scope")
    if rec["actor_id"]==auth.actor.actor_id:raise HTTPException(422,"cannot review own changeset")
    rid=uuid4()
    db.execute(text("INSERT INTO gis_access.reviews(organization_id,project_id,id,changeset_id,reviewer_id,decision,note) VALUES(:org,:proj,:id,:cid,:uid,:decision,:note)"),p|{"id":rid,"uid":auth.actor.actor_id,"decision":body.decision,"note":body.note.strip()})
    log(db,ctx,"change_reviewed",rid,{"changeset_id":str(changeset_id),"decision":body.decision})
    return {"id":rid}
