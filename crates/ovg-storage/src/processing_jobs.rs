//! E08 durable processing job repository over the canonical processing.jobs table.
//!
//! Callers must open ProjectRead with the correct read/edit policy first;
//! this repository keeps every statement inside that already authorized,
//! RLS-bound transaction.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "processing job database operation failed");
    AppError::internal("processing job database operation failed")
}

fn not_found() -> AppError {
    AppError::not_found("processing job not found")
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn row_wire(row: &PgRow) -> Result<Value, AppError> {
    let completed_at = row
        .try_get::<Option<DateTime<Utc>>, _>("completed_at")
        .map_err(db_error)?
        .map(iso);
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "organization_id": row.try_get::<Uuid, _>("organization_id").map_err(db_error)?,
        "project_id": row.try_get::<Uuid, _>("project_id").map_err(db_error)?,
        "algorithm_id": row.try_get::<String, _>("algorithm_id").map_err(db_error)?,
        "operation_id": row.try_get::<String, _>("operation_id").map_err(db_error)?,
        "status": row.try_get::<String, _>("status").map_err(db_error)?,
        "actor_type": row.try_get::<String, _>("actor_type").map_err(db_error)?,
        "actor_id": row.try_get::<Uuid, _>("actor_id").map_err(db_error)?,
        "input_refs": row.try_get::<Value, _>("input_refs").map_err(db_error)?,
        "parameters": row.try_get::<Value, _>("parameters").map_err(db_error)?,
        "output_name": row.try_get::<String, _>("output_name").map_err(db_error)?,
        "output_crs": row.try_get::<i32, _>("output_crs").map_err(db_error)?,
        "engine_version": row.try_get::<String, _>("engine_version").map_err(db_error)?,
        "attempt_count": row.try_get::<i32, _>("attempt_count").map_err(db_error)?,
        "output_source_id": row.try_get::<Option<Uuid>, _>("output_source_id").map_err(db_error)?,
        "output_layer_id": row.try_get::<Option<Uuid>, _>("output_layer_id").map_err(db_error)?,
        "output_content_revision": row.try_get::<Option<String>, _>("output_content_revision").map_err(db_error)?,
        "feature_count": row.try_get::<Option<i32>, _>("feature_count").map_err(db_error)?,
        "error_code": row.try_get::<Option<String>, _>("error_code").map_err(db_error)?,
        "error_message": row.try_get::<Option<String>, _>("error_message").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
        "completed_at": completed_at,
    }))
}

pub async fn list_jobs(read: &mut ProjectRead<'_>, limit: i64) -> Result<Value, AppError> {
    if !(1..=200).contains(&limit) {
        return Err(AppError::validation(
            "INVALID_REQUEST",
            "limit must be between 1 and 200",
        ));
    }
    let rows = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(limit)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(row_wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn get_job(read: &mut ProjectRead<'_>, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(job_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(not_found)?;
    row_wire(&row)
}

pub async fn cancel_job(write: &mut ProjectRead<'_>, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT status FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         FOR UPDATE",
    )
    .bind(write.organization_id())
    .bind(write.project_id())
    .bind(job_id)
    .fetch_optional(&mut **write.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(not_found)?;
    let status: String = row.try_get("status").map_err(db_error)?;
    validate_cancel_status(&status)?;

    let row = sqlx::query(
        "UPDATE processing.jobs
         SET status = 'cancelled', updated_at = $4, completed_at = $4
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(write.organization_id())
    .bind(write.project_id())
    .bind(job_id)
    .bind(Utc::now())
    .fetch_one(&mut **write.transaction())
    .await
    .map_err(db_error)?;
    row_wire(&row)
}

fn validate_cancel_status(status: &str) -> Result<(), AppError> {
    if matches!(status, "succeeded" | "cancelled") {
        return Err(AppError::conflict(
            "JOB_TERMINAL",
            format!("cannot cancel job in status {status}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_semantics_match_python_repository() {
        for status in ["queued", "running", "failed"] {
            assert!(validate_cancel_status(status).is_ok(), "{status}");
        }
        for status in ["succeeded", "cancelled"] {
            let error = validate_cancel_status(status).unwrap_err();
            assert_eq!(error.code(), "JOB_TERMINAL");
            assert_eq!(
                error.to_string(),
                format!("cannot cancel job in status {status}")
            );
        }
    }

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T01:02:03.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T01:02:03.123456+00:00");
    }
}
