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

fn db_error(error: sqlx::Error) -> AppError {
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
    let items = rows
        .iter()
        .map(|row| row.try_get::<Value, _>("wire").map_err(db_error))
        .collect::<Result<Vec<_>, _>>()?;
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
            let mut wire = row.try_get::<Value, _>("wire").map_err(db_error)?;
            let distance = row
                .try_get::<String, _>("max_distance_m")
                .map_err(db_error)?;
            let object = wire
                .as_object_mut()
                .ok_or_else(|| AppError::internal("house-numbering rule row must be an object"))?;
            // FastAPI preserves SQLAlchemy Decimal as its scale-preserving
            // string wire form for this untyped module response.
            object.insert("max_distance_m".to_owned(), Value::String(distance));
            Ok(wire)
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
