use std::collections::HashMap;
use axum::{body::Bytes,extract::{Path,State},http::{HeaderMap,StatusCode},routing::{get,post},Json,Router};
use ovg_module_http::{ModuleHttpState as AppState,ModuleSession as HumanSession};
use ovg_module_house_numbering_storage as store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use super::{audit_actor,csrf,decode,module_read,module_approve,path_id,scope_ids,text_len,HnResult,HouseNumberingError,BASE};

pub(super) fn router()->Router<AppState>{
    Router::new().route(&format!("{BASE}/addresses/{{number_id}}/retire"),post(retire))
      .route(&format!("{BASE}/addresses/{{number_id}}/successor"),post(successor))
      .route(&format!("{BASE}/buildings/{{building_id}}/address-history"),get(history))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Retire{expected_revision:i64,effective_date:chrono::NaiveDate,decision_ref:String,reason:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Successor{new_number_id:Uuid,proposal_id:Uuid,effective_date:chrono::NaiveDate,decision_ref:String}
async fn retire(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<Json<Value>>{
    csrf(&state,&auth,&headers)?;let (org,project)=scope_ids(&params)?;let id=path_id(&params,"number_id")?;
    let input:Retire=decode(&body)?;
    if input.expected_revision<1||!text_len(input.decision_ref.trim(),1,200)||!text_len(input.reason.trim(),1,500){return Err(HouseNumberingError::InvalidRequest);}
    let mut edit=module_approve(&state,&auth,org,project).await?;
    let result=store::retire_address(&mut edit,audit_actor(&auth),id,input.expected_revision,input.effective_date,input.decision_ref.trim(),input.reason.trim()).await?;
    edit.commit().await?;Ok(Json(result))
}
async fn successor(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;let (org,project)=scope_ids(&params)?;let id=path_id(&params,"number_id")?;
    let input:Successor=decode(&body)?;
    if id==input.new_number_id||!text_len(input.decision_ref.trim(),1,200){return Err(HouseNumberingError::InvalidRequest);}
    let mut edit=module_approve(&state,&auth,org,project).await?;
    let result=store::link_successor(&mut edit,audit_actor(&auth),id,input.new_number_id,input.proposal_id,input.effective_date,input.decision_ref.trim()).await?;
    edit.commit().await?;Ok((StatusCode::CREATED,Json(result)))
}
async fn history(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let id=path_id(&params,"building_id")?;
    let mut read=module_read(&state,&auth,org,project).await?;
    let result=store::address_history(&mut read,id).await?;read.commit().await?;Ok(Json(result))
}
