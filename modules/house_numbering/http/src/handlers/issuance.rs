//! Record external decisions, track plates and resolve authenticated QR references.
use std::collections::HashMap;
use axum::{body::Bytes,extract::{Path,State},http::{HeaderMap,StatusCode},routing::{get,post},Json,Router};
use ovg_module_http::{ModuleHttpState as AppState,ModuleSession as HumanSession};
use ovg_module_house_numbering_storage as store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use super::{audit_actor,csrf,decode,module_manage,module_read,require_module_permission,path_id,scope_ids,text_len,HnResult,HouseNumberingError,BASE};

pub(super) fn router()->Router<AppState>{
    Router::new()
        .route(&format!("{BASE}/issuances"),post(record))
        .route(&format!("{BASE}/issuances/{{issuance_id}}"),get(detail))
        .route(&format!("{BASE}/issuances/{{issuance_id}}/transition"),post(transition))
        .route(&format!("{BASE}/buildings/{{building_id}}/issuances"),get(list))
        .route(&format!("{BASE}/address-lookup/{{issuance_id}}"),get(lookup))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordBody{
    proposal_id:Uuid,number_id:Uuid,authority_name:String,decision_ref:String,
    certificate_ref:String,effective_date:chrono::NaiveDate,template_revision:String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionBody{expected_revision:i64,state:String,evidence_ref:String}

async fn record(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;
    let input:RecordBody=decode(&body)?;
    if ![&input.authority_name,&input.decision_ref,&input.certificate_ref].iter().all(|s|text_len(s.trim(),1,200))||!text_len(input.template_revision.trim(),1,100){
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit=module_manage(&state,&auth,org,project).await?;
    require_module_permission(&mut edit,"ext.house_numbering.approve").await?;
    let result=store::record_issuance(&mut edit,audit_actor(&auth),store::IssuanceInput{
        proposal_id:input.proposal_id,number_id:input.number_id,authority_name:input.authority_name.trim(),
        decision_ref:input.decision_ref.trim(),certificate_ref:input.certificate_ref.trim(),effective_date:input.effective_date,template_revision:input.template_revision.trim(),
    }).await?;
    edit.commit().await?;Ok((StatusCode::CREATED,Json(result)))
}
async fn transition(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<Json<Value>>{
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;let id=path_id(&params,"issuance_id")?;
    let input:TransitionBody=decode(&body)?;
    if input.expected_revision<1||!matches!(input.state.as_str(),"printed"|"delivered"|"installed"|"void")||!text_len(input.evidence_ref.trim(),1,500){
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit=module_manage(&state,&auth,org,project).await?;
    let result=store::transition_issuance(&mut edit,audit_actor(&auth),id,input.expected_revision,&input.state,input.evidence_ref.trim()).await?;
    edit.commit().await?;Ok(Json(result))
}
async fn detail(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let id=path_id(&params,"issuance_id")?;
    let mut read=module_manage(&state,&auth,org,project).await?;
    let result=store::get_issuance(&mut read,id).await?;read.commit().await?;Ok(Json(result))
}
async fn list(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let id=path_id(&params,"building_id")?;
    let mut read=module_manage(&state,&auth,org,project).await?;
    let result=store::list_issuances(&mut read,id).await?;read.commit().await?;Ok(Json(result))
}
async fn lookup(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let id=path_id(&params,"issuance_id")?;
    let mut read=module_read(&state,&auth,org,project).await?;
    let result=store::lookup_issuance(&mut read,id).await?;read.commit().await?;Ok(Json(result))
}
