//! Observed cadastral identity; not evidence of land title or automatic address allocation.
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::{append_audit,db_error,AuditActor,buildings::get_building};
use ovg_storage::module_ports::{lock_linked_features,validate_linked_feature,SpatialFeatureRef};

pub async fn create_parcel(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,source:Uuid,feature:&str,reference:&str,evidence:&str)->Result<Value,AppError>{
    let link=SpatialFeatureRef{source_id:source,feature_id:feature.to_owned()};
    if !lock_linked_features(edit,std::slice::from_ref(&link),true).await?.contains(&link){return Err(AppError::not_found("parcel feature not found"));}
    validate_linked_feature(edit,source,feature,&["ST_Polygon"]).await?;
    let id=Uuid::new_v4();
    let result:Option<Value>=sqlx::query_scalar(
        "WITH inserted AS (INSERT INTO house_numbering.parcels(id,organization_id,project_id,source_id,feature_id,reference,evidence_ref,created_by)
          VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING *)
          SELECT to_jsonb(inserted) FROM inserted"
    ).bind(id).bind(edit.organization_id()).bind(edit.project_id()).bind(source).bind(feature).bind(reference).bind(evidence).bind(actor.id)
      .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let result=result.ok_or_else(||AppError::conflict("PARCEL_EXISTS","parcel feature already registered"))?;
    append_audit(edit,actor,"house_numbering.parcel.register","house_numbering_parcel",id,json!({"source_id":source,"feature_id":feature})).await?;
    Ok(result)
}
pub async fn list_parcels(read:&mut ProjectRead<'_>,offset:i64,limit:i64)->Result<Value,AppError>{
    let mut items:Vec<Value>=sqlx::query_scalar(
        "SELECT to_jsonb(p)-'geometry_digest'||jsonb_build_object('geometry_ready',p.geometry_digest IS NOT NULL)
         FROM house_numbering.parcel_geometry_v1 p WHERE p.organization_id=$1 AND p.project_id=$2
         ORDER BY p.reference,p.id LIMIT $3 OFFSET $4"
    ).bind(read.organization_id()).bind(read.project_id()).bind(limit+1).bind(offset).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    let next=if items.len()>limit as usize{items.pop();Some(offset+limit)}else{None};
    Ok(json!({"items":items,"next_offset":next}))
}
pub async fn list_parcel_links(read:&mut ProjectRead<'_>,building:Uuid)->Result<Value,AppError>{
    get_building(read,building,false).await?;
    let items:Vec<Value>=sqlx::query_scalar(
        "SELECT jsonb_build_object('parcel_id',p.id,'reference',p.reference,'source_id',p.source_id,'feature_id',p.feature_id,
          'history',h.history,'revision',r.revision,'state',r.state,'geometry_current',coalesce(p.geometry_digest=r.geometry_digest,false))
         FROM house_numbering.parcel_geometry_v1 p
         JOIN LATERAL (SELECT revision,state,geometry_digest FROM house_numbering.parcel_building_links l
          WHERE (l.organization_id,l.project_id,l.parcel_id,l.building_id)=(p.organization_id,p.project_id,p.id,$3)
          ORDER BY revision DESC LIMIT 1) r ON true
         JOIN LATERAL (SELECT jsonb_agg(to_jsonb(l) ORDER BY l.revision) AS history FROM house_numbering.parcel_building_links l
          WHERE (l.organization_id,l.project_id,l.parcel_id,l.building_id)=(p.organization_id,p.project_id,p.id,$3)) h ON true
         WHERE p.organization_id=$1 AND p.project_id=$2 ORDER BY p.reference,p.id"
    ).bind(read.organization_id()).bind(read.project_id()).bind(building).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(json!({"items":items}))
}
pub async fn append_parcel_link(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,building:Uuid,parcel:Uuid,expected:i64,state:&str,evidence:&str)->Result<Value,AppError>{
    get_building(edit,building,true).await?;
    let link:Option<(Uuid,String)>=sqlx::query_as("SELECT source_id,feature_id FROM house_numbering.parcels WHERE organization_id=$1 AND project_id=$2 AND id=$3")
      .bind(edit.organization_id()).bind(edit.project_id()).bind(parcel).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let (source_id,feature_id)=link.ok_or_else(||AppError::not_found("parcel not found"))?;
    if matches!(state,"observed"|"confirmed"){
        let link=SpatialFeatureRef{source_id,feature_id};
        if !lock_linked_features(edit,std::slice::from_ref(&link),true).await?.contains(&link){return Err(AppError::conflict("PARCEL_STALE","parcel geometry missing"));}
    }
    let next=expected.checked_add(1).ok_or_else(||AppError::internal("parcel revision overflow"))?;
    let result=sqlx::query(
        "INSERT INTO house_numbering.parcel_building_links(organization_id,project_id,parcel_id,building_id,revision,state,evidence_ref,geometry_digest,actor_id)
         SELECT $1,$2,$3,$4,$5,$6,$7,CASE WHEN $6 IN ('observed','confirmed') THEN p.geometry_digest ELSE r.geometry_digest END,$8
         FROM house_numbering.parcel_geometry_v1 p LEFT JOIN LATERAL (
           SELECT revision,geometry_digest FROM house_numbering.parcel_building_links l WHERE (l.organization_id,l.project_id,l.parcel_id,l.building_id)=(p.organization_id,p.project_id,p.id,$4)
           ORDER BY revision DESC LIMIT 1) r ON true
         WHERE p.organization_id=$1 AND p.project_id=$2 AND p.id=$3 AND coalesce(r.revision,0)=$5-1
           AND CASE WHEN $6 IN ('observed','confirmed') THEN p.geometry_digest ELSE r.geometry_digest END IS NOT NULL"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(parcel).bind(building).bind(next).bind(state).bind(evidence).bind(actor.id)
      .execute(&mut **edit.transaction()).await.map_err(db_error)?;
    if result.rows_affected()!=1{return Err(AppError::conflict("STALE_PARCEL_LINK","parcel revision or geometry changed"));}
    append_audit(edit,actor,"house_numbering.parcel.link","house_numbering_building",building,json!({"parcel_id":parcel,"state":state,"revision":next})).await?;
    list_parcel_links(edit,building).await
}
