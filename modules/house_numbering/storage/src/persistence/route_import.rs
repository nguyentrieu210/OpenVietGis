//! SQL for route-tree import, staged geometry reviews, address verification and
//! verified links. Planning rules live in the module crate; these functions only read
//! features, routes and measured distances, and write confirmed decisions.

use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use super::buildings::{get_building, update_building, BuildingUpdateInput};
use super::hierarchy::{validate_route_segments, RouteSegmentInput};
use super::{append_audit, db_error, AuditActor};
use crate::catalog::ProjectRead;

/// `(child key, parent key, parent geometry revision, endpoint distance m)`.
pub type Junction = (String, String, Value, Option<f64>);

fn wires(rows: &[PgRow]) -> Result<Vec<Value>, AppError> {
    rows.iter()
        .map(|row| row.try_get::<Value, _>("wire").map_err(db_error))
        .collect()
}

fn one_wire(row: Option<PgRow>) -> Result<Option<Value>, AppError> {
    row.map(|row| row.try_get::<Value, _>("wire").map_err(db_error))
        .transpose()
}

/// `pg_advisory_xact_lock` on `house-numbering-{name}:{project}`.
pub async fn project_lock(edit: &mut ProjectRead<'_>, name: &str) -> Result<(), AppError> {
    let key = format!("house-numbering-{name}:{}", edit.project_id());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    Ok(())
}

/// Line features of one ready source, ordered by feature id.
pub async fn route_import_features(
    read: &mut ProjectRead<'_>,
    source_id: Uuid,
    limit: i64,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('feature_id',f.feature_id,'attributes',f.attributes,
                  'content_revision',f.content_revision,'geometry_state',f.geometry_state,
                  'geometry_type',ST_GeometryType(f.geometry),'srid',ST_SRID(f.geometry)) AS wire
         FROM extensions.spatial_sources_v1 s JOIN extensions.spatial_features_v1 f
           ON f.organization_id=s.organization_id AND f.project_id=s.project_id AND f.source_id=s.id
         WHERE s.organization_id=$1 AND s.project_id=$2 AND s.id=$3
           AND s.lifecycle='active' AND s.validation_state='ready'
         ORDER BY f.feature_id LIMIT $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(source_id)
    .bind(limit)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Ids of merge specs whose segments resolve to one continuous ready line.
pub async fn merge_connected(
    read: &mut ProjectRead<'_>,
    specs: Vec<Value>,
) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar(
        "WITH specs AS (SELECT v->>'id' id,v->'links' links FROM jsonb_array_elements(CAST($3 AS jsonb)) v),
         merged AS (
           SELECT s.id,house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) g
           FROM specs s CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY l(v,ord)
           LEFT JOIN extensions.spatial_features_v1 f ON f.organization_id=$1 AND f.project_id=$2
              AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
           LEFT JOIN extensions.spatial_sources_v1 ds ON ds.organization_id=f.organization_id AND ds.project_id=f.project_id AND ds.id=f.source_id
           GROUP BY s.id HAVING bool_and(COALESCE(f.geometry_state='present' AND ds.lifecycle='active' AND ds.validation_state='ready',false))
         ) SELECT id FROM merged WHERE g IS NOT NULL",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(Value::Array(specs))
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)
}

/// One ready line feature offered as a missing parent route.
pub async fn parent_feature(
    read: &mut ProjectRead<'_>,
    source_id: Uuid,
    feature_id: &str,
) -> Result<Option<Value>, AppError> {
    let row = sqlx::query(
        "SELECT jsonb_build_object('content_revision',f.content_revision,'geometry_state',f.geometry_state,
                  'geometry_type',ST_GeometryType(f.geometry),'srid',ST_SRID(f.geometry)) AS wire
         FROM extensions.spatial_features_v1 f JOIN extensions.spatial_sources_v1 ds
           ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3 AND f.feature_id=$4
           AND ds.lifecycle='active' AND ds.validation_state='ready'",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(source_id)
    .bind(feature_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    one_wire(row)
}

/// `validate_route_segments` as `create_route` would run it.
pub async fn check_route_segments(
    edit: &mut ProjectRead<'_>,
    source_id: Uuid,
    feature_id: &str,
    segments: &[RouteSegmentInput<'_>],
) -> Result<(), AppError> {
    validate_route_segments(edit, source_id, feature_id, segments)
        .await
        .map(|_| ())
}

/// Endpoint→line distances between planned/existing child and parent candidates.
pub async fn parent_distances(
    read: &mut ProjectRead<'_>,
    specs: Vec<Value>,
    pairs: &[(String, String)],
    existing_ids: &[String],
) -> Result<Vec<Junction>, AppError> {
    let pairs: Vec<Value> = pairs
        .iter()
        .map(|(child, parent)| json!({"child": child, "parent": parent}))
        .collect();
    let rows = sqlx::query(
        "WITH specs AS MATERIALIZED (
           SELECT v->>'key' key,v->'links' links FROM jsonb_array_elements(CAST($3 AS jsonb)) v
         ), planned_lines AS MATERIALIZED (
           SELECT s.key,house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) geom,
                  string_agg(f.content_revision,'|' ORDER BY l.ord) AS revision
           FROM specs s CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY l(v,ord)
           LEFT JOIN extensions.spatial_features_v1 f ON f.organization_id=$1 AND f.project_id=$2
             AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
           LEFT JOIN extensions.spatial_sources_v1 ds ON ds.organization_id=f.organization_id AND ds.project_id=f.project_id AND ds.id=f.source_id
           GROUP BY s.key HAVING bool_and(COALESCE(f.geometry_state='present' AND ds.lifecycle='active' AND ds.validation_state='ready',false))
         ), lines AS MATERIALIZED (
           SELECT key,ST_Transform(geom,4326)::geography g,revision FROM planned_lines WHERE geom IS NOT NULL
           UNION ALL
           SELECT route_id::text,ST_Transform(geometry,4326)::geography,content_revision FROM house_numbering.route_lines
           WHERE organization_id=$1 AND project_id=$2 AND route_id::text=ANY($5)
         ), comparisons AS (
           SELECT v->>'child' child,v->>'parent' parent FROM jsonb_array_elements(CAST($4 AS jsonb)) v
         )
         SELECT c.child,c.parent,to_jsonb(b.revision) AS revision,LEAST(ST_Distance(ST_StartPoint(a.g::geometry)::geography,b.g),
                 ST_Distance(ST_EndPoint(a.g::geometry)::geography,b.g)) distance
         FROM comparisons c JOIN lines a ON a.key=c.child JOIN lines b ON b.key=c.parent",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(Value::Array(specs))
    .bind(Value::Array(pairs))
    .bind(existing_ids)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("child").map_err(db_error)?,
                row.try_get("parent").map_err(db_error)?,
                row.try_get::<Option<Value>, _>("revision")
                    .map_err(db_error)?
                    .unwrap_or(Value::Null),
                row.try_get("distance").map_err(db_error)?,
            ))
        })
        .collect()
}

/// Aggregated legacy survey attributes of active buildings by route code.
pub async fn survey_observations(
    read: &mut ProjectRead<'_>,
    codes: &[String],
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('code',code,'road',road,'alley',alley,'unassigned',unassigned,
                  'amount',amount,'example_sources',example_sources,'example_features',example_features) AS wire
         FROM (
           SELECT f.attributes->>'tuyen_id' code, f.attributes->>'ten_duong' road,
                  f.attributes->>'hem' alley, b.route_id IS NULL unassigned, count(*) amount,
                  (array_agg(b.linked_source_id::text ORDER BY b.id))[1:20] example_sources,
                  (array_agg(b.linked_feature_id ORDER BY b.id))[1:20] example_features
           FROM house_numbering.buildings b JOIN extensions.spatial_features_v1 f
             ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
           WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active'
             AND f.attributes->>'tuyen_id'=ANY($3)
           GROUP BY code,road,alley,unassigned
         ) g ORDER BY code,road,alley,unassigned",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(codes)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

const STAGE_WIRE: &str =
    "jsonb_build_object('id',id,'route_code',route_code,'snapshot',snapshot,'revision',revision) AS wire";

/// Every staged geometry review of the project.
pub async fn geometry_stages(read: &mut ProjectRead<'_>) -> Result<Vec<Value>, AppError> {
    let sql = format!(
        "SELECT {STAGE_WIRE} FROM house_numbering.geometry_reviews
         WHERE organization_id=$1 AND project_id=$2"
    );
    let rows = sqlx::query(&sql)
        .bind(read.organization_id())
        .bind(read.project_id())
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    wires(&rows)
}

/// One staged review (`FOR UPDATE` when `lock`); 404 like `PlatformNotFoundError`.
pub async fn geometry_stage(
    read: &mut ProjectRead<'_>,
    review_id: Uuid,
    lock: bool,
) -> Result<Value, AppError> {
    let sql = format!(
        "SELECT {STAGE_WIRE} FROM house_numbering.geometry_reviews
         WHERE organization_id=$1 AND project_id=$2 AND id=$3{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    let row = sqlx::query(&sql)
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(review_id)
        .fetch_optional(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    one_wire(row)?.ok_or_else(|| AppError::not_found("Không tìm thấy tuyến chờ xác minh"))
}

/// Has the staged route already been imported into the catalog?
pub async fn stage_promoted(
    read: &mut ProjectRead<'_>,
    route_code: &str,
) -> Result<bool, AppError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM house_numbering.routes
           WHERE organization_id=$1 AND project_id=$2 AND source_label=$3)",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(format!("route-tree-import:{route_code}"))
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)
}

/// Parameters of the staged address report queries.
pub struct ReportArgs<'a> {
    pub review_id: Uuid,
    pub refs: &'a Value,
    pub codes: &'a [String],
    pub parent: Option<&'a str>,
    pub parent_revision: Option<i64>,
    pub parent_geometry: Option<&'a str>,
}

const REPORT_CTE: &str = "
  WITH links AS (SELECT * FROM jsonb_to_recordset(CAST($3 AS jsonb)) AS x(source_id uuid,feature_id text,revision text)),
  line AS (
   SELECT house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY x.source_id,x.feature_id)) AS g
   FROM links x JOIN extensions.spatial_features_v1 f ON f.organization_id=$1 AND f.project_id=$2
      AND f.source_id=x.source_id AND f.feature_id=x.feature_id AND f.content_revision=x.revision
   JOIN extensions.spatial_sources_v1 ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
   WHERE f.geometry_state='present' AND ds.lifecycle='active' AND ds.validation_state='ready'
   HAVING count(*)=(SELECT count(*) FROM links)
  ), valid AS (
   SELECT ST_Transform(line.g,4326)::geography g FROM line JOIN house_numbering.routes r ON r.organization_id=$1 AND r.project_id=$2 AND r.id=CAST($4 AS uuid)
   JOIN house_numbering.route_lines l ON (l.organization_id,l.project_id,l.route_id)=(r.organization_id,r.project_id,r.id)
   WHERE line.g IS NOT NULL AND r.revision=$5 AND l.content_revision=$6
     AND LEAST(ST_Distance(ST_Transform(ST_StartPoint(line.g),4326)::geography,ST_Transform(l.geometry,4326)::geography),
         ST_Distance(ST_Transform(ST_EndPoint(line.g),4326)::geography,ST_Transform(l.geometry,4326)::geography))<=8
  )
";

/// Does the staged line still resolve and meet its unchanged parent?
pub async fn report_current(
    read: &mut ProjectRead<'_>,
    args: &ReportArgs<'_>,
) -> Result<bool, AppError> {
    let sql = format!("{REPORT_CTE} SELECT EXISTS(SELECT 1 FROM valid)");
    sqlx::query_scalar(&sql)
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(args.refs)
        .bind(args.parent)
        .bind(args.parent_revision)
        .bind(args.parent_geometry)
        .fetch_one(&mut **read.transaction())
        .await
        .map_err(db_error)
}

/// Active buildings surveyed under the staged codes, with their decisions and the
/// point→line `distance` (null when the building geometry is unusable).
pub async fn report_rows(
    read: &mut ProjectRead<'_>,
    args: &ReportArgs<'_>,
) -> Result<Vec<Value>, AppError> {
    let sql = format!(
        "{REPORT_CTE}
      SELECT jsonb_build_object('id',b.id,'revision',b.revision,'route_id',b.route_id,'eligibility',b.eligibility,
        'source_id',b.linked_source_id,'feature_id',b.linked_feature_id,'content_revision',f.content_revision,
        'road',f.attributes->>'ten_duong','alley',f.attributes->>'hem','code',f.attributes->>'tuyen_id',
        'locked',EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
           AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('proposed','submitted','approved','reserved')),
        'evidence_token',a.evidence_token,'decision',a.decision,'verified_road',a.verified_road,
        'verified_prefix',a.verified_prefix,'evidence',a.evidence,'decision_revision',a.revision) AS wire,
        CASE WHEN f.geometry_state='present' AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
          AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND ds.lifecycle='active' AND ds.validation_state='ready'
          THEN ST_Distance(ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography,valid.g) END distance
      FROM house_numbering.buildings b JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
      JOIN extensions.spatial_sources_v1 ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
      CROSS JOIN valid
      LEFT JOIN house_numbering.geometry_address_reviews a ON (a.organization_id,a.project_id,a.building_id,a.geometry_review_id)=(b.organization_id,b.project_id,b.id,$7)
      WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active' AND f.attributes->>'tuyen_id'=ANY($8)
      ORDER BY b.id LIMIT 10001"
    );
    let rows = sqlx::query(&sql)
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(args.refs)
        .bind(args.parent)
        .bind(args.parent_revision)
        .bind(args.parent_geometry)
        .bind(args.review_id)
        .bind(args.codes)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            let mut value = row.try_get::<Value, _>("wire").map_err(db_error)?;
            let distance: Option<f64> = row.try_get("distance").map_err(db_error)?;
            value["distance"] = json!(distance);
            Ok(value)
        })
        .collect()
}

/// Upsert a staged snapshot; `None` when an identical snapshot and reason exist.
pub async fn stage_upsert(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    route_code: &str,
    snapshot: &str,
    token: &str,
    reason: &str,
) -> Result<Option<Value>, AppError> {
    let row = sqlx::query(
        "INSERT INTO house_numbering.geometry_reviews(id,organization_id,project_id,route_code,snapshot,snapshot_token,
              reason,created_by_actor_id,updated_by_actor_id)
         VALUES($3,$1,$2,$4,CAST($5 AS jsonb),$6,$7,$8,$8)
         ON CONFLICT(organization_id,project_id,route_code) DO UPDATE SET
              snapshot=EXCLUDED.snapshot,snapshot_token=EXCLUDED.snapshot_token,reason=EXCLUDED.reason,
              revision=geometry_reviews.revision+1,updated_at=now(),updated_by_actor_id=EXCLUDED.updated_by_actor_id
         WHERE geometry_reviews.snapshot_token<>EXCLUDED.snapshot_token OR geometry_reviews.reason<>EXCLUDED.reason
         RETURNING jsonb_build_object('id',id,'revision',revision) AS wire",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(Uuid::new_v4())
    .bind(route_code)
    .bind(snapshot)
    .bind(token)
    .bind(reason)
    .bind(actor_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    one_wire(row)
}

pub async fn stage_lookup(edit: &mut ProjectRead<'_>, route_code: &str) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT jsonb_build_object('id',id,'revision',revision) AS wire FROM house_numbering.geometry_reviews
         WHERE organization_id=$1 AND project_id=$2 AND route_code=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_code)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    row.try_get::<Value, _>("wire").map_err(db_error)
}

/// One verified address decision for a staged review.
pub struct AddressDecision<'a> {
    pub review_id: Uuid,
    pub building_id: Uuid,
    pub evidence_token: &'a str,
    pub decision: &'a str,
    pub road: &'a str,
    pub prefix: &'a str,
    pub evidence: &'a str,
    pub batch_id: Option<Uuid>,
}

/// `_write_review`: upsert the decision and audit it; buildings are never changed.
pub async fn write_address_review(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: &AddressDecision<'_>,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "INSERT INTO house_numbering.geometry_address_reviews(id,organization_id,project_id,geometry_review_id,building_id,evidence_token,
            decision,verified_road,verified_prefix,evidence,created_by_actor_id,updated_by_actor_id)
         VALUES($3,$1,$2,$4,$5,$6,$7,$8,$9,$10,$11,$11)
         ON CONFLICT(organization_id,project_id,geometry_review_id,building_id) DO UPDATE SET
            evidence_token=EXCLUDED.evidence_token,decision=EXCLUDED.decision,verified_road=EXCLUDED.verified_road,
            verified_prefix=EXCLUDED.verified_prefix,evidence=EXCLUDED.evidence,revision=geometry_address_reviews.revision+1,
            updated_at=now(),updated_by_actor_id=EXCLUDED.updated_by_actor_id
         RETURNING jsonb_build_object('id',id,'revision',revision) AS wire, id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(Uuid::new_v4())
    .bind(input.review_id)
    .bind(input.building_id)
    .bind(input.evidence_token)
    .bind(input.decision)
    .bind(input.road)
    .bind(input.prefix)
    .bind(input.evidence)
    .bind(actor.id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let saved = row.try_get::<Value, _>("wire").map_err(db_error)?;
    let id: Uuid = row.try_get("id").map_err(db_error)?;
    let mut metadata = json!({
        "geometry_review_id": input.review_id.to_string(),
        "building_id": input.building_id.to_string(),
        "decision": input.decision,
        "verified_road": input.road,
        "verified_prefix": input.prefix,
        "evidence_token": input.evidence_token,
        "evidence": input.evidence,
    });
    if let Some(batch) = input.batch_id {
        metadata["batch_id"] = json!(batch.to_string());
    }
    append_audit(
        edit,
        actor,
        "house_numbering.building.verify_address",
        "house_numbering_geometry_address_review",
        id,
        metadata,
    )
    .await?;
    Ok(json!({
        "id": id.to_string(),
        "revision": saved["revision"],
        "decision": input.decision,
        "building_updated": false,
    }))
}

/// Routes with segments and reviewed aliases for address suggestions, by id.
pub async fn suggestion_routes(read: &mut ProjectRead<'_>) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('id',id,'name',name,'route_type',route_type,'parent_route_id',parent_route_id,
                  'prefix',prefix,'source_label',source_label,'status',status,'revision',revision,'segments',segments,
                  'reviewed_aliases',reviewed_aliases,'linked_source_id',linked_source_id,
                  'linked_feature_id',linked_feature_id) AS wire
         FROM house_numbering.routes WHERE organization_id=$1 AND project_id=$2 ORDER BY id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Nearest catalog lines (≤ `radius` m, 8 per building) ∪ exact address pairs.
pub async fn suggestion_distances(
    read: &mut ProjectRead<'_>,
    ids: &[Uuid],
    route_ids: &[Uuid],
    radius: f64,
    pairs: &[(String, String)],
) -> Result<Vec<(String, String, f64)>, AppError> {
    let pairs: Vec<Value> = pairs
        .iter()
        .map(|(b, r)| json!({"building_id": b, "route_id": r}))
        .collect();
    let rows = sqlx::query(
        "WITH buildings AS MATERIALIZED (
           SELECT b.id,ST_Transform(ST_PointOnSurface(f.geometry),4326)::geography g
           FROM house_numbering.buildings b JOIN extensions.spatial_features_v1 f
             ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
           JOIN extensions.spatial_sources_v1 ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
           WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY($3)
             AND f.geometry_state='present' AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
             AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon') AND ds.lifecycle='active' AND ds.validation_state='ready'
         ), lines AS MATERIALIZED (
           SELECT l.route_id,ST_Transform(l.geometry,4326)::geography g
           FROM house_numbering.route_lines l WHERE l.organization_id=$1 AND l.project_id=$2
               AND l.route_id=ANY($4)
         ), near AS (
           SELECT b.id building_id,c.route_id,c.distance FROM buildings b CROSS JOIN LATERAL (
             SELECT l.route_id,ST_Distance(b.g,l.g) distance FROM lines l
             WHERE ST_DWithin(b.g,l.g,$5) ORDER BY distance,l.route_id LIMIT 8
           ) c
         ), exact AS (
           SELECT b.id building_id,l.route_id,ST_Distance(b.g,l.g) distance
           FROM jsonb_to_recordset(CAST($6 AS jsonb)) p(building_id uuid,route_id uuid)
           JOIN buildings b ON b.id=p.building_id JOIN lines l ON l.route_id=p.route_id
         ), merged AS (SELECT * FROM near UNION SELECT * FROM exact)
         SELECT building_id::text AS building_id,route_id::text AS route_id,distance FROM merged",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(ids)
    .bind(route_ids)
    .bind(radius)
    .bind(Value::Array(pairs))
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

/// Root street name above the staged route's parent (`verified_path`).
pub async fn verified_root(
    read: &mut ProjectRead<'_>,
    parent: Option<&str>,
) -> Result<Option<String>, AppError> {
    sqlx::query_scalar(
        "WITH RECURSIVE ancestors AS (
           SELECT id,name,route_type,parent_route_id,ARRAY[id] path FROM house_numbering.routes
           WHERE organization_id=$1 AND project_id=$2 AND id=CAST($3 AS uuid)
           UNION ALL
           SELECT r.id,r.name,r.route_type,r.parent_route_id,a.path||r.id FROM ancestors a
           JOIN house_numbering.routes r ON r.id=a.parent_route_id AND r.organization_id=$1 AND r.project_id=$2
           WHERE NOT r.id=ANY(a.path) AND cardinality(a.path)<100
         )
         SELECT name FROM ancestors WHERE route_type='street' AND parent_route_id IS NULL",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(parent)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)
}

/// Lock the listed buildings (and share-lock their features, and sources when
/// `with_sources`); returns how many exist.
pub async fn lock_review_buildings(
    edit: &mut ProjectRead<'_>,
    ids: &[Uuid],
    with_sources: bool,
) -> Result<usize, AppError> {
    // Lock domain rows first, then use the Core port for the linked features.
    // A missing link still fails the caller's original all-items count check.
    let rows = sqlx::query(
        "SELECT b.linked_source_id,b.linked_feature_id FROM house_numbering.buildings b
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY($3)
         ORDER BY b.id FOR UPDATE OF b",
    )
    .bind(edit.organization_id()).bind(edit.project_id()).bind(ids)
    .fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    let mut links = Vec::with_capacity(rows.len());
    for row in &rows {
        let source_id: Option<Uuid> = row.try_get("linked_source_id").map_err(db_error)?;
        let feature_id: Option<String> = row.try_get("linked_feature_id").map_err(db_error)?;
        if let (Some(source_id),Some(feature_id)) = (source_id,feature_id) {
            links.push(ovg_storage::module_ports::SpatialFeatureRef {source_id,feature_id});
        }
    }
    let locked = ovg_storage::module_ports::lock_linked_features(edit,&links,with_sources).await?;
    // Multiple domain rows may reference the same feature; preserve row-count semantics.
    let count = links.iter().filter(|link|locked.contains(*link)).count();
    Ok(count)
}

/// The exact route promoted from a staged snapshot (only when unique).
pub async fn promoted_route(
    read: &mut ProjectRead<'_>,
    route_code: &str,
    snapshot: &Value,
) -> Result<Option<Value>, AppError> {
    let text = |key: &str| snapshot[key].as_str().map(str::to_owned);
    let segments = match &snapshot["segments"] {
        Value::Array(items) if !items.is_empty() => Value::Array(items.clone()),
        _ => json!([]),
    };
    let rows = sqlx::query(
        "SELECT jsonb_build_object('id',r.id,'name',r.name,'route_type',r.route_type,'parent_route_id',r.parent_route_id,
                  'prefix',r.prefix,'status',r.status,'revision',r.revision,'geometry_revision',l.content_revision) AS wire
         FROM house_numbering.routes r JOIN house_numbering.route_lines l
           ON (l.organization_id,l.project_id,l.route_id)=(r.organization_id,r.project_id,r.id)
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.source_label=$3
           AND r.name=$4 AND r.route_type=$5 AND r.parent_route_id=CAST($6 AS uuid)
           AND r.prefix=$7 AND r.linked_source_id=CAST($8 AS uuid) AND r.linked_feature_id=$9
           AND r.direction_state=$10 AND COALESCE(r.segments,'[]'::jsonb)=CAST($11 AS jsonb)
         LIMIT 2",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(format!("route-tree-import:{route_code}"))
    .bind(text("name"))
    .bind(text("route_type"))
    .bind(text("parent_id"))
    .bind(text("resolved_prefix"))
    .bind(text("source_id"))
    .bind(text("feature_id"))
    .bind(text("direction_state"))
    .bind(segments)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if rows.len() != 1 {
        return Ok(None);
    }
    Ok(wires(&rows)?.pop())
}

/// Recorded address decisions of one staged review with their buildings' state.
pub async fn link_records(
    read: &mut ProjectRead<'_>,
    review_id: Uuid,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object('review_entry_id',a.id,'building_id',a.building_id,'review_revision',a.revision,
            'decision',a.decision,'verified_road',a.verified_road,'verified_prefix',a.verified_prefix,
            'evidence',a.evidence,'evidence_token',a.evidence_token,'building_revision',b.revision,
            'route_id',b.route_id,'state',b.state,'eligibility',b.eligibility,'source_id',b.linked_source_id,
            'feature_id',b.linked_feature_id,
            'locked',EXISTS(SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
              AND n.building_id=b.id AND n.retired_at IS NULL AND n.kind IN ('proposed','submitted','approved','reserved'))) AS wire
         FROM house_numbering.geometry_address_reviews a JOIN house_numbering.buildings b
           ON (b.organization_id,b.project_id,b.id)=(a.organization_id,a.project_id,a.building_id)
         WHERE a.organization_id=$1 AND a.project_id=$2 AND a.geometry_review_id=$3
         ORDER BY a.building_id LIMIT 10001",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(review_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Share-lock the promoted route, its parent and their ancestors.
pub async fn lineage_routes(
    edit: &mut ProjectRead<'_>,
    route_code: &str,
    parent: Option<&str>,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "WITH RECURSIVE lineage(id) AS (
           SELECT r.id FROM house_numbering.routes r WHERE r.organization_id=$1 AND r.project_id=$2
             AND (r.source_label=$3 OR r.id=CAST($4 AS uuid))
           UNION
           SELECT r.parent_route_id FROM house_numbering.routes r JOIN lineage a ON a.id=r.id
             WHERE r.organization_id=$1 AND r.project_id=$2 AND r.parent_route_id IS NOT NULL
         )
         SELECT jsonb_build_object('id',r.id,'linked_source_id',r.linked_source_id,
                  'linked_feature_id',r.linked_feature_id,'segments',r.segments) AS wire
         FROM house_numbering.routes r JOIN lineage t ON t.id=r.id
         WHERE r.organization_id=$1 AND r.project_id=$2 ORDER BY r.id FOR SHARE OF r",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(format!("route-tree-import:{route_code}"))
    .bind(parent)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    wires(&rows)
}

/// Share-lock the source features (and sources) behind a set of route links.
pub async fn lock_link_features(
    edit: &mut ProjectRead<'_>,
    links: Vec<Value>,
) -> Result<(), AppError> {
    let links: Vec<ovg_storage::module_ports::SpatialFeatureRef> =
        serde_json::from_value(Value::Array(links)).map_err(|_|AppError::validation("INVALID_FEATURE_REF","invalid linked feature reference"))?;
    ovg_storage::module_ports::lock_linked_features(edit,&links,true).await?;
    Ok(())
}

/// Link one verified building to its promoted route via `update_building`; returns
/// the new building revision.
pub async fn link_verified_building(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    expected_revision: i64,
    route_id: Uuid,
) -> Result<Value, AppError> {
    let current = get_building(edit, building_id, true).await?;
    let id = |key: &str| current[key].as_str().and_then(|v| Uuid::parse_str(v).ok());
    let result = update_building(
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
    Ok(result["revision"].clone())
}
