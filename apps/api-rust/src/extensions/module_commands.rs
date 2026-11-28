//! Authenticated, bounded dispatch of pure SDK commands from trusted registered modules.
use axum::{extract::{Path, Request, State}, routing::post, Json, Router};
use ovg_core::{ActorContext,ActorType,AppError,Permission,TenantScope};
use ovg_module_sdk::{ModuleCommandRequest,ModuleExecutionContext};
use ovg_storage::{catalog::ProjectReadPolicy,extensions::permissions};
use serde::Deserialize;
use serde_json::{json,Value};
use crate::AppState;
use super::{authenticate_json,decode_body,open_project_scope,parse_uuid,require_actor_csrf,ExtensionError,module_permissions::verified_manifest};

const BASE:&str="/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/commands/{command_id}";
pub(crate) fn router()->Router<AppState>{Router::new().route(BASE,post(dispatch))}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body { payload: Value }

async fn dispatch(State(state):State<AppState>,Path((org,project,key,command_id)):Path<(String,String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (actor,headers,body)=authenticate_json(&state,request,&org).await?;
    // Initial gateway is human-only; service credentials need a separate verified scope adapter.
    if !actor.is_human(){return Err(ExtensionError::Domain(AppError::forbidden("human session required")));}
    require_actor_csrf(&state,&actor,&headers)?;
    let body:Body=decode_body(body)?;
    if serde_json::to_vec(&body.payload).map_err(|_|ExtensionError::InvalidRequest)?.len()>16*1024{return Err(ExtensionError::InvalidRequest);}
    let exposed=state.modules.pure_command(&key,&command_id).ok_or_else(||ExtensionError::Domain(AppError::not_found("command is not exposed as a pure runtime operation")))?;
    let policy=match exposed.core_permission{Permission::ProjectRead=>ProjectReadPolicy::Unrestricted,Permission::ProjectEdit=>ProjectReadPolicy::UnrestrictedEdit,Permission::ProjectManage=>ProjectReadPolicy::UnrestrictedManage,_=>return Err(ExtensionError::Domain(AppError::forbidden("unsupported core permission")))};
    let org_id=parse_uuid(&org)?;let project_id=parse_uuid(&project)?;
    let mut scope=open_project_scope(&state,&actor,org_id,project_id,policy).await?;
    let manifest=verified_manifest(&state,&mut scope,&key).await.map_err(ExtensionError::Domain)?;
    let spec=manifest.commands.iter().find(|c|c.id==command_id).ok_or_else(||ExtensionError::Domain(AppError::not_found("command missing from active contract")))?;
    let read_permission=format!("ext.{key}.read");
    for permission in [&read_permission,&spec.required_permission]{
        if !manifest.permissions.contains(permission)||!permissions::allows(&mut scope,&key,permission).await.map_err(ExtensionError::Domain)?{return Err(ExtensionError::Domain(AppError::forbidden("missing module command permission")));}
    }
    let ctx=ModuleExecutionContext{tenant:TenantScope{organization_id:org_id,workspace_id:None,project_id:Some(project_id),actor:ActorContext{actor_type:ActorType::Human,actor_id:actor.actor_id(),credential_id:None,email:None},permissions:vec![Permission::ProjectRead,exposed.core_permission]},granted_extension_permissions:vec![read_permission,spec.required_permission.clone()]};
    let result=state.modules.dispatch_pure(&key,&ctx,&ModuleCommandRequest{command_id,payload:body.payload}).map_err(ExtensionError::Domain)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(json!(result)))
}
