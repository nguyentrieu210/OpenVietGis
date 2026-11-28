use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::db_error;

/// Read both explicitly selected GIS point sources in a single SQL snapshot.
pub async fn score_inputs(read:&mut ProjectRead<'_>,points:Uuid,grid:Uuid,point_number:&str,point_road:&str,grid_number:&str,grid_road:&str,metric_crs:i32)->Result<Value,AppError>{
    let rows:Vec<Value>=sqlx::query_scalar(
        "WITH requested AS (
           SELECT * FROM (VALUES ('points',$3::uuid,$5::text,$6::text),('grid',$4::uuid,$7::text,$8::text)) q(role,source_id,number_field,road_field)
         )
         SELECT jsonb_build_object('role',q.role,'source_id',s.id,'revision',s.content_revision,
           'items',coalesce(rows.items,'[]'::jsonb),'number_field_found',coalesce(rows.number_field_found,false),
           'road_field_found',coalesce(rows.road_field_found,false))
         FROM requested q JOIN extensions.spatial_sources_v1 s
          ON s.organization_id=$1 AND s.project_id=$2 AND s.id=q.source_id
          AND s.lifecycle='active' AND s.validation_state='ready' AND s.geometry_family='Point'
         CROSS JOIN LATERAL (
           SELECT jsonb_agg(jsonb_build_object('id',f.feature_id,'number',left(f.attributes->>q.number_field,200),
             'road',left(f.attributes->>q.road_field,200),'x',ST_X(f.pt),'y',ST_Y(f.pt)) ORDER BY f.feature_id) AS items,
             bool_or(f.attributes ? q.number_field) AS number_field_found,bool_or(f.attributes ? q.road_field) AS road_field_found
           FROM (SELECT f.feature_id,f.attributes,
             CASE WHEN f.geometry_state='present' AND ST_GeometryType(f.geometry)='ST_Point' AND ST_SRID(f.geometry)>0
               AND NOT ST_IsEmpty(f.geometry) THEN ST_Transform(f.geometry,$9) END AS pt
             FROM extensions.spatial_features_v1 f
             WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=s.id
             ORDER BY f.feature_id LIMIT 50001) f
         ) rows ORDER BY q.role"
    ).bind(read.organization_id()).bind(read.project_id()).bind(points).bind(grid)
     .bind(point_number).bind(point_road).bind(grid_number).bind(grid_road).bind(metric_crs)
     .fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    if rows.len()!=2{return Err(AppError::validation("SCORE_SOURCE_UNAVAILABLE","select two ready point sources in this project"));}
    for r in &rows{
        let count=r["items"].as_array().map(Vec::len).unwrap_or(0);
        if count>50000{return Err(AppError::validation("SCORE_SOURCE_LIMIT","each source supports at most 50000 points"));}
        if count>0&&(r["number_field_found"]!=true||r["road_field_found"]!=true){
            return Err(AppError::validation("SCORE_FIELD_NOT_FOUND","select existing number and road fields"));
        }
    }
    Ok(json!({"grid":rows[0],"points":rows[1]}))
}
