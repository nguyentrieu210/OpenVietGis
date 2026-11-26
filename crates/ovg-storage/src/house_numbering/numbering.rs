//! Route geometry snapshots, numbering previews and preview application.

use std::collections::HashMap;

use ovg_core::AppError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use super::buildings::get_building;
use super::hierarchy::get_route;
use super::{append_audit, db_error, wire, AuditActor};
use crate::catalog::ProjectRead;

/// Python `json.dumps(value, sort_keys=True)` (default separators, ASCII-escaped).
pub fn py_json_dumps(value: &Value) -> String {
    fn string(out: &mut String, text: &str) {
        out.push('"');
        for ch in text.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                ' '..='~' => out.push(ch),
                other => {
                    let mut units = [0_u16; 2];
                    for unit in other.encode_utf16(&mut units) {
                        out.push_str(&format!("\\u{unit:04x}"));
                    }
                }
            }
        }
        out.push('"');
    }
    fn walk(out: &mut String, value: &Value) {
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Value::Number(number) => out.push_str(&number.to_string()),
            Value::String(text) => string(out, text),
            Value::Array(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    walk(out, item);
                }
                out.push(']');
            }
            Value::Object(map) => {
                // serde_json maps iterate in sorted key order (sort_keys=True).
                out.push('{');
                for (index, (key, item)) in map.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    string(out, key);
                    out.push_str(": ");
                    walk(out, item);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    walk(&mut out, value);
    out
}

/// Port of `numbering_route_snapshot`: sha256 over the route's buildings and numbers.
pub async fn route_snapshot(
    edit: &mut ProjectRead<'_>,
    route_id: Uuid,
) -> Result<String, AppError> {
    let buildings: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',b.id,'revision',b.revision,'content_revision',f.content_revision,
                'geometry_state',f.geometry_state,'lifecycle',s.lifecycle,
                'validation_state',s.validation_state)
         FROM house_numbering.buildings b
         LEFT JOIN spatial.reference_features f
           ON (f.organization_id,f.project_id,f.source_id,f.feature_id)
            =(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
         LEFT JOIN spatial.data_sources s
           ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.route_id=$3 AND b.state='active'
         ORDER BY b.id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let numbers: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'building_id',building_id,'revision',revision,
                'number_text',number_text,'kind',kind,'side',side,'measure_m',measure_m::text)
         FROM house_numbering.address_numbers
         WHERE organization_id=$1 AND project_id=$2 AND route_id=$3 AND retired_at IS NULL
         ORDER BY id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let payload = json!([buildings, numbers]);
    Ok(format!(
        "{:x}",
        Sha256::digest(py_json_dumps(&payload).as_bytes())
    ))
}

/// Route line in its own CRS, oriented by `direction_state` (`route_geometry`).
#[derive(Debug, Clone)]
pub struct RouteGeometry {
    pub id: Uuid,
    pub prefix: Option<String>,
    pub revision: i64,
    pub content_revision: Value,
    pub srid: i32,
    pub geometry: Value,
}

pub async fn route_geometry(
    edit: &mut ProjectRead<'_>,
    route_id: Uuid,
) -> Result<RouteGeometry, AppError> {
    let row = sqlx::query(
        "SELECT r.id,r.prefix,r.revision,to_jsonb(f.content_revision) AS content_revision,
                ST_SRID(f.geometry) AS srid,
                ST_AsGeoJSON(
                    CASE WHEN r.direction_state='reverse' THEN ST_Reverse(f.geometry) ELSE f.geometry END
                )::jsonb AS geometry
         FROM house_numbering.routes r
         JOIN house_numbering.route_lines f
           ON f.organization_id=r.organization_id AND f.project_id=r.project_id AND f.route_id=r.id
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("route geometry not available"))?;
    let srid = row
        .try_get::<Option<i32>, _>("srid")
        .map_err(db_error)?
        .unwrap_or(0);
    let projected: Option<Option<bool>> = sqlx::query_scalar(
        "SELECT NOT (COALESCE(srtext,'') ILIKE 'GEOGCS%' OR COALESCE(proj4text,'') LIKE '%+proj=longlat%')
         FROM spatial_ref_sys WHERE srid=$1",
    )
    .bind(srid)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if projected != Some(Some(true)) {
        return Err(AppError::validation(
            "PROJECTED_CRS_REQUIRED",
            "numbering requires a verified projected route CRS",
        ));
    }
    Ok(RouteGeometry {
        id: row.try_get("id").map_err(db_error)?,
        prefix: row.try_get("prefix").map_err(db_error)?,
        revision: row.try_get("revision").map_err(db_error)?,
        content_revision: row
            .try_get::<Option<Value>, _>("content_revision")
            .map_err(db_error)?
            .unwrap_or(Value::Null),
        srid,
        geometry: row
            .try_get::<Option<Value>, _>("geometry")
            .map_err(db_error)?
            .unwrap_or(Value::Null),
    })
}

pub async fn transform_point(
    edit: &mut ProjectRead<'_>,
    x: f64,
    y: f64,
    source: i32,
    target: i32,
) -> Result<(f64, f64), AppError> {
    let row = sqlx::query(
        "SELECT ST_X(g) AS x,ST_Y(g) AS y FROM (
            SELECT ST_Transform(ST_SetSRID(ST_MakePoint($1,$2),$3),$4) g
         ) q",
    )
    .bind(x)
    .bind(y)
    .bind(source)
    .bind(target)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok((
        row.try_get("x").map_err(db_error)?,
        row.try_get("y").map_err(db_error)?,
    ))
}

/// One `building_points_for_route` row.
#[derive(Debug, Clone)]
pub struct BuildingPoint {
    pub id: Uuid,
    pub number_id: Option<Uuid>,
    pub number_text: Option<String>,
    pub kind: Option<String>,
    pub x: f64,
    pub y: f64,
}

pub async fn building_points(
    edit: &mut ProjectRead<'_>,
    route_id: Uuid,
    target_crs: i32,
) -> Result<Vec<BuildingPoint>, AppError> {
    let rows = sqlx::query(
        "SELECT b.id,n.id AS number_id,n.number_text,n.kind,
                ST_X(ST_Transform(ST_PointOnSurface(f.geometry),$4)) AS x,
                ST_Y(ST_Transform(ST_PointOnSurface(f.geometry),$4)) AS y
         FROM house_numbering.buildings b
         JOIN spatial.reference_features f
           ON f.organization_id=b.organization_id AND f.project_id=b.project_id
          AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
         JOIN spatial.data_sources s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
         LEFT JOIN LATERAL (
             SELECT * FROM house_numbering.address_numbers n
             WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
               AND n.building_id=b.id AND n.route_id=b.route_id AND n.retired_at IS NULL
             ORDER BY CASE n.kind
                 WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 WHEN 'reserved' THEN 3
                 WHEN 'proposed' THEN 4 WHEN 'certificate' THEN 5 WHEN 'survey' THEN 6
                 WHEN 'cad' THEN 7 ELSE 8 END, n.created_at DESC LIMIT 1
         ) n ON true
         WHERE b.organization_id=$1 AND b.project_id=$2
           AND b.route_id=$3 AND b.state='active' AND f.geometry_state='present'
           AND s.lifecycle='active' AND s.validation_state='ready'
           AND ST_SRID(f.geometry)>0 AND NOT ST_IsEmpty(f.geometry)
           AND ST_GeometryType(f.geometry) IN ('ST_Point','ST_Polygon','ST_MultiPolygon')",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .bind(target_crs)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| -> Result<BuildingPoint, AppError> {
            Ok(BuildingPoint {
                id: row.try_get("id").map_err(db_error)?,
                number_id: row.try_get("number_id").map_err(db_error)?,
                number_text: row.try_get("number_text").map_err(db_error)?,
                kind: row.try_get("kind").map_err(db_error)?,
                x: row.try_get("x").map_err(db_error)?,
                y: row.try_get("y").map_err(db_error)?,
            })
        })
        .collect()
}

/// One `list_route_numbers` row.
#[derive(Debug, Clone)]
pub struct RouteNumber {
    pub id: Uuid,
    pub building_id: Option<Uuid>,
    pub number_text: String,
    pub kind: String,
    pub side: Option<String>,
    pub measure_m: Option<f64>,
}

pub async fn route_numbers(
    edit: &mut ProjectRead<'_>,
    route_id: Uuid,
) -> Result<Vec<RouteNumber>, AppError> {
    let rows = sqlx::query(
        "SELECT id,building_id,number_text,kind,side,measure_m::float8 AS measure_m
         FROM house_numbering.address_numbers
         WHERE organization_id=$1 AND project_id=$2 AND route_id=$3 AND retired_at IS NULL
         ORDER BY measure_m NULLS LAST,created_at,id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(route_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| -> Result<RouteNumber, AppError> {
            Ok(RouteNumber {
                id: row.try_get("id").map_err(db_error)?,
                building_id: row.try_get("building_id").map_err(db_error)?,
                number_text: row.try_get("number_text").map_err(db_error)?,
                kind: row.try_get("kind").map_err(db_error)?,
                side: row.try_get("side").map_err(db_error)?,
                measure_m: row.try_get("measure_m").map_err(db_error)?,
            })
        })
        .collect()
}

/// One `numbering_buildings` row (database order is preserved like Python).
#[derive(Debug, Clone)]
pub struct NumberingTarget {
    pub id: Uuid,
    pub route_id: Option<Uuid>,
    pub revision: i64,
    pub state: String,
    pub eligibility: String,
    pub protected_number: bool,
}

pub async fn numbering_targets_for(
    edit: &mut ProjectRead<'_>,
    ids: &[Uuid],
) -> Result<Vec<NumberingTarget>, AppError> {
    let rows = sqlx::query(
        "SELECT b.id,b.route_id,b.revision,b.state,b.eligibility, EXISTS (
            SELECT 1 FROM house_numbering.address_numbers n
            WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
              AND n.building_id=b.id AND n.retired_at IS NULL
              AND n.kind IN ('submitted','approved','reserved','proposed')
         ) AS protected_number
         FROM house_numbering.buildings b
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY($3)",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(ids)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(|row| -> Result<NumberingTarget, AppError> {
            Ok(NumberingTarget {
                id: row.try_get("id").map_err(db_error)?,
                route_id: row.try_get("route_id").map_err(db_error)?,
                revision: row.try_get("revision").map_err(db_error)?,
                state: row.try_get("state").map_err(db_error)?,
                eligibility: row.try_get("eligibility").map_err(db_error)?,
                protected_number: row.try_get("protected_number").map_err(db_error)?,
            })
        })
        .collect()
}

/// Route parents and active occupants of the given full numbers, for tree checks.
pub async fn tree_occupancy(
    edit: &mut ProjectRead<'_>,
    numbers: &[String],
) -> Result<
    (
        HashMap<String, Option<String>>,
        Vec<(String, String, Option<String>)>,
    ),
    AppError,
> {
    let parents_rows = sqlx::query(
        "SELECT id,parent_route_id FROM house_numbering.routes
         WHERE organization_id=$1 AND project_id=$2",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let mut parents = HashMap::new();
    for row in &parents_rows {
        let id: Uuid = row.try_get("id").map_err(db_error)?;
        let parent: Option<Uuid> = row.try_get("parent_route_id").map_err(db_error)?;
        parents.insert(id.to_string(), parent.map(|value| value.to_string()));
    }
    let active = sqlx::query(
        "SELECT route_id,number_text,building_id FROM house_numbering.address_numbers
         WHERE organization_id=$1 AND project_id=$2 AND retired_at IS NULL
           AND number_text=ANY($3)",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(numbers)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let mut occupants = Vec::new();
    for row in &active {
        let route: Uuid = row.try_get("route_id").map_err(db_error)?;
        let building: Option<Uuid> = row.try_get("building_id").map_err(db_error)?;
        occupants.push((
            route.to_string(),
            row.try_get::<String, _>("number_text").map_err(db_error)?,
            building.map(|value| value.to_string()),
        ));
    }
    Ok((parents, occupants))
}

pub async fn route_name(edit: &mut ProjectRead<'_>, route_id: Uuid) -> Result<Value, AppError> {
    Ok(get_route(edit, route_id, false).await?["name"].clone())
}

pub struct PreviewInput<'a> {
    pub operation_id: Uuid,
    pub rule_id: Uuid,
    pub base_revisions: &'a Value,
    pub changes: &'a Value,
    pub warnings: &'a Value,
}

/// Port of `create_preview`; an existing `operation_id` returns the stored preview.
pub async fn create_preview(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: PreviewInput<'_>,
) -> Result<Value, AppError> {
    let existing = sqlx::query(
        "SELECT to_jsonb(p) AS wire FROM house_numbering.previews p
         WHERE p.organization_id=$1 AND p.project_id=$2 AND p.operation_id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.operation_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if let Some(row) = existing {
        return wire(&row);
    }
    let now = chrono::Utc::now();
    let row = sqlx::query(
        "WITH inserted AS (
            INSERT INTO house_numbering.previews (
                id,organization_id,project_id,operation_id,rule_id,base_revisions,
                candidate_changes,warnings,expires_at,created_by_actor_id,created_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
            RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire FROM inserted",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.operation_id)
    .bind(input.rule_id)
    .bind(input.base_revisions)
    .bind(input.changes)
    .bind(input.warnings)
    .bind(now + chrono::Duration::minutes(15))
    .bind(actor.id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    wire(&row)
}

fn number_wire(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    let mut value = wire(row)?;
    // Python keeps numeric(14,3) as a scale-preserving Decimal string.
    value["measure_m"] = row
        .try_get::<Option<String>, _>("measure_text")
        .map_err(db_error)?
        .map_or(Value::Null, Value::String);
    Ok(value)
}

fn stale(message: &str) -> AppError {
    AppError::conflict("STALE_PREVIEW", message.to_owned())
}

/// Port of `apply_preview`; `tree_conflict` is the module's street-tree collision rule
/// over `(route_id, number_text, building_id, is_proposal)` occupants.
pub async fn apply_preview(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    preview_id: Uuid,
    tree_conflict: &(dyn Fn(&HashMap<String, Option<String>>, &[(String, String, Option<String>, bool)]) -> bool
          + Sync),
) -> Result<Value, AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "house-numbering-number-apply:{}",
            edit.project_id()
        ))
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    let preview = sqlx::query(
        "SELECT rule_id, base_revisions, candidate_changes, applied_at IS NOT NULL AS applied,
                expires_at <= now() AS expired
         FROM house_numbering.previews
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(preview_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("numbering preview not found"))?;
    let changes: Value = preview.try_get("candidate_changes").map_err(db_error)?;
    let changes: Vec<Value> = changes.as_array().cloned().unwrap_or_default();
    let number_sql = "SELECT to_jsonb(n) AS wire, n.measure_m::text AS measure_text
         FROM house_numbering.address_numbers n
         WHERE n.organization_id=$1 AND n.project_id=$2 AND n.id = ANY($3)
         ORDER BY n.created_at,n.id";
    if preview.try_get::<bool, _>("applied").map_err(db_error)? {
        let ids: Vec<Uuid> = changes
            .iter()
            .filter_map(|change| change["result_number_id"].as_str())
            .filter_map(|id| Uuid::parse_str(id).ok())
            .collect();
        if ids.is_empty() {
            return Ok(json!({ "items": [] }));
        }
        let rows = sqlx::query(number_sql)
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(&ids)
            .fetch_all(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
        let items = rows
            .iter()
            .map(number_wire)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "items": items }));
    }
    if preview.try_get::<bool, _>("expired").map_err(db_error)? {
        return Err(AppError::conflict(
            "PREVIEW_EXPIRED",
            "numbering preview expired",
        ));
    }
    let rule_id: Uuid = preview.try_get("rule_id").map_err(db_error)?;
    let base: Value = preview.try_get("base_revisions").map_err(db_error)?;
    let text = |value: &Value| match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let buildings: std::collections::HashSet<String> =
        changes.iter().map(|c| text(&c["building_id"])).collect();
    let slots: std::collections::HashSet<(String, String)> = changes
        .iter()
        .map(|c| (text(&c["route_id"]), text(&c["number"])))
        .collect();
    if buildings.len() != changes.len() || slots.len() != changes.len() {
        return Err(AppError::conflict(
            "NUMBER_OCCUPIED",
            "duplicate in numbering preview",
        ));
    }
    let mut route_ids: Vec<Uuid> = changes
        .iter()
        .map(|c| {
            Uuid::parse_str(&text(&c["route_id"])).map_err(|e| AppError::internal(e.to_string()))
        })
        .collect::<Result<_, _>>()?;
    route_ids.sort();
    route_ids.dedup();
    for route_id in &route_ids {
        let route = get_route(edit, *route_id, true).await?;
        if route["revision"].as_i64() != base[format!("route:{route_id}")].as_i64() {
            return Err(stale("route changed since preview"));
        }
        let current = match route_geometry(edit, *route_id).await {
            Ok(geometry) => geometry.content_revision,
            Err(AppError::NotFound { .. } | AppError::Validation { .. }) => {
                return Err(stale("route geometry no longer usable"))
            }
            Err(other) => return Err(other),
        };
        if let Some(expected) = base.get(format!("route_geometry:{route_id}")) {
            if &current != expected {
                return Err(stale("route geometry changed since preview"));
            }
        }
        if let Some(expected) = base.get(format!("route_snapshot:{route_id}")) {
            if Some(route_snapshot(edit, *route_id).await?.as_str()) != expected.as_str() {
                return Err(stale(
                    "route addresses or building geometry changed since preview",
                ));
            }
        }
    }
    if !changes.is_empty() {
        let mut numbers: Vec<String> = changes.iter().map(|c| text(&c["number"])).collect();
        numbers.sort();
        numbers.dedup();
        let (parents, existing) = tree_occupancy(edit, &numbers).await?;
        let mut occupants: Vec<(String, String, Option<String>, bool)> = existing
            .into_iter()
            .map(|(route, number, building)| (route, number, building, false))
            .collect();
        occupants.extend(changes.iter().map(|c| {
            (
                text(&c["route_id"]),
                text(&c["number"]),
                Some(text(&c["building_id"])),
                true,
            )
        }));
        if tree_conflict(&parents, &occupants) {
            return Err(AppError::conflict(
                "CROSS_ROUTE_NUMBER_OCCUPIED",
                "full number already exists on another route in the same street tree",
            ));
        }
    }
    let mut applied_changes = Vec::new();
    let mut result_ids = Vec::new();
    for mut change in changes {
        let building_id = Uuid::parse_str(&text(&change["building_id"]))
            .map_err(|e| AppError::internal(e.to_string()))?;
        let route_id = Uuid::parse_str(&text(&change["route_id"]))
            .map_err(|e| AppError::internal(e.to_string()))?;
        let building = get_building(edit, building_id, true).await?;
        if building["revision"].as_i64() != base[format!("building:{building_id}")].as_i64()
            || building["route_id"].as_str() != Some(route_id.to_string().as_str())
            || building["state"].as_str() != Some("active")
        {
            return Err(stale("building changed since preview"));
        }
        if building["eligibility"].as_str() == Some("ineligible") {
            return Err(AppError::conflict(
                "INELIGIBLE_BUILDING",
                "ineligible building cannot receive a number",
            ));
        }
        let number = text(&change["number"]);
        let occupied = sqlx::query(
            "SELECT 1 AS hit FROM house_numbering.address_numbers
             WHERE organization_id=$1 AND project_id=$2 AND retired_at IS NULL AND (
                 (building_id=$3 AND kind IN ('proposed','submitted','approved','reserved'))
                 OR (route_id=$4 AND number_text=$5 AND building_id IS DISTINCT FROM $3)
             ) LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(building_id)
        .bind(route_id)
        .bind(&number)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if occupied.is_some() {
            return Err(AppError::conflict(
                "NUMBER_OCCUPIED",
                "number became occupied or building already numbered",
            ));
        }
        let now = chrono::Utc::now();
        sqlx::query(
            "UPDATE house_numbering.address_numbers SET retired_at=$4,
                 revision=revision+1,updated_by_actor_id=$5,updated_at=$4
             WHERE organization_id=$1 AND project_id=$2 AND building_id=$3
               AND retired_at IS NULL AND kind='proposed'",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(building_id)
        .bind(now)
        .bind(actor.id)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        let number_id = Uuid::new_v4();
        let side = change["side"].as_str().map(str::to_owned);
        let measure = change["measure"].as_f64();
        sqlx::query(
            "INSERT INTO house_numbering.address_numbers (
                id,organization_id,project_id,building_id,route_id,number_text,kind,
                side,measure_m,rule_id,revision,created_by_actor_id,updated_by_actor_id,
                created_at,updated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,'proposed',$7,$8,$9,1,$10,$10,$11,$11)",
        )
        .bind(number_id)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(building_id)
        .bind(route_id)
        .bind(&number)
        .bind(side)
        .bind(measure)
        .bind(rule_id)
        .bind(actor.id)
        .bind(now)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        change["result_number_id"] = json!(number_id.to_string());
        result_ids.push(number_id);
        applied_changes.push(change);
    }
    sqlx::query(
        "UPDATE house_numbering.previews SET candidate_changes=$4,applied_at=$5
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(preview_id)
    .bind(Value::Array(applied_changes))
    .bind(chrono::Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let rows = sqlx::query(number_sql)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(&result_ids)
        .fetch_all(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    // RETURNING order is insertion order; created_at,id sorts identically per batch
    // except for equal timestamps, so keep the explicit order of insertion.
    let mut by_id: HashMap<String, Value> = HashMap::new();
    for row in &rows {
        let value = number_wire(row)?;
        by_id.insert(text(&value["id"]), value);
    }
    let items: Vec<Value> = result_ids
        .iter()
        .filter_map(|id| by_id.remove(&id.to_string()))
        .collect();
    append_audit(
        edit,
        actor,
        "house_numbering.preview.apply",
        "house_numbering_preview",
        preview_id,
        json!({ "count": items.len() }),
    )
    .await?;
    Ok(json!({ "items": items }))
}

/// Port of `inspection_context` (building on the route plus its evidence).
pub async fn inspection_context(
    edit: &mut ProjectRead<'_>,
    building_id: Option<Uuid>,
    route_id: Uuid,
) -> Result<Value, AppError> {
    let Some(building_id) = building_id else {
        return Ok(json!({"building": null, "evidence": []}));
    };
    let building = sqlx::query(
        "SELECT jsonb_build_object('id',b.id,'parcel_ref',b.parcel_ref,
                'linked_source_id',b.linked_source_id,'linked_feature_id',b.linked_feature_id,
                'core_feature_attributes',f.attributes) AS wire
         FROM house_numbering.buildings b
         LEFT JOIN spatial.reference_features f
           ON f.organization_id=b.organization_id AND f.project_id=b.project_id
          AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=$3 AND b.route_id=$4",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(route_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("building is not on the inspected route"))?;
    let evidence = sqlx::query(
        "SELECT jsonb_build_object('id',id,'origin',origin,'raw_number',raw_number,
                'source_reference',source_reference,'evidence_date',evidence_date,
                'confidence',confidence,'created_at',created_at) AS wire
         FROM house_numbering.evidence
         WHERE organization_id=$1 AND project_id=$2
           AND (building_id=$3 OR (building_id IS NULL AND route_id=$4))
         ORDER BY evidence_date NULLS LAST,created_at,id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(route_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let evidence = evidence.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"building": wire(&building)?, "evidence": evidence}))
}
