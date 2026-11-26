//! Administrative hierarchy (areas) and route catalog writes.

use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use super::{append_audit, db_error, spatial_feature, wire, AuditActor};
use crate::catalog::ProjectRead;

#[derive(Debug, Clone, Copy)]
pub struct AreaUpdateInput<'a> {
    pub expected_revision: i64,
    pub name: &'a str,
    pub kind: &'a str,
    pub parent_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    pub feature_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct RouteSegmentInput<'a> {
    pub source_id: Uuid,
    pub feature_id: &'a str,
    pub code: &'a str,
}

#[derive(Debug, Clone)]
pub struct RouteCreateInput<'a> {
    pub name: &'a str,
    pub route_type: &'a str,
    pub parent_route_id: Option<Uuid>,
    pub prefix: Option<&'a str>,
    pub source_id: Uuid,
    pub feature_id: &'a str,
    pub direction_state: &'a str,
    pub source_label: &'a str,
    pub status: &'a str,
    pub segments: Vec<RouteSegmentInput<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct RouteUpdateInput<'a> {
    pub expected_revision: i64,
    pub name: &'a str,
    pub direction_state: &'a str,
    pub status: &'a str,
}

fn revision(row: &Value) -> i64 {
    row.get("revision").and_then(Value::as_i64).unwrap_or(0)
}

/// `SELECT * ... [FOR UPDATE]` for one area, as wire JSON; 404 "area not found".
pub(crate) async fn get_area(
    edit: &mut ProjectRead<'_>,
    area_id: Uuid,
    for_update: bool,
) -> Result<Value, AppError> {
    let sql = if for_update {
        "SELECT to_jsonb(a) AS wire FROM house_numbering.areas a
         WHERE a.organization_id=$1 AND a.project_id=$2 AND a.id=$3 FOR UPDATE"
    } else {
        "SELECT to_jsonb(a) AS wire FROM house_numbering.areas a
         WHERE a.organization_id=$1 AND a.project_id=$2 AND a.id=$3"
    };
    let row = sqlx::query(sql)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(area_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("area not found"))?;
    wire(&row)
}

/// `SELECT * ... [FOR UPDATE]` for one route, as wire JSON; 404 "route not found".
pub(crate) async fn get_route(
    edit: &mut ProjectRead<'_>,
    route_id: Uuid,
    for_update: bool,
) -> Result<Value, AppError> {
    let sql = if for_update {
        "SELECT to_jsonb(r) AS wire FROM house_numbering.routes r
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3 FOR UPDATE"
    } else {
        "SELECT to_jsonb(r) AS wire FROM house_numbering.routes r
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3"
    };
    let row = sqlx::query(sql)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(route_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("route not found"))?;
    wire(&row)
}

fn area_snapshot(current: &Value) -> Value {
    json!({
        "name": current["name"].clone(),
        "kind": current["kind"].clone(),
        "parent_id": current["parent_id"].clone(),
        "linked_source_id": current["linked_source_id"].clone(),
        "linked_feature_id": current["linked_feature_id"].clone(),
    })
}

pub async fn update_area(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    area_id: Uuid,
    input: AreaUpdateInput<'_>,
) -> Result<Value, AppError> {
    let current = get_area(edit, area_id, true).await?;
    if revision(&current) != input.expected_revision {
        return Err(AppError::conflict("STALE_AREA", "area revision is stale"));
    }
    if input.parent_id == Some(area_id) {
        return Err(AppError::validation(
            "AREA_PARENT",
            "area cannot be its own parent",
        ));
    }
    if input.source_id.is_none() != input.feature_id.is_none() {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "area spatial source and feature must be supplied together",
        ));
    }
    if let (Some(source_id), Some(feature_id)) = (input.source_id, input.feature_id) {
        spatial_feature(edit, source_id, feature_id, &["ST_Polygon"]).await?;
    }
    if let Some(parent_id) = input.parent_id {
        get_area(edit, parent_id, false).await?;
        let cycle = sqlx::query(
            "WITH RECURSIVE descendants AS (
                SELECT id FROM house_numbering.areas
                WHERE organization_id=$1 AND project_id=$2 AND parent_id=$3
                UNION ALL
                SELECT child.id
                FROM house_numbering.areas child
                JOIN descendants parent ON child.parent_id=parent.id
                WHERE child.organization_id=$1 AND child.project_id=$2
             )
             SELECT 1 AS hit FROM descendants WHERE id=$4 LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(area_id)
        .bind(parent_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if cycle.is_some() {
            return Err(AppError::conflict(
                "AREA_PARENT_CYCLE",
                "area parent would create a cycle",
            ));
        }
    }
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.areas
            SET name=$4,kind=$5,parent_id=$6,linked_source_id=$7,linked_feature_id=$8,
                revision=revision+1,updated_by_actor_id=$9,updated_at=$10
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(area_id)
    .bind(input.name.trim())
    .bind(input.kind)
    .bind(input.parent_id)
    .bind(input.source_id)
    .bind(input.feature_id)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.area.update",
        "house_numbering_area",
        area_id,
        json!({"before": area_snapshot(&current), "after_revision": revision(&result)}),
    )
    .await?;
    Ok(result)
}

pub async fn delete_area(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    area_id: Uuid,
    expected_revision: i64,
) -> Result<(), AppError> {
    let current = get_area(edit, area_id, true).await?;
    if revision(&current) != expected_revision {
        return Err(AppError::conflict("STALE_AREA", "area revision is stale"));
    }
    let dependency = sqlx::query(
        "SELECT kind FROM (
            SELECT 'child_area' AS kind FROM house_numbering.areas
            WHERE organization_id=$1 AND project_id=$2 AND parent_id=$3
            UNION ALL
            SELECT 'building_history' AS kind FROM house_numbering.buildings
            WHERE organization_id=$1 AND project_id=$2 AND area_id=$3
         ) dependencies LIMIT 1",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(area_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if let Some(row) = dependency {
        let kind: String = row.try_get("kind").map_err(db_error)?;
        return Err(AppError::conflict(
            "AREA_HAS_DEPENDENCIES",
            format!("area has dependent {kind}"),
        ));
    }
    sqlx::query(
        "DELETE FROM house_numbering.areas
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(area_id)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let mut snapshot = area_snapshot(&current);
    snapshot["revision"] = json!(revision(&current));
    append_audit(
        edit,
        actor,
        "house_numbering.area.delete",
        "house_numbering_area",
        area_id,
        json!({ "deleted_snapshot": snapshot }),
    )
    .await
}

/// Port of `HouseNumberingRepository.validate_route_segments`.
async fn validate_route_segments(
    edit: &mut ProjectRead<'_>,
    source_id: Uuid,
    feature_id: &str,
    segments: &[RouteSegmentInput<'_>],
) -> Result<Vec<Value>, AppError> {
    let grouped = !segments.is_empty();
    let links: Vec<Value> = if grouped {
        let first = &segments[0];
        if first.source_id != source_id || first.feature_id != feature_id {
            return Err(AppError::validation(
                "INVALID_ROUTE_SEGMENTS",
                "first segment must be the primary linked feature",
            ));
        }
        segments
            .iter()
            .map(|item| {
                json!({
                    "source_id": item.source_id.to_string(),
                    "feature_id": item.feature_id,
                    "code": item.code,
                })
            })
            .collect()
    } else {
        vec![json!({"source_id": source_id.to_string(), "feature_id": feature_id})]
    };
    let mut pairs = std::collections::HashSet::new();
    let mut code_set = std::collections::HashSet::new();
    let mut codes: Vec<String> = Vec::new();
    for item in &links {
        pairs.insert((
            item["source_id"].to_string(),
            item["feature_id"].to_string(),
        ));
        if let Some(code) = item.get("code").and_then(Value::as_str) {
            if !code.is_empty() {
                code_set.insert(code.to_owned());
                codes.push(code.to_owned());
            }
        }
    }
    if links.len() > 200
        || pairs.len() != links.len()
        || code_set.len() != codes.len()
        || codes.iter().any(|code| code.chars().count() > 160)
    {
        return Err(AppError::validation(
            "INVALID_ROUTE_SEGMENTS",
            "duplicate or excessive route segments",
        ));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("house-numbering-segments:{}", edit.project_id()))
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    let links_json = Value::Array(links.clone());
    let conflict = sqlx::query(
        "SELECT 1 AS hit FROM house_numbering.routes r
         CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_array_length(r.segments)>0 THEN r.segments
           ELSE jsonb_build_array(jsonb_build_object('source_id',r.linked_source_id,'feature_id',r.linked_feature_id)) END) e(v)
         JOIN jsonb_array_elements($3::jsonb) l(v)
           ON (e.v->>'source_id', e.v->>'feature_id')=(l.v->>'source_id',l.v->>'feature_id')
         WHERE r.organization_id=$1 AND r.project_id=$2
           AND (jsonb_array_length(r.segments)>0 OR $4)
         LIMIT 1",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(&links_json)
    .bind(grouped)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if conflict.is_some() {
        return Err(AppError::conflict(
            "ROUTE_SEGMENT_OCCUPIED",
            "segment is already linked to a route",
        ));
    }
    if !codes.is_empty() {
        let alias_conflict = sqlx::query(
            "SELECT 1 AS hit FROM house_numbering.routes r
             WHERE r.organization_id=$1 AND r.project_id=$2 AND (
               (r.source_label LIKE 'route-tree-import:%' AND substring(r.source_label from 19)=ANY($3))
               OR EXISTS (SELECT 1 FROM jsonb_array_elements(r.segments) e(v)
                          WHERE e.v->>'code'=ANY($3)))
             LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(&codes)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if alias_conflict.is_some() {
            return Err(AppError::conflict(
                "ROUTE_SEGMENT_CODE_OCCUPIED",
                "segment code is already linked to another route",
            ));
        }
    }
    let usable: Option<bool> = sqlx::query_scalar(
        "SELECT house_numbering.resolve_route_line(array_agg(f.geometry ORDER BY l.ord)) IS NOT NULL
         FROM jsonb_array_elements($3::jsonb) WITH ORDINALITY l(v,ord)
         LEFT JOIN spatial.reference_features f ON f.organization_id=$1 AND f.project_id=$2
           AND f.source_id=(l.v->>'source_id')::uuid AND f.feature_id=l.v->>'feature_id'
         LEFT JOIN spatial.data_sources s ON s.organization_id=$1 AND s.project_id=$2 AND s.id=f.source_id
         HAVING bool_and(COALESCE(f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready',false))",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(&links_json)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if usable != Some(true) {
        return Err(AppError::validation(
            "ROUTE_SEGMENTS_DISCONNECTED",
            "route segments must form one continuous non-branching line in one CRS",
        ));
    }
    Ok(if grouped { links } else { Vec::new() })
}

pub async fn create_route(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: RouteCreateInput<'_>,
) -> Result<Value, AppError> {
    spatial_feature(
        edit,
        input.source_id,
        input.feature_id,
        &["ST_LineString", "ST_MultiLineString"],
    )
    .await?;
    let links =
        validate_route_segments(edit, input.source_id, input.feature_id, &input.segments).await?;
    let segment_count = links.len().max(1);
    let row = sqlx::query(
        "WITH inserted AS (
            INSERT INTO house_numbering.routes (
                id,organization_id,project_id,name,route_type,parent_route_id,prefix,
                linked_source_id,linked_feature_id,direction_state,source_label,status,revision,segments,
                created_by_actor_id,updated_by_actor_id,created_at,updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,1,$13::jsonb,$14,$14,$15,$15)
            RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire, id FROM inserted",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.name.trim())
    .bind(input.route_type)
    .bind(input.parent_route_id)
    .bind(input.prefix)
    .bind(input.source_id)
    .bind(input.feature_id)
    .bind(input.direction_state)
    .bind(input.source_label.trim())
    .bind(input.status)
    .bind(Value::Array(links))
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let id: Uuid = row.try_get("id").map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.route.create",
        "house_numbering_route",
        id,
        json!({"name": result["name"].clone(), "segment_count": segment_count}),
    )
    .await?;
    Ok(result)
}

pub async fn update_route(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    route_id: Uuid,
    input: RouteUpdateInput<'_>,
) -> Result<Value, AppError> {
    let current = get_route(edit, route_id, true).await?;
    if revision(&current) != input.expected_revision {
        return Err(AppError::conflict("STALE_ROUTE", "route revision is stale"));
    }
    if current["direction_state"].as_str() != Some(input.direction_state) {
        let locked = sqlx::query(
            "SELECT 1 AS hit FROM house_numbering.address_numbers
             WHERE organization_id=$1 AND project_id=$2 AND route_id=$3
               AND retired_at IS NULL AND kind IN ('submitted','approved') LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(route_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if locked.is_some() {
            return Err(AppError::conflict(
                "ROUTE_DIRECTION_LOCKED",
                "route direction is locked by submitted or approved numbers",
            ));
        }
    }
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.routes
            SET name=$4,direction_state=$5,status=$6,
                revision=revision+1,updated_by_actor_id=$7,updated_at=$8
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .bind(input.name.trim())
    .bind(input.direction_state)
    .bind(input.status)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.route.update",
        "house_numbering_route",
        route_id,
        json!({"revision": revision(&result)}),
    )
    .await?;
    Ok(result)
}

pub async fn delete_route(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    route_id: Uuid,
) -> Result<(), AppError> {
    let dependency = sqlx::query(
        "SELECT kind FROM (
            SELECT 'child_route' AS kind FROM house_numbering.routes
             WHERE organization_id=$1 AND project_id=$2 AND parent_route_id=$3
            UNION ALL
            SELECT 'building' AS kind FROM house_numbering.buildings
             WHERE organization_id=$1 AND project_id=$2 AND route_id=$3
            UNION ALL
            SELECT 'number' AS kind FROM house_numbering.address_numbers
             WHERE organization_id=$1 AND project_id=$2 AND route_id=$3
         ) dependencies
         LIMIT 1",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if let Some(row) = dependency {
        let kind: String = row.try_get("kind").map_err(db_error)?;
        return Err(AppError::conflict(
            "ROUTE_HAS_DEPENDENCIES",
            format!("route has dependent {kind}"),
        ));
    }
    let deleted = sqlx::query(
        "DELETE FROM house_numbering.routes
         WHERE organization_id=$1 AND project_id=$2 AND id=$3
         RETURNING id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if deleted.is_none() {
        return Err(AppError::not_found("route not found"));
    }
    append_audit(
        edit,
        actor,
        "house_numbering.route.delete",
        "house_numbering_route",
        route_id,
        json!({}),
    )
    .await
}

/// Port of `HouseNumberingRepository.numbering_targets`.
pub async fn numbering_targets(
    read: &mut ProjectRead<'_>,
    route_id: Uuid,
    include_descendants: bool,
    limit: i64,
) -> Result<Value, AppError> {
    get_route(read, route_id, false).await?;
    let rows = sqlx::query(
        "WITH RECURSIVE tree(id) AS (
            SELECT CAST($3 AS uuid)
            UNION SELECT r.id FROM house_numbering.routes r JOIN tree t ON r.parent_route_id=t.id
            WHERE r.organization_id=$1 AND r.project_id=$2 AND $4
         )
         SELECT jsonb_build_object('id',b.id,'route_id',b.route_id,'parcel_ref',b.parcel_ref,
                                   'eligibility',b.eligibility) AS wire,
                count(*) OVER () AS total
         FROM house_numbering.buildings b JOIN tree t ON t.id=b.route_id
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active'
           AND b.eligibility!='ineligible'
           AND NOT EXISTS (
             SELECT 1 FROM house_numbering.address_numbers n WHERE n.organization_id=b.organization_id
               AND n.project_id=b.project_id AND n.building_id=b.id AND n.retired_at IS NULL
               AND n.kind IN ('proposed','submitted','approved','reserved')
           )
         ORDER BY b.route_id,b.id LIMIT $5",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(route_id)
    .bind(include_descendants)
    .bind(limit)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let total: i64 = match rows.first() {
        Some(row) => row.try_get("total").map_err(db_error)?,
        None => 0,
    };
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items": items, "total": total, "truncated": total > limit}))
}
