use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::db_error;

/// Read both explicitly selected GIS point sources in a single SQL snapshot.
pub async fn score_inputs(read:&mut ProjectRead<'_>,points:Uuid,grid:Option<Uuid>,point_number:&str,point_road:&str,grid_number:&str,grid_road:&str,metric_crs:i32)->Result<Value,AppError>{
    let rows:Vec<Value>=sqlx::query_scalar(
        "WITH requested AS (
           SELECT * FROM (VALUES ('points',$3::uuid,$5::text,$6::text),('grid',$4::uuid,$7::text,$8::text)) q(role,source_id,number_field,road_field)
         ), registry_candidates AS (
           SELECT b.id AS building_id,n.number_text AS number,r.name AS road,n.kind AS origin,
              CASE n.kind WHEN 'approved' THEN 0 WHEN 'certificate' THEN 1 ELSE 2 END AS priority
           FROM house_numbering.buildings b JOIN house_numbering.address_numbers n
             ON (n.organization_id,n.project_id,n.building_id,n.route_id)=(b.organization_id,b.project_id,b.id,b.route_id)
           JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
           WHERE $4::uuid IS NULL AND b.organization_id=$1 AND b.project_id=$2 AND b.state='active'
             AND b.eligibility<>'ineligible' AND n.retired_at IS NULL AND n.kind IN ('approved','certificate','cad')
           UNION ALL
           SELECT b.id,e.raw_number,r.name,e.origin,CASE e.origin WHEN 'certificate' THEN 1 ELSE 2 END
           FROM house_numbering.buildings b JOIN house_numbering.evidence e
             ON (e.organization_id,e.project_id,e.building_id)=(b.organization_id,b.project_id,b.id)
           LEFT JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(b.organization_id,b.project_id,coalesce(e.route_id,b.route_id))
           WHERE $4::uuid IS NULL AND b.organization_id=$1 AND b.project_id=$2 AND b.state='active'
             AND b.eligibility<>'ineligible' AND e.origin IN ('certificate','cad') AND e.confidence<>'estimated'
             AND e.raw_number IS NOT NULL AND btrim(e.raw_number)<>''
             AND (e.route_id IS NULL OR e.route_id=b.route_id)
         ), ranked AS (
           SELECT c.*,min(priority) OVER(PARTITION BY building_id) AS best_priority FROM registry_candidates c
           WHERE c.origin='approved' OR NOT EXISTS(SELECT 1 FROM house_numbering.address_numbers old
             WHERE old.organization_id=$1 AND old.project_id=$2 AND old.building_id=c.building_id
               AND old.kind='approved' AND old.retired_at IS NOT NULL
               AND translate(btrim(old.number_text),'abcdefghijklmnopqrstuvwxyz','ABCDEFGHIJKLMNOPQRSTUVWXYZ')=
                   translate(btrim(c.number),'abcdefghijklmnopqrstuvwxyz','ABCDEFGHIJKLMNOPQRSTUVWXYZ'))
         ), chosen AS (
           SELECT building_id,min(number) AS number,min(road) AS road,min(origin) AS origin
           FROM ranked WHERE priority=best_priority
           GROUP BY building_id
           HAVING count(DISTINCT translate(btrim(number),'abcdefghijklmnopqrstuvwxyz','ABCDEFGHIJKLMNOPQRSTUVWXYZ'))=1
         ), registry_rows AS (
           SELECT c.building_id::text AS id,c.number,c.road,c.origin,f.source_id,f.feature_id,
             ST_Transform(ST_PointOnSurface(f.geometry),$9) AS pt
           FROM chosen c JOIN house_numbering.buildings b ON b.organization_id=$1 AND b.project_id=$2 AND b.id=c.building_id
           JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
           JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
           WHERE f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready'
             AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
             AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon','ST_MultiPolygon')
           ORDER BY c.building_id LIMIT 50001
         ), supplied AS (
         SELECT q.role,jsonb_build_object('role',q.role,'source_id',s.id,'revision',s.content_revision,
           'items',coalesce(rows.items,'[]'::jsonb),'number_field_found',coalesce(rows.number_field_found,false),
           'road_field_found',coalesce(rows.road_field_found,false)) AS wire
         FROM requested q JOIN extensions.spatial_sources_v1 s
          ON s.organization_id=$1 AND s.project_id=$2 AND s.id=q.source_id
          AND s.lifecycle='active' AND s.validation_state='ready' AND s.geometry_family='Point'
         CROSS JOIN LATERAL (
           SELECT jsonb_agg(jsonb_build_object('id',f.feature_id,'number',left(f.attributes->>q.number_field,200),
             'road',left(f.attributes->>q.road_field,200),'source_id',s.id,'feature_id',f.feature_id,'x',ST_X(f.pt),'y',ST_Y(f.pt)) ORDER BY f.feature_id) AS items,
             bool_or(f.attributes ? q.number_field) AS number_field_found,bool_or(f.attributes ? q.road_field) AS road_field_found
           FROM (SELECT f.feature_id,f.attributes,
             CASE WHEN f.geometry_state='present' AND ST_GeometryType(f.geometry)='ST_Point' AND ST_SRID(f.geometry)>0
               AND NOT ST_IsEmpty(f.geometry) THEN ST_Transform(f.geometry,$9) END AS pt
             FROM extensions.spatial_features_v1 f
             WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=s.id
             ORDER BY f.feature_id LIMIT 50001) f
         ) rows
         ), registry AS (
           SELECT 'grid' AS role,jsonb_build_object('role','grid','source_id',NULL,'revision',NULL,
             'items',coalesce(jsonb_agg(jsonb_build_object('id',id,'number',number,'road',road,'origin',origin,
               'source_id',source_id,'feature_id',feature_id,'x',ST_X(pt),'y',ST_Y(pt)) ORDER BY id),'[]'::jsonb),
             'number_field_found',true,'road_field_found',true,
             'ambiguous_buildings',(SELECT count(DISTINCT building_id) FROM ranked WHERE priority=best_priority) -(SELECT count(*) FROM chosen)) AS wire
           FROM registry_rows WHERE $4::uuid IS NULL
         )
         SELECT wire FROM (SELECT * FROM supplied UNION ALL SELECT * FROM registry WHERE $4::uuid IS NULL) q ORDER BY role"
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
