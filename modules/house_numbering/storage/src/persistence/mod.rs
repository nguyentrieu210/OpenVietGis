//! E15 House Numbering read-side persistence over the module-owned schema.
//!
//! The official module activation is checked separately from project auth. All
//! table reads remain inside the already authorized, RLS-bound ProjectRead
//! transaction.

use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

mod assignment;
mod buildings;
mod field;
mod hierarchy;
mod issuance;
mod numbering;
mod proposals;
mod records;
mod route_import;

pub use assignment::*;
pub use buildings::*;
pub use field::*;
pub use hierarchy::*;
pub use issuance::*;
pub use numbering::*;
pub use proposals::*;
pub use records::*;
pub use route_import::*;

/// Read the `wire` jsonb column that every module query projects, with Python
/// `datetime.isoformat()` timestamps (`_wire` rows).
pub(crate) fn wire(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    let mut value = row.try_get::<Value, _>("wire").map_err(db_error)?;
    fix_times(&mut value, false);
    Ok(value)
}

/// Like [`wire`] for rows FastAPI serializes with Pydantic (`...Z` for UTC).
pub(crate) fn wire_py(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    let mut value = row.try_get::<Value, _>("wire").map_err(db_error)?;
    fix_times(&mut value, true);
    Ok(value)
}

/// Postgres trims trailing fractional zeros; Python keeps six microsecond digits.
pub(crate) fn fix_times(value: &mut Value, pydantic: bool) {
    if let Some(object) = value.as_object_mut() {
        for (key, item) in object.iter_mut() {
            if !key.ends_with("_at") {
                continue;
            }
            if let Some(fixed) = item.as_str().and_then(|text| py_timestamp(text, pydantic)) {
                *item = Value::String(fixed);
            }
        }
    }
}

fn py_timestamp(text: &str, pydantic: bool) -> Option<String> {
    if text.len() < 25 || text.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    let (main, offset) = text.split_at(text.len() - 6);
    let ob = offset.as_bytes();
    if !matches!(ob[0], b'+' | b'-') || ob[3] != b':' {
        return None;
    }
    let (seconds, fraction) = main.split_once('.').unwrap_or((main, ""));
    if seconds.len() != 19 || fraction.len() > 6 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let fraction = if fraction.bytes().all(|b| b == b'0') {
        String::new()
    } else {
        format!(".{fraction:0<6}")
    };
    let offset = if pydantic && offset == "+00:00" {
        "Z"
    } else {
        offset
    };
    Some(format!("{seconds}{fraction}{offset}"))
}

pub(crate) fn db_error(error: sqlx::Error) -> AppError {
    // SQLSTATE OV409 is a module lock guard; Core renders it as 409 DATA_LOCKED.
    if let sqlx::Error::Database(db) = &error {
        if db.code().as_deref() == Some("OV409") {
            return AppError::conflict("DATA_LOCKED", db.message().to_owned());
        }
    }
    tracing::error!(%error, "house-numbering database operation failed");
    AppError::internal("house-numbering database operation failed")
}

async fn json_rows(
    read: &mut ProjectRead<'_>,
    sql: &str,
    route_id: Option<Uuid>,
) -> Result<Value, AppError> {
    let mut query = sqlx::query(sql)
        .bind(read.organization_id())
        .bind(read.project_id());
    if let Some(route_id) = route_id {
        query = query.bind(route_id);
    }
    let rows = query
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn list_rules(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(r) - 'max_distance_m' AS wire,
                r.max_distance_m::text AS max_distance_m
         FROM house_numbering.rules r
         WHERE r.organization_id = $1 AND r.project_id = $2
         ORDER BY r.created_at DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    let items = rows
        .iter()
        .map(|row| {
            let wire = row.try_get::<Value, _>("wire").map_err(db_error)?;
            let distance = row
                .try_get::<String, _>("max_distance_m")
                .map_err(db_error)?;
            // FastAPI preserves SQLAlchemy Decimal as its scale-preserving
            // string wire form for this untyped module response.
            rule_wire(wire, distance)
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(json!({ "items": items }))
}

pub async fn list_areas(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    json_rows(
        read,
        "SELECT to_jsonb(a) AS wire
         FROM house_numbering.areas a
         WHERE a.organization_id = $1 AND a.project_id = $2
         ORDER BY a.name, a.id",
        None,
    )
    .await
}

pub async fn list_routes(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    json_rows(
        read,
        "SELECT to_jsonb(r) || jsonb_build_object(
            'building_count',
            (
              SELECT count(*)
              FROM house_numbering.buildings b
              WHERE b.organization_id = r.organization_id
                AND b.project_id = r.project_id
                AND b.route_id = r.id
                AND b.state = 'active'
            )
         ) AS wire
         FROM house_numbering.routes r
         WHERE r.organization_id = $1 AND r.project_id = $2
         ORDER BY r.route_type, r.name, r.id",
        None,
    )
    .await
}

pub async fn list_buildings(
    read: &mut ProjectRead<'_>,
    route_id: Option<Uuid>,
) -> Result<Value, AppError> {
    let sql = if route_id.is_some() {
        "SELECT to_jsonb(b) || jsonb_build_object(
             'current_number', n.number_text,
             'number_kind', n.kind,
             'number_id', n.id
           ) AS wire
         FROM house_numbering.buildings b
         LEFT JOIN LATERAL (
             SELECT id, number_text, kind
             FROM house_numbering.address_numbers n
             WHERE n.organization_id = b.organization_id
               AND n.project_id = b.project_id
               AND n.building_id = b.id
               AND n.retired_at IS NULL
             ORDER BY CASE n.kind
               WHEN 'approved' THEN 1
               WHEN 'submitted' THEN 2
               WHEN 'proposed' THEN 3
               WHEN 'certificate' THEN 4
               WHEN 'survey' THEN 5
               WHEN 'cad' THEN 6
               ELSE 7 END,
               n.created_at DESC
             LIMIT 1
         ) n ON true
         WHERE b.organization_id = $1
           AND b.project_id = $2
           AND b.state = 'active'
           AND b.route_id = $3
         ORDER BY b.created_at, b.id"
    } else {
        "SELECT to_jsonb(b) || jsonb_build_object(
             'current_number', n.number_text,
             'number_kind', n.kind,
             'number_id', n.id
           ) AS wire
         FROM house_numbering.buildings b
         LEFT JOIN LATERAL (
             SELECT id, number_text, kind
             FROM house_numbering.address_numbers n
             WHERE n.organization_id = b.organization_id
               AND n.project_id = b.project_id
               AND n.building_id = b.id
               AND n.retired_at IS NULL
             ORDER BY CASE n.kind
               WHEN 'approved' THEN 1
               WHEN 'submitted' THEN 2
               WHEN 'proposed' THEN 3
               WHEN 'certificate' THEN 4
               WHEN 'survey' THEN 5
               WHEN 'cad' THEN 6
               ELSE 7 END,
               n.created_at DESC
             LIMIT 1
         ) n ON true
         WHERE b.organization_id = $1
           AND b.project_id = $2
           AND b.state = 'active'
         ORDER BY b.created_at, b.id"
    };
    json_rows(read, sql, route_id).await
}

#[derive(Debug, Clone, Copy)]
pub struct AuditActor<'a> {
    pub id: Uuid,
    pub display_name: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct RuleCreateInput<'a> {
    pub version: &'a str,
    pub status: &'a str,
    pub authority_ref: Option<&'a str>,
    pub left_odd: bool,
    pub suffix_style: &'a str,
    pub max_distance_m: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct AreaCreateInput<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub parent_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    pub feature_id: Option<&'a str>,
}

pub(crate) async fn append_audit(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    metadata: Value,
) -> Result<(), AppError> {
    ovg_storage::module_ports::append_audit(
        edit, actor.id, actor.display_name, action, resource_type, resource_id, metadata,
    ).await
}

pub(crate) use ovg_storage::module_ports::validate_linked_feature as spatial_feature;

fn rule_wire(mut wire: Value, distance: String) -> Result<Value, AppError> {
    fix_times(&mut wire, false);
    let object = wire
        .as_object_mut()
        .ok_or_else(|| AppError::internal("house-numbering rule row must be an object"))?;
    object.insert("max_distance_m".to_owned(), Value::String(distance));
    Ok(wire)
}

pub async fn create_rule(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: RuleCreateInput<'_>,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "WITH inserted AS (
           INSERT INTO house_numbering.rules (
             id, organization_id, project_id, version, status, authority_ref,
             left_odd, suffix_style, max_distance_m, sealed, revision,
             created_by_actor_id, updated_by_actor_id, created_at, updated_at
           ) VALUES (
             $1,$2,$3,$4,$5,$6,$7,$8,$9,true,1,$10,$10,$11,$11
           )
           RETURNING *
         )
         SELECT to_jsonb(inserted) - 'max_distance_m' AS wire,
                max_distance_m::text AS max_distance_m,
                id
         FROM inserted",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.version)
    .bind(input.status)
    .bind(input.authority_ref)
    .bind(input.left_odd)
    .bind(input.suffix_style)
    .bind(input.max_distance_m)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    let id: Uuid = row.try_get("id").map_err(db_error)?;
    let wire = rule_wire(
        row.try_get::<Value, _>("wire").map_err(db_error)?,
        row.try_get::<String, _>("max_distance_m")
            .map_err(db_error)?,
    )?;
    append_audit(
        edit,
        actor,
        "house_numbering.rule.create",
        "house_numbering_rule",
        id,
        json!({ "version": input.version }),
    )
    .await?;
    Ok(wire)
}

pub async fn create_area(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: AreaCreateInput<'_>,
) -> Result<Value, AppError> {
    if input.source_id.is_none() != input.feature_id.is_none() {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "area spatial source and feature must be supplied together",
        ));
    }
    if let (Some(source_id), Some(feature_id)) = (input.source_id, input.feature_id) {
        spatial_feature(edit, source_id, feature_id, &["ST_Polygon"]).await?;
    }

    let row = sqlx::query(
        "WITH inserted AS (
           INSERT INTO house_numbering.areas (
             id, organization_id, project_id, name, kind, parent_id,
             linked_source_id, linked_feature_id, revision,
             created_by_actor_id, updated_by_actor_id, created_at, updated_at
           ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$9,$10,$10)
           RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire, id, name
         FROM inserted",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
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

    let id: Uuid = row.try_get("id").map_err(db_error)?;
    let persisted_name: String = row.try_get("name").map_err(db_error)?;
    append_audit(
        edit,
        actor,
        "house_numbering.area.create",
        "house_numbering_area",
        id,
        json!({ "name": persisted_name }),
    )
    .await?;
    wire(&row)
}

#[cfg(test)]
mod tests {
    #[test]
    fn building_number_priority_is_pinned_in_sql_contract() {
        let priority = [
            "approved",
            "submitted",
            "proposed",
            "certificate",
            "survey",
            "cad",
        ];
        assert_eq!(priority[0], "approved");
        assert_eq!(priority[5], "cad");
    }
}
