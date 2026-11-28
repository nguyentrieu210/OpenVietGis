use std::collections::HashMap;
use axum::{body::Bytes,extract::{Path,State,Query},http::{HeaderMap,StatusCode},routing::{get,post},Json,Router};
use ovg_module_http::{ModuleHttpState as AppState,ModuleSession as HumanSession};
use ovg_module_house_numbering_storage as store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use super::{audit_actor,csrf,decode,module_read,module_survey,module_approve,path_id,scope_ids,text_len,HnResult,HouseNumberingError,BASE};

pub(super) fn router()->Router<AppState>{
    Router::new().route(&format!("{BASE}/parcels"),get(list).post(create))
      .route(&format!("{BASE}/buildings/{{building_id}}/parcels"),get(links))
      .route(&format!("{BASE}/buildings/{{building_id}}/parcels/{{parcel_id}}/transition"),post(transition))
}
#[derive(Deserialize)]
struct Page{offset:Option<i64>,limit:Option<i64>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create{source_id:Uuid,feature_id:String,reference:String,evidence_ref:String}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transition{expected_revision:i64,state:String,evidence_ref:String}
async fn list(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>,Query(page):Query<Page>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let offset=page.offset.unwrap_or(0);let limit=page.limit.unwrap_or(100);
    if !(0..=1_000_000).contains(&offset)||!(1..=200).contains(&limit){return Err(HouseNumberingError::InvalidRequest);}
    let mut read=module_read(&state,&auth,org,project).await?;
    let result=store::list_parcels(&mut read,offset,limit).await?;read.commit().await?;Ok(Json(result))
}
async fn create(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;let (org,project)=scope_ids(&params)?;let input:Create=decode(&body)?;
    if !text_len(&input.feature_id,1,128)||!text_len(input.reference.trim(),1,200)||!text_len(input.evidence_ref.trim(),1,500){return Err(HouseNumberingError::InvalidRequest);}
    let mut edit=module_survey(&state,&auth,org,project).await?;
    let result=store::create_parcel(&mut edit,audit_actor(&auth),input.source_id,&input.feature_id,input.reference.trim(),input.evidence_ref.trim()).await?;
    edit.commit().await?;Ok((StatusCode::CREATED,Json(result)))
}
async fn links(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let building=path_id(&params,"building_id")?;
    let mut read=module_read(&state,&auth,org,project).await?;
    let result=store::list_parcel_links(&mut read,building).await?;read.commit().await?;Ok(Json(result))
}
async fn transition(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<Json<Value>>{
    csrf(&state,&auth,&headers)?;let (org,project)=scope_ids(&params)?;
    let building=path_id(&params,"building_id")?;let parcel=path_id(&params,"parcel_id")?;
    let input:Transition=decode(&body)?;
    if input.expected_revision<0||!matches!(input.state.as_str(),"observed"|"confirmed"|"rejected"|"detached")||!text_len(input.evidence_ref.trim(),1,500){return Err(HouseNumberingError::InvalidRequest);}
    let mut edit=if input.state=="observed"{module_survey(&state,&auth,org,project).await?}else{module_approve(&state,&auth,org,project).await?};
    let result=store::append_parcel_link(&mut edit,audit_actor(&auth),building,parcel,input.expected_revision,&input.state,input.evidence_ref.trim()).await?;
    edit.commit().await?;Ok(Json(result))
}
