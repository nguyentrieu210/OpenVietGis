//! Conservative, read-only location hints for unmatched certificates.
//! Interpolation is not a building identity or an approved address.
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::db_error;

pub async fn certificate_locations(read:&mut ProjectRead<'_>,route:Uuid,metric_crs:i32,offset:i64)->Result<Value,AppError>{
    let rows:Vec<Value>=sqlx::query_scalar(
        "WITH line AS MATERIALIZED (
           SELECT r.id,ST_Transform(f.geometry,$4) AS geom
           FROM house_numbering.routes r JOIN house_numbering.route_lines f
             ON (f.organization_id,f.project_id,f.route_id)=(r.organization_id,r.project_id,r.id)
           WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3 AND r.status='confirmed'
             AND r.route_type='street' AND ST_GeometryType(f.geometry)='ST_LineString'
             AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry) AND ST_IsSimple(f.geometry)
         ), targets AS MATERIALIZED (
           SELECT e.id,e.raw_number,CASE WHEN btrim(e.raw_number) ~ '^[0-9]{1,5}$'
             THEN btrim(e.raw_number)::integer END AS num
           FROM house_numbering.evidence e
           WHERE e.organization_id=$1 AND e.project_id=$2 AND e.route_id=$3
             AND e.origin='certificate' AND e.building_id IS NULL
           ORDER BY e.id LIMIT 201 OFFSET $5
         ), anchor_rows AS MATERIALIZED (
           SELECT e.building_id,CASE WHEN btrim(e.raw_number) ~ '^[0-9]{1,5}$'
             THEN btrim(e.raw_number)::integer END AS num,
             ST_Transform(ST_PointOnSurface(f.geometry),$4) AS pt
           FROM house_numbering.evidence e
           JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(e.organization_id,e.project_id,e.building_id)
           JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
           JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
           WHERE e.organization_id=$1 AND e.project_id=$2 AND e.route_id=$3 AND e.origin='certificate'
             AND e.confidence<>'estimated' AND b.route_id=$3 AND b.state='active' AND b.eligibility<>'ineligible'
             AND f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready'
             AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
             AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon','ST_MultiPolygon')
             AND NOT EXISTS(SELECT 1 FROM house_numbering.address_numbers n
               WHERE n.organization_id=$1 AND n.project_id=$2 AND n.building_id=b.id AND n.kind='approved'
                 AND (n.retired_at IS NOT NULL OR upper(btrim(n.number_text))<>upper(btrim(e.raw_number))))
         ), unique_numbers AS (
           SELECT num,min(building_id::text)::uuid AS building_id FROM anchor_rows WHERE num>0
           GROUP BY num HAVING count(DISTINCT building_id)=1
         ), unique_buildings AS (
           SELECT building_id FROM anchor_rows WHERE num>0 GROUP BY building_id HAVING count(DISTINCT num)=1
         ), projected AS MATERIALIZED (
           SELECT DISTINCT ON (a.num) a.num,a.building_id,a.pt,l.geom,ST_Length(l.geom) AS len,
             ST_LineLocatePoint(l.geom,a.pt) AS fraction
           FROM anchor_rows a JOIN unique_numbers u USING(num,building_id)
           JOIN unique_buildings b USING(building_id) CROSS JOIN line l
           WHERE ST_Length(l.geom)>2 AND ST_DWithin(l.geom,a.pt,40)
           ORDER BY a.num
         ), tangent AS (
           SELECT p.*,ST_LineInterpolatePoint(geom,fraction) AS center,
             ST_LineInterpolatePoint(geom,greatest(0,fraction-1/len)) AS tail,
             ST_LineInterpolatePoint(geom,least(1,fraction+1/len)) AS head
           FROM projected p
         ), anchors AS MATERIALIZED (
           SELECT num,building_id,fraction*len AS measure,
             ((ST_X(head)-ST_X(tail))*(ST_Y(pt)-ST_Y(center))-
              (ST_Y(head)-ST_Y(tail))*(ST_X(pt)-ST_X(center)))/
                nullif(ST_Distance(head,tail),0) AS lateral_m
           FROM tangent
         ), bracketed AS (
           SELECT t.*,lo.num AS lo_num,hi.num AS hi_num,lo.building_id AS lo_id,hi.building_id AS hi_id,
             lo.measure AS lo_measure,hi.measure AS hi_measure,lo.lateral_m AS lo_side,hi.lateral_m AS hi_side,
             l.geom,ST_Length(l.geom) AS len
           FROM targets t LEFT JOIN line l ON true
           LEFT JOIN LATERAL (SELECT * FROM anchors a WHERE a.num<t.num AND mod(a.num,2)=mod(t.num,2) ORDER BY a.num DESC LIMIT 1) lo ON true
           LEFT JOIN LATERAL (SELECT * FROM anchors a WHERE a.num>t.num AND mod(a.num,2)=mod(t.num,2) ORDER BY a.num LIMIT 1) hi ON true
         ), checked AS (
           SELECT b.*,CASE
             WHEN num IS NULL OR num<=0 THEN 'unsupported_number'
             WHEN geom IS NULL THEN 'route_unavailable'
             WHEN EXISTS(SELECT 1 FROM house_numbering.evidence e
               WHERE e.organization_id=$1 AND e.project_id=$2 AND e.route_id=$3 AND e.origin='certificate'
                 AND e.building_id IS NOT NULL AND CASE WHEN btrim(e.raw_number) ~ '^[0-9]{1,5}$' THEN btrim(e.raw_number)::integer END=b.num)
               THEN 'number_already_linked'
             WHEN lo_num IS NULL OR hi_num IS NULL THEN 'missing_bracket'
             WHEN hi_num-lo_num>200 THEN 'bracket_too_wide'
             WHEN lo_side IS NULL OR hi_side IS NULL OR abs(lo_side)<0.5 OR abs(hi_side)<0.5 OR lo_side*hi_side<=0 THEN 'different_or_uncertain_side'
             WHEN abs(hi_measure-lo_measure)/((hi_num-lo_num)/2.0) NOT BETWEEN 1.5 AND 60 THEN 'implausible_spacing'
             ELSE 'estimated' END AS status,
             (num-lo_num)::float8/nullif(hi_num-lo_num,0) AS weight
           FROM bracketed b
         ), interpolated AS (
           SELECT c.*,CASE WHEN status='estimated' THEN (lo_measure+weight*(hi_measure-lo_measure))/nullif(len,0) END AS f,
             CASE WHEN status='estimated' THEN lo_side+weight*(hi_side-lo_side) END AS offset_m
           FROM checked c
         ), coordinates AS (
           SELECT i.*,CASE WHEN f IS NOT NULL THEN ST_LineInterpolatePoint(geom,f) END AS center,
             CASE WHEN f IS NOT NULL THEN ST_LineInterpolatePoint(geom,greatest(0,f-1/len)) END AS tail,
             CASE WHEN f IS NOT NULL THEN ST_LineInterpolatePoint(geom,least(1,f+1/len)) END AS head
           FROM interpolated i
         ), locations AS (
           SELECT c.*,CASE WHEN status='estimated' AND ST_Distance(head,tail)>0
             THEN ST_Transform(ST_SetSRID(ST_MakePoint(
               ST_X(center)-(ST_Y(head)-ST_Y(tail))*offset_m/ST_Distance(head,tail),
               ST_Y(center)+(ST_X(head)-ST_X(tail))*offset_m/ST_Distance(head,tail)),$4),4326) END AS position
           FROM coordinates c
         )
         SELECT jsonb_build_object('evidence_id',id,'number',raw_number,
           'status',CASE WHEN status='estimated' AND position IS NULL THEN 'route_unavailable' ELSE status END,
           'lower_number',lo_num,'upper_number',hi_num,'lower_building_id',lo_id,'upper_building_id',hi_id,
           'longitude',ST_X(position),'latitude',ST_Y(position),'confidence','estimated')
         FROM locations ORDER BY id"
    ).bind(read.organization_id()).bind(read.project_id()).bind(route).bind(metric_crs).bind(offset)
    .fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    let mut items=rows;
    let next=if items.len()>200{items.pop();Some(offset+200)}else{None};
    Ok(json!({"items":items,"next_offset":next,"route_id":route,"metric_crs":metric_crs,"method":"same_side_bracket_v1"}))
}
