use std::collections::HashMap;
use axum::{body::Bytes,extract::{Path,State,Query},http::HeaderMap,routing::{post,get},Json,Router};
use ovg_module_http::{ModuleHttpState as AppState,ModuleSession as HumanSession};
use ovg_module_house_numbering_storage as store;
use ovg_module_house_numbering::score_review::{evaluate,PointRow};
use serde::Deserialize;
use serde_json::{json,Value};
use uuid::Uuid;
use super::{csrf,decode,module_read,scope_ids,text_len,HnResult,HouseNumberingError,BASE};

pub(super) fn router()->Router<AppState>{
    Router::new().route(&format!("{BASE}/score-review/preview"),post(preview))
        .route(&format!("{BASE}/certificate-locations"),get(locations))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input{
    point_source:Uuid,grid_source:Option<Uuid>,point_number_field:String,point_road_field:String,
    grid_number_field:String,grid_road_field:String,metric_crs:i32,offset:Option<usize>,status:Option<String>,
}
async fn preview(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<Json<Value>>{
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;let input:Input=decode(&body)?;
    if Some(input.point_source)==input.grid_source||!matches!(input.metric_crs,32648|32649)
      || ![&input.point_number_field,&input.point_road_field].iter().all(|v|text_len(v,1,128))
      || (input.grid_source.is_some() && ![&input.grid_number_field,&input.grid_road_field].iter().all(|v|text_len(v,1,128)))
      || input.offset.unwrap_or(0)>50000
      || input.status.as_deref().is_some_and(|s|!matches!(s,"khop"|"lech_vi_tri"|"sai_so"|"khong_co"|"thieu_so"|"chua_vi_tri")){
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut read=module_read(&state,&auth,org,project).await?;
    let snapshot=store::score_inputs(&mut read,input.point_source,input.grid_source,&input.point_number_field,&input.point_road_field,&input.grid_number_field,&input.grid_road_field,input.metric_crs).await?;
    read.commit().await?;
    let points:Vec<PointRow>=serde_json::from_value(snapshot["points"]["items"].clone()).map_err(|_|ovg_core::AppError::internal("invalid point data"))?;
    let grid:Vec<PointRow>=serde_json::from_value(snapshot["grid"]["items"].clone()).map_err(|_|ovg_core::AppError::internal("invalid grid data"))?;
    let grid_usable=grid.iter().filter(|g|g.x.is_some()&&g.y.is_some()&&g.number.as_deref().and_then(ovg_module_house_numbering::score_review::number).is_some()).count();
    if grid_usable==0{return Err(ovg_core::AppError::validation("SCORE_GRID_EMPTY","no usable reference grid points").into());}
    let total=points.len();let offset=input.offset.unwrap_or(0);
    let (rows,counts,duplicates)=tokio::task::spawn_blocking(move||evaluate(&points,&grid)).await.map_err(|_|ovg_core::AppError::internal("score evaluation failed"))?;
    let filtered:Vec<_>=rows.into_iter().filter(|r|input.status.as_deref().is_none_or(|s|r.status.key()==s)).collect();
    let filtered_total=filtered.len();
    let page:Vec<_>=filtered.into_iter().skip(offset).take(200).collect();
    Ok(Json(json!({"total":total,"counts":counts,"duplicate_points":duplicates,"grid_usable":grid_usable,"items":page,
      "filtered_total":filtered_total,"next_offset":if offset+page.len()<filtered_total{Some(offset+page.len())}else{None},
      "point_revision":snapshot["points"]["revision"],"grid_revision":snapshot["grid"]["revision"],"metric_crs":input.metric_crs,
      "grid_mode":if input.grid_source.is_some(){"source"}else{"registry"},
      "ambiguous_grid_buildings":snapshot["grid"]["ambiguous_buildings"].as_u64().unwrap_or(0),
      "policy":"binh-hung-e9e4f0d-preview"})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocationInput{route_id:Uuid,metric_crs:i32,offset:Option<i64>}
async fn locations(State(state):State<AppState>,auth:HumanSession,Path(params):Path<HashMap<String,String>>,Query(input):Query<LocationInput>)->HnResult<Json<Value>>{
    let (org,project)=scope_ids(&params)?;let offset=input.offset.unwrap_or(0);
    if !matches!(input.metric_crs,32648|32649)||!(0..=50000).contains(&offset){return Err(HouseNumberingError::InvalidRequest);}
    let mut read=module_read(&state,&auth,org,project).await?;
    let result=store::certificate_locations(&mut read,input.route_id,input.metric_crs,offset).await?;
    read.commit().await?;Ok(Json(result))
}
