use std::collections::HashMap;
use axum::{body::Bytes,extract::{Path,State},http::{HeaderMap,StatusCode},routing::{get,post},Json,Router};
use ovg_module_http::{ModuleHttpState as AppState,ModuleSession as HumanSession};
use ovg_module_house_numbering_storage as store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use super::{audit_actor,csrf,decode,module_read,module_survey,module_approve,path_id,scope_ids,text_len,HnResult,HouseNumberingError,BASE};

pub(super) fn router()->Router<AppState>{
    Router::new().route(&format!("{BASE}/buildings/{{building_id}}/entrances"),get(list).post(observe))
      .route(&format!("{BASE}/entrances/{{entrance_id}}/review"),post(review))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observe{route_id:Uuid,source_id:Uuid,feature_id:String,evidence_ref:String,observed_number:Option<String>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review{expected_revision:i64,state:String,reason:String}
async fn list(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let id=path_id(&params,"building_id")?;
    let mut read=module_read(&state,&auth,org,project).await?;
    let result=store::list_entrances(&mut read,id).await?;read.commit().await?;Ok(Json(result))
}
async fn observe(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;let (org,project)=scope_ids(&params)?;let id=path_id(&params,"building_id")?;
    let input:Observe=decode(&body)?;
    if !text_len(&input.feature_id,1,128)||!text_len(input.evidence_ref.trim(),1,500)||input.observed_number.as_ref().is_some_and(|s|!text_len(s.trim(),1,200)){return Err(HouseNumberingError::InvalidRequest);}
    let mut edit=module_survey(&state,&auth,org,project).await?;
    let result=store::observe_entrance(&mut edit,audit_actor(&auth),id,store::EntranceInput{route_id:input.route_id,source_id:input.source_id,feature_id:&input.feature_id,evidence_ref:input.evidence_ref.trim(),observed_number:input.observed_number.as_deref()}).await?;
    edit.commit().await?;Ok((StatusCode::CREATED,Json(result)))
}
async fn review(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<Json<Value>>{
    csrf(&state,&auth,&headers)?;let (org,project)=scope_ids(&params)?;let id=path_id(&params,"entrance_id")?;
    let input:Review=decode(&body)?;
    if input.expected_revision<1||!matches!(input.state.as_str(),"confirmed"|"rejected"|"retired")||!text_len(input.reason.trim(),1,500){return Err(HouseNumberingError::InvalidRequest);}
    let mut edit=module_approve(&state,&auth,org,project).await?;
    let result=store::review_entrance(&mut edit,audit_actor(&auth),id,input.expected_revision,&input.state,input.reason.trim()).await?;
    edit.commit().await?;Ok(Json(result))
}
