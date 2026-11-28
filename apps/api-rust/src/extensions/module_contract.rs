//! Read-only discovery and aggregate services for trusted native modules.
use axum::{extract::{Path,Request,State},routing::get,Json,Router};
use ovg_core::AppError;
use ovg_storage::{catalog::ProjectReadPolicy,extensions::permissions,module_ports};
use serde_json::{json,Value};
use crate::AppState;
use super::{authenticate_actor,open_project_scope,parse_uuid,ExtensionError,module_permissions::verified_manifest};

pub(crate) fn router()->Router<AppState>{
    Router::new()
        .route("/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/runtime-contract",get(contract))
        .route("/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/spatial-summary/{role}",get(summary))
}
async fn read_scope<'a>(state:&'a AppState,request:Request,org:&str,project:&str,key:&str)->Result<ovg_storage::catalog::ProjectRead<'a>,ExtensionError>{
    let (mut parts,_)=request.into_parts();
    let actor=authenticate_actor(state,&mut parts,org).await?;
    let mut read=open_project_scope(state,&actor,parse_uuid(org)?,parse_uuid(project)?,ProjectReadPolicy::Unrestricted).await?;
    let manifest=verified_manifest(state,&mut read,key).await.map_err(ExtensionError::Domain)?;
    let permission=format!("ext.{key}.read");
    if !manifest.permissions.contains(&permission)||!permissions::allows(&mut read,key,&permission).await.map_err(ExtensionError::Domain)?{
        return Err(ExtensionError::Domain(AppError::forbidden("missing module read permission")));
    }
    Ok(read)
}
async fn contract(State(state):State<AppState>,Path((org,project,key)):Path<(String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let read=read_scope(&state,request,&org,&project,&key).await?;
    let manifest=state.modules.manifest(&key).ok_or_else(||ExtensionError::Domain(AppError::not_found("module runtime missing")))?;
    let roles=state.modules.layer_roles(&key).unwrap_or_default().iter().map(|r|json!({"id":r.id,"geometry_families":r.geometry_families,"required_fields":r.required_fields})).collect::<Vec<_>>();
    let commands=manifest.commands.iter().filter_map(|c|state.modules.pure_command(&key,&c.id).map(|_|json!({"id":c.id,"required_permission":c.required_permission}))).collect::<Vec<_>>();
    let result=json!({"extension_key":key,"version":manifest.version,"api_compatibility":manifest.api_compatibility,"layer_roles":roles,"pure_commands":commands});
    read.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(result))
}
async fn summary(State(state):State<AppState>,Path((org,project,key,role)):Path<(String,String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let mut read=read_scope(&state,request,&org,&project,&key).await?;
    if !state.modules.layer_roles(&key).is_some_and(|roles|roles.iter().any(|r|r.id==role)){
        return Err(ExtensionError::Domain(AppError::validation("UNKNOWN_LAYER_ROLE","role not declared by module")));
    }
    let value=module_ports::bound_spatial_summary(&mut read,&key,&role).await.map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(value))
}
