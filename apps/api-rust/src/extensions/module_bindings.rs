//! Core module binding endpoints. Only human project managers may replace bindings.
use axum::extract::{Path,Request,State};
use axum::routing::get;
use axum::{Json,Router};
use ovg_core::AppError;
use ovg_storage::{catalog::ProjectReadPolicy,extensions::{bindings,permissions}};
use serde::Deserialize;
use serde_json::Value;
use crate::AppState;
use super::{authenticate_actor,authenticate_json,decode_body,open_project_scope,parse_uuid,project_actor_display,require_actor_csrf,ExtensionError,module_permissions::verified_manifest};
const BASE:&str="/organizations/{organization_id}/projects/{project_id}/extensions/{extension_key}/layer-bindings";
pub(crate) fn router()->Router<AppState>{Router::new().route(BASE,get(get_bindings).put(put_bindings))}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replace{expected_revision:i64,items:Vec<bindings::BindingInput>}
async fn get_bindings(State(state):State<AppState>,Path((org,project,key)):Path<(String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (mut parts,_)=request.into_parts();let actor=authenticate_actor(&state,&mut parts,&org).await?;
    let mut read=open_project_scope(&state,&actor,parse_uuid(&org)?,parse_uuid(&project)?,ProjectReadPolicy::Unrestricted).await?;
    verified_manifest(&state,&mut read,&key).await.map_err(ExtensionError::Domain)?;
    if !permissions::allows(&mut read,&key,&format!("ext.{key}.read")).await.map_err(ExtensionError::Domain)?{return Err(ExtensionError::Domain(AppError::forbidden("missing module read permission")));}
    let value=bindings::get(&mut read,&key).await.map_err(ExtensionError::Domain)?;read.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(value))
}
async fn put_bindings(State(state):State<AppState>,Path((org,project,key)):Path<(String,String,String)>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (actor,headers,body)=authenticate_json(&state,request,&org).await?;
    if !actor.is_human(){return Err(ExtensionError::Domain(AppError::forbidden("human project manager required")));}
    require_actor_csrf(&state,&actor,&headers)?;let body:Replace=decode_body(body)?;
    if body.expected_revision<0{return Err(ExtensionError::InvalidRequest);}
    let mut edit=open_project_scope(&state,&actor,parse_uuid(&org)?,parse_uuid(&project)?,ProjectReadPolicy::UnrestrictedManage).await?;
    verified_manifest(&state,&mut edit,&key).await.map_err(ExtensionError::Domain)?;
    for suffix in ["read","manage"]{if !permissions::allows(&mut edit,&key,&format!("ext.{key}.{suffix}")).await.map_err(ExtensionError::Domain)?{return Err(ExtensionError::Domain(AppError::forbidden("missing module management permission")));}}
    let runtime=state.modules.get(&key).ok_or_else(||ExtensionError::Domain(AppError::not_found("module runtime missing")))?;
    if body.items.len()>256{return Err(ExtensionError::InvalidRequest);}
    for item in &body.items{
        let role=runtime.layer_roles().iter().find(|r|r.id==item.role).ok_or_else(||ExtensionError::Domain(AppError::validation("UNKNOWN_LAYER_ROLE","role is not declared by module")))?;
        let (geometry,fields)=bindings::source_contract(&mut edit,item.source_id).await.map_err(ExtensionError::Domain)?;
        if !role.geometry_families.contains(&geometry.as_str())||role.required_fields.iter().any(|field|!fields.iter().any(|f|f==field)){
            return Err(ExtensionError::Domain(AppError::validation("LAYER_ROLE_SCHEMA_MISMATCH","source geometry or required field IDs do not match module role")));
        }
    }
    let display=project_actor_display(&edit,&actor);
    let value=bindings::replace(&mut edit,&key,body.expected_revision,&body.items,&display).await.map_err(ExtensionError::Domain)?;edit.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(value))
}
