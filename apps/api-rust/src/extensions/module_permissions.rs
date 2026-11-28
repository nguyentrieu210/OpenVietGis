//! Official plugin permission policies managed by human project managers.
use axum::extract::{Path,Request,State};
use axum::routing::get;
use axum::{Json,Router};
use ovg_core::AppError;
use ovg_module_sdk::ModuleManifest;
use ovg_storage::{catalog::{ProjectRead,ProjectReadPolicy},extensions};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;
use crate::AppState;
use super::{authenticate_actor,authenticate_json,decode_body,open_project_scope,parse_uuid,project_actor_display,require_actor_csrf,ExtensionError};

const BASE:&str="/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/permissions/{actor_type}/{actor_id}";

pub(crate) fn router()->Router<AppState>{Router::new().route(BASE,get(get_policy).put(put_policy)).route("/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/permissions/self",get(self_policy))}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replace{permissions:Option<Vec<String>>,expected_revision:i64}

pub(crate) async fn verified_manifest(state:&AppState,read:&mut ProjectRead<'_>,key:&str)->Result<ModuleManifest,AppError>{
    ovg_module_http::verified_manifest(&state.modules,read,key).await
}

fn target(actor_type:&str,actor:&str)->Result<Uuid,ExtensionError>{
    if !matches!(actor_type,"human"|"service_principal"){return Err(ExtensionError::InvalidRequest);}
    let id=parse_uuid(actor)?;if id.is_nil(){return Err(ExtensionError::InvalidRequest);}Ok(id)
}

async fn get_policy(State(state):State<AppState>,Path((org,project,key,actor_type,actor_id)):Path<(String,String,String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (mut parts,_)=request.into_parts();let actor=authenticate_actor(&state,&mut parts,&org).await?;
    if !actor.is_human(){return Err(ExtensionError::Domain(AppError::forbidden("human project manager required")));}
    let id=target(&actor_type,&actor_id)?;
    let mut read=open_project_scope(&state,&actor,parse_uuid(&org)?,parse_uuid(&project)?,ProjectReadPolicy::UnrestrictedManage).await?;
    verified_manifest(&state,&mut read,&key).await.map_err(ExtensionError::Domain)?;
    let policy=extensions::permissions::policy(&mut read,&key,&actor_type,id).await.map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(policy))
}

async fn put_policy(State(state):State<AppState>,Path((org,project,key,actor_type,actor_id)):Path<(String,String,String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (actor,headers,body)=authenticate_json(&state,request,&org).await?;
    if !actor.is_human(){return Err(ExtensionError::Domain(AppError::forbidden("human project manager required")));}
    require_actor_csrf(&state,&actor,&headers)?;let id=target(&actor_type,&actor_id)?;
    // Require the permissions key explicitly: omitted input must not restore inherited privileges.
    if !body.as_ref().is_some_and(|v|v.get("permissions").is_some()){return Err(ExtensionError::InvalidRequest);}
    let body:Replace=decode_body(body)?;
    if body.expected_revision<0{return Err(ExtensionError::InvalidRequest);}
    let mut edit=open_project_scope(&state,&actor,parse_uuid(&org)?,parse_uuid(&project)?,ProjectReadPolicy::UnrestrictedManage).await?;
    let manifest=verified_manifest(&state,&mut edit,&key).await.map_err(ExtensionError::Domain)?;
    if body.permissions.as_ref().is_some_and(|p|p.len()>64||p.iter().collect::<HashSet<_>>().len()!=p.len()||p.iter().any(|permission|!manifest.permissions.contains(permission))){return Err(ExtensionError::InvalidRequest);}
    let display=project_actor_display(&edit,&actor);
    let result=extensions::permissions::replace_policy(&mut edit,&key,&actor_type,id,body.permissions.as_deref(),body.expected_revision,&display).await.map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(result))
}

async fn self_policy(State(state):State<AppState>,Path((org,project,key)):Path<(String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (mut parts,_)=request.into_parts();let actor=authenticate_actor(&state,&mut parts,&org).await?;
    let mut read=open_project_scope(&state,&actor,parse_uuid(&org)?,parse_uuid(&project)?,ProjectReadPolicy::Unrestricted).await?;
    verified_manifest(&state,&mut read,&key).await.map_err(ExtensionError::Domain)?;
    let actor_type=read.actor_type();let id=read.actor_id();
    let value=extensions::permissions::policy(&mut read,&key,actor_type,id).await.map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(value))
}
