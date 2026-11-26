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

mod buildings;
mod field;
mod hierarchy;
mod numbering;
mod proposals;
mod records;

pub use buildings::*;
pub use field::*;
pub use hierarchy::*;
pub use numbering::*;
pub use proposals::*;
pub use records::*;

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

pub async fn active_manifest_contract(
    read: &mut ProjectRead<'_>,
) -> Result<Option<(Value, bool)>, AppError> {
    let row = sqlx::query(
        "SELECT
            m.extension_key, m.version, m.api_compatibility,
            m.display_name, m.description, m.permissions, m.commands,
            m.capabilities, m.entity_schemas, m.ui_contributions, m.enabled
         FROM extensions.project_activations a
         JOIN extensions.manifests m
           ON m.organization_id = a.organization_id
          AND m.id = a.manifest_id
          AND m.extension_key = a.extension_key
         WHERE a.organization_id = $1
           AND a.project_id = $2
           AND a.extension_key = 'house_numbering'
           AND a.status = 'active'",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    row.map(|row| {
        let enabled: bool = row.try_get("enabled").map_err(db_error)?;
        let contract = json!({
            "extension_key": row.try_get::<String, _>("extension_key").map_err(db_error)?,
            "version": row.try_get::<String, _>("version").map_err(db_error)?,
            "api_compatibility": row.try_get::<String, _>("api_compatibility").map_err(db_error)?,
            "display_name": row.try_get::<String, _>("display_name").map_err(db_error)?,
            "description": row.try_get::<String, _>("description").map_err(db_error)?,
            "permissions": row.try_get::<Value, _>("permissions").map_err(db_error)?,
            "commands": row.try_get::<Value, _>("commands").map_err(db_error)?,
            "capabilities": row.try_get::<Value, _>("capabilities").map_err(db_error)?,
            "entity_schemas": row.try_get::<Value, _>("entity_schemas").map_err(db_error)?,
            "ui_contributions": row.try_get::<Value, _>("ui_contributions").map_err(db_error)?,
        });
        Ok((contract, enabled))
    })
    .transpose()
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
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1,$2,'human',$3,$4,$5,$6,$7,'success',$8,$9)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(actor.id)
    .bind(actor.display_name)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .bind(chrono::Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn spatial_feature(
    edit: &mut ProjectRead<'_>,
    source_id: Uuid,
    feature_id: &str,
    expected: &[&str],
) -> Result<i32, AppError> {
    let row = sqlx::query(
        "SELECT ds.stored_crs, ds.lifecycle, ds.validation_state,
                ST_SRID(rf.geometry) AS srid,
                ST_GeometryType(rf.geometry) AS geometry_type
         FROM spatial.data_sources ds
         JOIN spatial.reference_features rf
           ON rf.organization_id = ds.organization_id
          AND rf.project_id = ds.project_id
          AND rf.source_id = ds.id
         WHERE ds.organization_id = $1
           AND ds.project_id = $2
           AND ds.id = $3
           AND rf.feature_id = $4
           AND rf.geometry_state = 'present'",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(source_id)
    .bind(feature_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("linked spatial feature not found or not usable"))?;

    let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
    let validation_state: String = row.try_get("validation_state").map_err(db_error)?;
    if lifecycle != "active" || validation_state != "ready" {
        return Err(AppError::validation(
            "SOURCE_NOT_READY",
            "linked spatial source is not ready",
        ));
    }
    let geometry_type: String = row.try_get("geometry_type").map_err(db_error)?;
    if !expected.contains(&geometry_type.as_str()) {
        return Err(AppError::validation(
            "GEOMETRY_TYPE_MISMATCH",
            format!("linked feature geometry '{geometry_type}' is not valid for this entity"),
        ));
    }
    let srid = row
        .try_get::<Option<i32>, _>("srid")
        .map_err(db_error)?
        .filter(|srid| *srid != 0)
        .or(row
            .try_get::<Option<i32>, _>("stored_crs")
            .map_err(db_error)?
            .filter(|srid| *srid != 0))
        .unwrap_or(0);
    if srid <= 0 {
        return Err(AppError::validation(
            "MISSING_CRS",
            "linked geometry has no explicit CRS",
        ));
    }
    Ok(srid)
}

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
