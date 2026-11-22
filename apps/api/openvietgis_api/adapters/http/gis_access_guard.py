"""Fail closed for scoped accounts on project operations without a reviewed ACL adapter."""
from typing import Annotated
from uuid import UUID
from fastapi import Depends,Request,HTTPException
from sqlalchemy import text
from sqlalchemy.orm import Session
from openvietgis_api.adapters.http.security import AuthenticatedRequest,authenticate_request,get_db_session
from openvietgis_api.adapters.http.authorization import context_for
from openvietgis_api.adapters.persistence.rls import require_member_context
from openvietgis_api.adapters.persistence.gis_access import scoped

SAFE_GET={"","/layers","/layers/{layer_id}","/layers/{layer_id}/features","/layers/{layer_id}/features/{feature_id}",
          "/layers/{layer_id}/viewport","/layers/{layer_id}/display-extent","/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt",
          "/map","/extensions/activations","/access/mine","/access/export/{layer_id}","/access/reviews"}
SAFE_POST={"/sources/{source_id}/changesets","/layers/{layer_id}/geometry-preview","/pro/geometry","/editing/snap","/access/reviews/{changeset_id}"}
def project_access_guard(request:Request,auth:Annotated[AuthenticatedRequest,Depends(authenticate_request)],
    session:Annotated[Session,Depends(get_db_session,scope="function")]):
    if "project_id" not in request.path_params or "organization_id" not in request.path_params or auth.actor.actor_type!="human":
        return
    try:
        org=UUID(str(request.path_params["organization_id"]));proj=UUID(str(request.path_params["project_id"]))
    except ValueError:return
    ctx=context_for(auth,organization_id=org,project_id=proj)
    require_member_context(session,ctx,org)
    # Serialize ACL changes with project operations. Follow the existing organization -> project lock order.
    lock=" FOR SHARE" if request.method in ("GET","HEAD") else " FOR UPDATE"
    session.execute(text("SELECT id FROM org.organizations WHERE id=:org"+lock),{"org":org})
    session.execute(text("SELECT id FROM workspace.projects WHERE organization_id=:org AND id=:proj"+lock),{"org":org,"proj":proj})
    if not scoped(session,ctx):return
    path=str(getattr(request.scope.get("route"),"path",""))
    marker="/projects/{project_id}"
    suffix=path.split(marker,1)[1] if marker in path else "__unknown__"
    if (request.method=="GET" and suffix in SAFE_GET) or (request.method=="POST" and suffix in SAFE_POST):return
    raise HTTPException(403,detail="Thao tác chưa được cấp quyền trong phạm vi GIS")
