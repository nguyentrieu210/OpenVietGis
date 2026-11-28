//! Observed entrances are evidence, not independent legally allocated addresses.
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::{append_audit,db_error,AuditActor,buildings::get_building,hierarchy::get_route};
use ovg_storage::module_ports::{lock_linked_features,validate_linked_feature,SpatialFeatureRef};

pub struct EntranceInput<'a>{
    pub route_id:Uuid,pub source_id:Uuid,pub feature_id:&'a str,
    pub evidence_ref:&'a str,pub observed_number:Option<&'a str>,
}

async fn validate_point(edit:&mut ProjectRead<'_>,source_id:Uuid,feature_id:&str)->Result<(),AppError>{
    let link=SpatialFeatureRef{source_id,feature_id:feature_id.to_owned()};
    if !lock_linked_features(edit,std::slice::from_ref(&link),true).await?.contains(&link){
        return Err(AppError::not_found("entrance point not found"));
    }
    validate_linked_feature(edit,source_id,feature_id,&["ST_Point"]).await?;
    Ok(())
}

pub async fn observe_entrance(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,building:Uuid,input:EntranceInput<'_>)->Result<Value,AppError>{
    let current=get_building(edit,building,true).await?;
    if current["state"]!="active"{return Err(AppError::conflict("BUILDING_INACTIVE","building is not active"));}
    get_route(edit,input.route_id,false).await?;
    validate_point(edit,input.source_id,input.feature_id).await?;
    let id=Uuid::new_v4();
    let inserted:Option<Uuid>=sqlx::query_scalar(
        "INSERT INTO house_numbering.building_entrances(id,organization_id,project_id,building_id,route_id,source_id,feature_id,evidence_ref,observed_number,created_by)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING RETURNING id"
    ).bind(id).bind(edit.organization_id()).bind(edit.project_id()).bind(building).bind(input.route_id)
      .bind(input.source_id).bind(input.feature_id).bind(input.evidence_ref).bind(input.observed_number).bind(actor.id)
      .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    if inserted.is_none(){return Err(AppError::conflict("ENTRANCE_EXISTS","entrance observation already exists"));}
    append_review(edit,actor.id,id,1,"observed",input.evidence_ref).await?;
    append_audit(edit,actor,"house_numbering.entrance.observe","house_numbering_entrance",id,json!({"building_id":building,"route_id":input.route_id})).await?;
    detail(edit,id).await
}
async fn append_review(edit:&mut ProjectRead<'_>,actor:Uuid,id:Uuid,revision:i64,state:&str,reason:&str)->Result<(),AppError>{
    sqlx::query("INSERT INTO house_numbering.entrance_reviews(organization_id,project_id,entrance_id,revision,state,reason,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
      .bind(edit.organization_id()).bind(edit.project_id()).bind(id).bind(revision).bind(state).bind(reason).bind(actor)
      .execute(&mut **edit.transaction()).await.map_err(db_error)?;
    Ok(())
}
async fn detail(read:&mut ProjectRead<'_>,id:Uuid)->Result<Value,AppError>{
    sqlx::query_scalar("SELECT jsonb_build_object('record',to_jsonb(e),'history',
      (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.revision),'[]'::jsonb) FROM house_numbering.entrance_reviews r
       WHERE (r.organization_id,r.project_id,r.entrance_id)=(e.organization_id,e.project_id,e.id)))
      FROM house_numbering.building_entrances e WHERE e.organization_id=$1 AND e.project_id=$2 AND e.id=$3")
      .bind(read.organization_id()).bind(read.project_id()).bind(id).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?
      .ok_or_else(||AppError::not_found("entrance not found"))
}
pub async fn list_entrances(read:&mut ProjectRead<'_>,building:Uuid)->Result<Value,AppError>{
    get_building(read,building,false).await?;
    let items:Vec<Value>=sqlx::query_scalar(
      "SELECT to_jsonb(e)||jsonb_build_object('state',r.state,'revision',r.revision,'route_name',route.name,
       'geometry_ready',coalesce(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready' AND ST_GeometryType(f.geometry)='ST_Point',false))
       FROM house_numbering.building_entrances e
       JOIN LATERAL (SELECT state,revision FROM house_numbering.entrance_reviews r WHERE (r.organization_id,r.project_id,r.entrance_id)=(e.organization_id,e.project_id,e.id) ORDER BY revision DESC LIMIT 1) r ON true
       JOIN house_numbering.routes route ON (route.organization_id,route.project_id,route.id)=(e.organization_id,e.project_id,e.route_id)
       LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(e.organization_id,e.project_id,e.source_id,e.feature_id)
       LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
       WHERE e.organization_id=$1 AND e.project_id=$2 AND e.building_id=$3 ORDER BY e.created_at,e.id")
       .bind(read.organization_id()).bind(read.project_id()).bind(building).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(json!({"items":items}))
}
pub async fn review_entrance(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,id:Uuid,expected:i64,state:&str,reason:&str)->Result<Value,AppError>{
    let row:Option<(Uuid,Uuid,Uuid,String,Uuid,i64)>=sqlx::query_as(
      "SELECT e.building_id,e.route_id,e.source_id,e.feature_id,e.created_by,r.revision FROM house_numbering.building_entrances e
       JOIN LATERAL (SELECT revision FROM house_numbering.entrance_reviews r WHERE (r.organization_id,r.project_id,r.entrance_id)=(e.organization_id,e.project_id,e.id) ORDER BY revision DESC LIMIT 1) r ON true
       WHERE e.organization_id=$1 AND e.project_id=$2 AND e.id=$3")
       .bind(edit.organization_id()).bind(edit.project_id()).bind(id).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let (building,route,source,feature,creator,revision)=row.ok_or_else(||AppError::not_found("entrance not found"))?;
    if revision!=expected{return Err(AppError::conflict("STALE_ENTRANCE","entrance revision changed"));}
    if matches!(state,"confirmed"|"rejected")&&creator==actor.id{return Err(AppError::forbidden("independent entrance reviewer required"));}
    if state=="confirmed"{
        if get_building(edit,building,true).await?["state"]!="active" || get_route(edit,route,false).await?["status"]!="confirmed"{
            return Err(AppError::conflict("ENTRANCE_UNVERIFIED","active building and confirmed route required"));
        }
        validate_point(edit,source,&feature).await?;
    }
    append_review(edit,actor.id,id,revision.checked_add(1).ok_or_else(||AppError::internal("entrance revision overflow"))?,state,reason).await?;
    append_audit(edit,actor,"house_numbering.entrance.review","house_numbering_entrance",id,json!({"state":state,"building_id":building})).await?;
    detail(edit,id).await
}

/// Conservative gate until multi-address selection has a separately approved
/// policy. Observations, competing routes and stale confirmed geometry need review.
pub async fn entrance_blockers(read:&mut ProjectRead<'_>,buildings:&[Uuid])->Result<Vec<Uuid>,AppError>{
    sqlx::query_scalar(
        "SELECT DISTINCT b.id FROM house_numbering.buildings b
         JOIN house_numbering.building_entrances e ON (e.organization_id,e.project_id,e.building_id)=(b.organization_id,b.project_id,b.id)
         LEFT JOIN LATERAL (SELECT state FROM house_numbering.entrance_reviews r WHERE (r.organization_id,r.project_id,r.entrance_id)=(e.organization_id,e.project_id,e.id) ORDER BY revision DESC LIMIT 1) r ON true
         LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(e.organization_id,e.project_id,e.source_id,e.feature_id)
         LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         JOIN house_numbering.routes route ON (route.organization_id,route.project_id,route.id)=(e.organization_id,e.project_id,e.route_id)
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY($3)
           AND (r.state IS NULL OR r.state='observed' OR (r.state='confirmed' AND (
             b.route_id IS DISTINCT FROM e.route_id OR route.status<>'confirmed'
             OR NOT coalesce(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready' AND ST_GeometryType(f.geometry)='ST_Point',false))))
         ORDER BY b.id"
    ).bind(read.organization_id()).bind(read.project_id()).bind(buildings)
      .fetch_all(&mut **read.transaction()).await.map_err(db_error)
}
