//! E13 offline package repository over the canonical field.offline_packages table.
//!
//! Every operation runs inside an authorized ProjectRead transaction. The list
//! endpoint is also the online revalidation gate for writable offline packs.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "offline package database operation failed");
    AppError::internal("offline package database operation failed")
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn wire(row: &PgRow) -> Result<Value, AppError> {
    let revoked_at = row
        .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
        .map_err(db_error)?
        .map(iso);
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "name": row.try_get::<String, _>("name").map_err(db_error)?,
        "profile": row.try_get::<String, _>("profile").map_err(db_error)?,
        "aoi_bbox": row.try_get::<Value, _>("aoi_bbox").map_err(db_error)?,
        "crs_code": row.try_get::<i32, _>("crs_code").map_err(db_error)?,
        "layer_pins": row.try_get::<Value, _>("layer_pins").map_err(db_error)?,
        "feature_count": row.try_get::<i32, _>("feature_count").map_err(db_error)?,
        "sha256": row.try_get::<String, _>("sha256").map_err(db_error)?,
        "byte_size": row.try_get::<i64, _>("byte_size").map_err(db_error)?,
        "expires_at": iso(row.try_get::<DateTime<Utc>, _>("expires_at").map_err(db_error)?),
        "revoked_at": revoked_at,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn list_packages(edit: &mut ProjectRead<'_>, actor_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT p.* FROM field.offline_packages p
         WHERE p.organization_id = $1 AND p.project_id = $2
           AND p.created_by_actor_id = $3
           AND EXISTS (
             SELECT 1 FROM spatial.layers l
             JOIN spatial.data_sources s
               ON s.organization_id = l.organization_id
              AND s.project_id = l.project_id
              AND s.id = l.source_id
             WHERE l.organization_id = p.organization_id
               AND l.project_id = p.project_id
               AND l.id::text = p.layer_pins->0->>'layer_id'
               AND s.id::text = p.layer_pins->0->>'source_id'
               AND l.lifecycle = 'active'
               AND s.lifecycle = 'active'
               AND s.validation_state = 'ready'
           )
         ORDER BY p.created_at DESC
         LIMIT 50",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(actor_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn revoke_package(
    edit: &mut ProjectRead<'_>,
    package_id: Uuid,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "UPDATE field.offline_packages
         SET revoked_at = COALESCE(revoked_at, $4)
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(package_id)
    .bind(Utc::now())
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("offline package not found"))?;
    wire(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_time_format_matches_python_isoformat() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T01:02:03.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T01:02:03.123456+00:00");
    }
}
