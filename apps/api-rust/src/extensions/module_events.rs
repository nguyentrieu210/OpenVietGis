//! Bounded replay of transactional module invalidation notices.
use axum::{extract::{Path,Query,Request,State},routing::get,Json,Router};
use ovg_storage::{catalog::ProjectReadPolicy,extensions::events::{self,ModuleCursor}};
use serde::Deserialize;
use serde_json::Value;
use crate::AppState;
use super::{authenticate_actor,open_project_scope,parse_uuid,ExtensionError};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replay { cursor:Option<String>, limit:Option<i64> }
pub(crate) fn router()->Router<AppState>{
    Router::new().route("/organizations/{organization_id}/projects/{project_id}/extensions/change-events",get(replay))
}
async fn replay(State(state):State<AppState>,Path((org,project)):Path<(String,String)>,Query(query):Query<Replay>,request:Request)->Result<Json<Value>,ExtensionError>{
    let (mut parts,_)=request.into_parts();
    let actor=authenticate_actor(&state,&mut parts,&org).await?;
    let mut read=open_project_scope(&state,&actor,parse_uuid(&org)?,parse_uuid(&project)?,ProjectReadPolicy::Unrestricted).await?;
    let cursor=query.cursor.as_deref().map(ModuleCursor::parse).transpose().map_err(ExtensionError::Domain)?;
    let value=events::replay(&mut read,cursor,query.limit.unwrap_or(100)).await.map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;Ok(Json(value))
}
