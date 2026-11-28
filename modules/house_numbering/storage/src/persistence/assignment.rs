//! SQL for reviewable building→route assignment, the unlinked-building audit,
//! alias/prefix recovery queues and cluster corrections. Matching rules live in
//! the module crate; these functions only read rows and apply confirmed plans.

use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use super::buildings::{get_building, official_number, update_building, BuildingUpdateInput};
use super::hierarchy::get_route;
use super::{append_audit, db_error, AuditActor};
use crate::catalog::ProjectRead;

/// Survey attribute names for road / alley / route code (`mapping.get(k, "")`).
#[derive(Debug, Clone, Copy)]
pub struct AttributeNames<'a> {
    pub road: &'a str,
    pub alley: &'a str,
    pub code: &'a str,
}

fn wires(rows: &[PgRow]) -> Result<Vec<Value>, AppError> {
    rows.iter()
        .map(|row| row.try_get::<Value, _>("wire").map_err(db_error))
        .collect()
}

fn pairs_json(pairs: &[(String, String)]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|(b, r)| json!({"building_id": b, "route_id": r}))
            .collect(),
    )
}

fn uuids(ids: &[String]) -> Vec<Uuid> {
    ids.iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect()
}

/// Every route with its resolved line revision and source names, ordered by id.
pub async fn assignment_routes(read: &mut ProjectRead<'_>) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(r) || jsonb_build_object(
                  'content_revision', l.content_revision,
                  'source_name', f.attributes->>'ten',
                  'planned_name', f.attributes->>'ten_quy_hoach') AS wire
         FROM house_numbering.routes r JOIN house_numbering.route_lines l
           ON (l.organization_id,l.project_id,l.route_id)=(r.organization_id,r.project_id,r.id)
         LEFT JOIN extensions.spatial_features_v1 f
           ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(r.organization_id,r.project_id,r.linked_source_id,r.linked_feature_id)
         WHERE r.organization_id=$1 AND r.project_id=$2 ORDER BY r.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Source route lines held back by the route-tree import (`review_reason`).
pub async fn pending_route_features(read: &mut ProjectRead<'_>) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('source_id',f.source_id,'feature_id',f.feature_id,
                  'code',f.attributes->>'route_code','reason',f.attributes->>'review_reason',
                  'parent_code',f.attributes->>'parent_code','name',f.attributes->>'ten') AS wire
         FROM extensions.spatial_features_v1 f JOIN extensions.spatial_sources_v1 s
           ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE f.organization_id=$1 AND f.project_id=$2 AND s.lifecycle='active'
           AND s.validation_state='ready' AND s.geometry_family='LineString'
           AND nullif(trim(f.attributes->>'route_code'),'') IS NOT NULL
           AND nullif(trim(f.attributes->>'review_reason'),'') IS NOT NULL
         ORDER BY f.source_id,f.feature_id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Active unlinked buildings with survey attributes for the audit.
pub async fn audit_buildings(
    read: &mut ProjectRead<'_>,
    source_id: Option<Uuid>,
    names: AttributeNames<'_>,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('id',b.id,'linked_source_id',b.linked_source_id,
           'linked_feature_id',b.linked_feature_id,'eligibility',b.eligibility,
           'road',f.attributes->>$3,'alley',f.attributes->>$4,'code',f.attributes->>$5,
           'alley_mapped',COALESCE(jsonb_exists(f.attributes,$4),false) AND $4 <> '',
           'geometry_usable',coalesce(f.geometry_state='present' AND NOT ST_IsEmpty(f.geometry) AND ST_SRID(f.geometry)>0
             AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon')
             AND s.lifecycle='active' AND s.validation_state='ready',false),
           'locked',EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
             AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('submitted','approved','reserved','proposed'))) AS wire
         FROM house_numbering.buildings b
         LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
         LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active' AND b.route_id IS NULL
           AND (CAST($6 AS uuid) IS NULL OR b.linked_source_id=$6)
         ORDER BY b.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(names.road)
    .bind(names.alley)
    .bind(names.code)
    .bind(source_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Point-on-surface → route line distance (metres) for still-unlinked buildings.
pub async fn pair_distances(
    read: &mut ProjectRead<'_>,
    pairs: &[(String, String)],
) -> Result<Vec<(String, Option<f64>)>, AppError> {
    let rows = sqlx::query(
        "WITH pairs AS MATERIALIZED (
           SELECT * FROM jsonb_to_recordset(CAST($3 AS jsonb)) AS p(building_id uuid, route_id uuid)
         )
         SELECT p.building_id::text AS building_id,
                ST_Distance(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,
                            ST_Transform(r.geometry,4326)::geography) AS distance
         FROM pairs p JOIN house_numbering.buildings b ON b.id=p.building_id
          AND b.organization_id=$1 AND b.project_id=$2 AND b.route_id IS NULL
         JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
         JOIN house_numbering.route_lines r ON r.route_id=p.route_id AND r.organization_id=$1 AND r.project_id=$2",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(pairs_json(pairs))
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("building_id").map_err(db_error)?,
                row.try_get("distance").map_err(db_error)?,
            ))
        })
        .collect()
}

/// One page of unlinked buildings for the assignment preview.
pub struct PreviewScope<'a> {
    pub source_id: Option<Uuid>,
    pub after_id: Option<Uuid>,
    pub limit: i64,
    pub building_ids: Option<&'a [Uuid]>,
}

pub async fn preview_buildings(
    read: &mut ProjectRead<'_>,
    scope: PreviewScope<'_>,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('id',b.id,'revision',b.revision,'linked_source_id',b.linked_source_id,
                  'linked_feature_id',b.linked_feature_id,'area_id',b.area_id,'attributes',f.attributes,
                  'content_revision',f.content_revision,
                  'geometry_usable',COALESCE(f.geometry_state='present' AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND ST_SRID(f.geometry)>0
                    AND s.lifecycle='active' AND s.validation_state='ready',false),
                  'locked',EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                    AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('submitted','approved','reserved'))) AS wire
         FROM house_numbering.buildings b LEFT JOIN extensions.spatial_features_v1 f
           ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
         LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active' AND b.route_id IS NULL
           AND (CAST($3 AS uuid) IS NULL OR b.linked_source_id=$3)
           AND (CAST($4 AS uuid) IS NULL OR b.id>$4)
           AND (CAST($5 AS uuid[]) IS NULL OR b.id=ANY(CAST($5 AS uuid[])))
         ORDER BY b.id LIMIT $6",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(scope.source_id)
    .bind(scope.after_id)
    .bind(scope.building_ids)
    .bind(scope.limit)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Inputs of the preview distance query (nearest routes ∪ textual candidates).
pub struct PreviewDistances<'a> {
    pub ids: &'a [String],
    pub route_ids: &'a [String],
    pub pairs: &'a [(String, String)],
    pub max_distance: f64,
    pub strict: bool,
}

pub async fn preview_distances(
    read: &mut ProjectRead<'_>,
    input: PreviewDistances<'_>,
) -> Result<Vec<(String, String, f64)>, AppError> {
    let rows = sqlx::query(
        "WITH buildings AS MATERIALIZED (
           SELECT b.id,ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography g
           FROM house_numbering.buildings b JOIN extensions.spatial_features_v1 f
             ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
           WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY(CAST($3 AS uuid[]))
         ), routes AS MATERIALIZED (
           SELECT r.id, ST_Transform(f.geometry,4326)::geography g
           FROM house_numbering.routes r JOIN house_numbering.route_lines f
             ON (f.organization_id,f.project_id,f.route_id)=(r.organization_id,r.project_id,r.id)
           WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=ANY(CAST($4 AS uuid[]))
         ), nearest AS (
           SELECT b.id building_id,c.id route_id,c.distance FROM buildings b CROSS JOIN LATERAL (
             SELECT r.id,ST_Distance(b.g,r.g) distance FROM routes r
             WHERE NOT $7 AND ST_DWithin(b.g,r.g,$6) ORDER BY distance,r.id LIMIT 5
           ) c
         ), candidates AS (
           SELECT b.id AS building_id,r.id AS route_id, ST_Distance(b.g,r.g) AS distance
           FROM jsonb_to_recordset(CAST($5 AS jsonb)) AS p(building_id uuid,route_id uuid)
           JOIN buildings b ON b.id=p.building_id JOIN routes r ON r.id=p.route_id
         ), merged AS (SELECT * FROM nearest UNION SELECT * FROM candidates)
         SELECT building_id::text AS building_id, route_id::text AS route_id, distance FROM merged",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(uuids(input.ids))
    .bind(uuids(input.route_ids))
    .bind(pairs_json(input.pairs))
    .bind(input.max_distance)
    .bind(input.strict)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("building_id").map_err(db_error)?,
                row.try_get("route_id").map_err(db_error)?,
                row.try_get("distance").map_err(db_error)?,
            ))
        })
        .collect()
}

/// Buildings of one correction batch (any state) with survey attributes.
pub async fn correction_buildings(
    read: &mut ProjectRead<'_>,
    ids: &[Uuid],
    names: AttributeNames<'_>,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('id',b.id,'revision',b.revision,'route_id',b.route_id,'state',b.state,
           'eligibility',b.eligibility,'linked_source_id',b.linked_source_id,'linked_feature_id',b.linked_feature_id,
           'content_revision',f.content_revision,'road',f.attributes->>$4,'alley',f.attributes->>$5,'code',f.attributes->>$6,
           'alley_mapped',COALESCE(jsonb_exists(f.attributes,$5),false) AND $5<>'',
           'geometry_usable',COALESCE(f.geometry_state='present' AND NOT ST_IsEmpty(f.geometry) AND ST_SRID(f.geometry)>0
              AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND s.lifecycle='active' AND s.validation_state='ready',false),
           'locked',EXISTS(SELECT 1 FROM house_numbering.address_numbers n
                 WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id AND n.building_id=b.id
                   AND n.retired_at IS NULL AND n.kind IN ('submitted','approved','reserved','proposed'))) AS wire
         FROM house_numbering.buildings b
         LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
         LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY(CAST($3 AS uuid[]))
         ORDER BY b.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(ids)
    .bind(names.road)
    .bind(names.alley)
    .bind(names.code)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Every (eligible building, route) distance for a correction batch.
pub async fn correction_distances(
    read: &mut ProjectRead<'_>,
    eligible: &[String],
    routes: &[String],
) -> Result<Vec<(String, String, Option<f64>)>, AppError> {
    let rows = sqlx::query(
        "WITH buildings AS MATERIALIZED (
             SELECT b.id,ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography g
             FROM house_numbering.buildings b JOIN extensions.spatial_features_v1 f
               ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
             WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY(CAST($3 AS uuid[]))
         ), routes AS MATERIALIZED (
             SELECT route_id,ST_Transform(geometry,4326)::geography g
             FROM house_numbering.route_lines WHERE organization_id=$1 AND project_id=$2 AND route_id=ANY(CAST($4 AS uuid[]))
         )
         SELECT b.id::text AS id,r.route_id::text AS route_id,ST_Distance(b.g,r.g) distance FROM buildings b CROSS JOIN routes r",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(uuids(eligible))
    .bind(uuids(routes))
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("id").map_err(db_error)?,
                row.try_get("route_id").map_err(db_error)?,
                row.try_get("distance").map_err(db_error)?,
            ))
        })
        .collect()
}

/// Named source lines that could become missing address paths.
pub async fn catalog_lines(read: &mut ProjectRead<'_>) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('source_id',f.source_id,'feature_id',f.feature_id,'name',f.attributes->>'ten',
           'prefix',f.attributes->>'prefix','route_type',f.attributes->>'route_type',
           'code',f.attributes->>'route_code','review_reason',f.attributes->>'review_reason') AS wire
         FROM extensions.spatial_features_v1 f JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE f.organization_id=$1 AND f.project_id=$2 AND s.lifecycle='active' AND s.validation_state='ready'
           AND f.geometry_state='present' AND ST_GeometryType(f.geometry)='ST_LineString' AND ST_SRID(f.geometry)>0
           AND nullif(f.attributes->>'ten','') IS NOT NULL
         ORDER BY f.source_id,f.feature_id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Every active linked building with survey attributes (catalog observations).
pub async fn catalog_observations(
    read: &mut ProjectRead<'_>,
    source_id: Option<Uuid>,
    names: AttributeNames<'_>,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('id',b.id,'revision',b.revision,'eligibility',b.eligibility,'route_id',b.route_id,
           'content_revision',f.content_revision,
           'road',f.attributes->>$3,'alley',f.attributes->>$4,'code',f.attributes->>$5,
           'mapped',COALESCE(jsonb_exists(f.attributes,$4),false) AND $4<>'',
           'usable',COALESCE(f.geometry_state='present' AND NOT ST_IsEmpty(f.geometry) AND ST_SRID(f.geometry)>0
             AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND s.lifecycle='active' AND s.validation_state='ready',false),
           'locked',EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
             AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('proposed','submitted','approved','reserved'))) AS wire
         FROM house_numbering.buildings b JOIN extensions.spatial_features_v1 f
          ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
         JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active'
           AND (CAST($6 AS uuid) IS NULL OR b.linked_source_id=$6)
         ORDER BY b.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(names.road)
    .bind(names.alley)
    .bind(names.code)
    .bind(source_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

const MERGED_LINES: &str = "
    WITH specs AS MATERIALIZED (SELECT v->>'id' id,v->'links' links,v->'building_ids' ids FROM jsonb_array_elements(CAST($3 AS jsonb)) v),
    merged AS MATERIALIZED (
      SELECT s.id,house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) geom
      FROM specs s CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY l(v,ord)
      JOIN extensions.spatial_features_v1 f ON f.organization_id=$1 AND f.project_id=$2 AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
      GROUP BY s.id HAVING count(*)=max(jsonb_array_length(s.links))
    ), lines AS MATERIALIZED (SELECT id,ST_Transform(geom,4326)::geography g FROM merged WHERE geom IS NOT NULL)
";

/// `(group id, merged line exists, eligible buildings within 35 m)` per spec.
pub async fn prefix_spec_stats(
    read: &mut ProjectRead<'_>,
    specs: &[Value],
) -> Result<Vec<(String, bool, i64)>, AppError> {
    let sql = format!(
        "{MERGED_LINES}
        SELECT s.id,l.id IS NOT NULL connected,count(*) FILTER (WHERE b.eligibility<>'ineligible' AND
            ST_DWithin(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,l.g,35)) near
        FROM specs s LEFT JOIN lines l ON s.id=l.id
        CROSS JOIN LATERAL jsonb_array_elements_text(s.ids) bid(val)
        JOIN house_numbering.buildings b ON b.organization_id=$1 AND b.project_id=$2 AND b.id=bid.val::uuid
        JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        GROUP BY s.id,l.id"
    );
    let rows = sqlx::query(&sql)
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(Value::Array(specs.to_vec()))
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("id").map_err(db_error)?,
                row.try_get("connected").map_err(db_error)?,
                row.try_get("near").map_err(db_error)?,
            ))
        })
        .collect()
}

/// `(group id, parent route id, endpoint→parent metres)` per junction pair.
pub async fn prefix_junctions(
    read: &mut ProjectRead<'_>,
    specs: &[Value],
    pairs: &[Value],
) -> Result<Vec<(String, String, Option<f64>)>, AppError> {
    let sql = format!(
        "{MERGED_LINES}
        , comparisons AS (SELECT * FROM jsonb_to_recordset(CAST($4 AS jsonb)) AS x(id text,parent_id uuid))
        SELECT c.id,c.parent_id::text AS parent_id,LEAST(ST_Distance(ST_StartPoint(a.g::geometry)::geography,ST_Transform(l.geometry,4326)::geography),
          ST_Distance(ST_EndPoint(a.g::geometry)::geography,ST_Transform(l.geometry,4326)::geography)) distance
        FROM comparisons c JOIN lines a ON a.id=c.id JOIN house_numbering.route_lines l ON l.organization_id=$1 AND l.project_id=$2 AND l.route_id=c.parent_id"
    );
    let rows = sqlx::query(&sql)
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(Value::Array(specs.to_vec()))
        .bind(Value::Array(pairs.to_vec()))
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("id").map_err(db_error)?,
                row.try_get("parent_id").map_err(db_error)?,
                row.try_get("distance").map_err(db_error)?,
            ))
        })
        .collect()
}

/// Serialize assignment writers of one project (`pg_advisory_xact_lock`).
pub async fn assignment_lock(edit: &mut ProjectRead<'_>) -> Result<(), AppError> {
    let key = format!("house-numbering-assignment:{}", edit.project_id());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    Ok(())
}

/// `SELECT * FROM routes ... FOR UPDATE` as wire JSON (404 "route not found").
pub async fn route_for_update(
    edit: &mut ProjectRead<'_>,
    route_id: Uuid,
) -> Result<Value, AppError> {
    get_route(edit, route_id, true).await
}

/// How a confirmed plan rejects a building that changed since preview.
#[derive(Debug, Clone, Copy)]
pub struct StaleGuard {
    pub code: &'static str,
    pub message: &'static str,
    /// Also refuse buildings holding a proposed/official number (message).
    pub protected: Option<&'static str>,
}

/// Link one still-unlinked building to its confirmed route via `update_building`.
pub async fn assign_building_route(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    expected_revision: i64,
    route_id: Uuid,
    guard: StaleGuard,
) -> Result<(), AppError> {
    let current = get_building(edit, building_id, true).await?;
    if !current["route_id"].is_null() || current["revision"].as_i64() != Some(expected_revision) {
        return Err(AppError::conflict(guard.code, guard.message));
    }
    if let Some(message) = guard.protected {
        let kinds = ["proposed", "submitted", "approved", "reserved"];
        if official_number(edit, building_id, &kinds).await? {
            return Err(AppError::conflict(guard.code, message));
        }
    }
    let id = |key: &str| current[key].as_str().and_then(|v| Uuid::parse_str(v).ok());
    update_building(
        edit,
        actor,
        building_id,
        BuildingUpdateInput {
            expected_revision,
            route_id: Some(route_id),
            area_id: id("area_id"),
            parcel_ref: current["parcel_ref"].as_str(),
            source_id: id("linked_source_id").unwrap_or_default(),
            feature_id: current["linked_feature_id"].as_str().unwrap_or_default(),
        },
    )
    .await?;
    Ok(())
}

/// Replace a route's reviewed aliases and bump its revision.
pub async fn save_route_aliases(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    route_id: Uuid,
    aliases: Value,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE house_numbering.routes SET reviewed_aliases=CAST($4 AS jsonb),revision=revision+1,
            updated_by_actor_id=$5,updated_at=now()
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .bind(aliases)
    .bind(actor.id)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

/// Append one project audit event (`repo.audit`).
pub async fn audit_event(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    action: &str,
    resource: (&str, Uuid),
    metadata: Value,
) -> Result<(), AppError> {
    append_audit(edit, actor, action, resource.0, resource.1, metadata).await
}
