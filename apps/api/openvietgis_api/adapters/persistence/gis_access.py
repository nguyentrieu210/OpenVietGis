"""Project-scoped access checks; rules are enforced by database RLS and the edit transaction."""
from __future__ import annotations
import json
from sqlalchemy import text
from openvietgis_api.domains.platform_errors import PlatformForbiddenError

def params(ctx):
    return {"org":ctx.organization_id,"proj":ctx.project_id}
def scoped(session,ctx):
    return bool(session.execute(text("SELECT gis_access.scoped(:org,:proj)"),params(ctx)).scalar())
def source_allowed(session,ctx,sid,action="read"):
    return bool(session.execute(text("SELECT gis_access.source_allowed(:org,:proj,:sid,:action)"),params(ctx)|{"sid":sid,"action":action}).scalar())
def feature_allowed(session,ctx,sid,fid,attrs,geometry,action="read"):
    return bool(session.execute(text("""SELECT gis_access.feature_allowed(:org,:proj,:sid,:fid,CAST(:attrs AS jsonb),
       CASE WHEN CAST(:wkb AS bytea) IS NULL THEN NULL ELSE ST_GeomFromEWKB(CAST(:wkb AS bytea)) END,:action)"""),
       params(ctx)|{"sid":sid,"fid":fid,"attrs":json.dumps(attrs or {}),"wkb":geometry,"action":action}).scalar())
def deny():
    raise PlatformForbiddenError("Đối tượng hoặc thao tác nằm ngoài quyền được cấp", code="GIS_ACCESS_DENIED")
def enable_internal_edit(session,ctx,sid):
    if not scoped(session,ctx): return False
    if not source_allowed(session,ctx,sid) or not any(source_allowed(session,ctx,sid,a) for a in ("create","geometry","attributes","delete")): deny()
    session.execute(text("SELECT set_config('openvietgis.acl_internal','on',true)"))
    return True
def verify_edit(session,ctx,sid,item):
    fid=item["feature_id"]; kind=item["op_type"];
    before=item["before_props"]; after=item["after_props"]; bw=item["before_wkb"]; aw=item["after_wkb"]
    def allowed(attrs,wkb,action):
        return feature_allowed(session,ctx,sid,fid,attrs,wkb,action)
    if kind=="create":
        if not allowed(after,aw,"create") or not allowed(after,aw,"read"): deny()
    elif kind=="delete":
        if not allowed(before,bw,"delete") or not allowed(before,bw,"read"): deny()
    else:
        if not allowed(before,bw,"read") or not allowed(after,aw,"read"): deny()
        if bw!=aw and not (allowed(before,bw,"geometry") and allowed(after,aw,"geometry")): deny()
        if before!=after and not (allowed(before,bw,"attributes") and allowed(after,aw,"attributes")): deny()
def filtered_layer_wire(session,ctx,wire):
    if not scoped(session,ctx): return wire
    src=wire["source"]; sid=src["id"]
    count=session.execute(text("SELECT count(*) FROM spatial.reference_features WHERE organization_id=:org AND project_id=:proj AND source_id=:sid"),params(ctx)|{"sid":sid}).scalar_one()
    src["feature_count"]=count
    src["extent"]=None;src["extent_state"]="unavailable";src["provenance"]={"reference_key":None,"fixture_version":None,"manifest_digest":None,"source_asset_id":None}
    actions=[a for a in ["read","create","geometry","attributes","delete","export","approve"] if source_allowed(session,ctx,sid,a)]
    wire["access_actions"]=actions
    for c in wire["capabilities"]:
        if c["name"]=="edit":c["actor_authorized"]=c["actor_authorized"] and bool(set(actions)&{"create","geometry","attributes","delete"})
        elif c["name"] not in ("feature_read","display","geometry_preview"):c["actor_authorized"]=False
    return wire
