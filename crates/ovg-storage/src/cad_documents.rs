//! E10 CAD document read-side repository.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "CAD document database operation failed");
    AppError::internal("CAD document database operation failed")
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn wire(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "version": row.try_get::<i32, _>("version").map_err(db_error)?,
        "filename": row.try_get::<String, _>("filename").map_err(db_error)?,
        "format": row.try_get::<String, _>("format").map_err(db_error)?,
        "sha256": row.try_get::<String, _>("sha256").map_err(db_error)?,
        "byte_size": row.try_get::<i32, _>("byte_size").map_err(db_error)?,
        "id": row.try_get::<Uuid, _>("document_id").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn list_documents(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT DISTINCT ON(document_id)
            document_id, version, filename, format, sha256, byte_size, created_at
         FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY document_id, version DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn list_versions(
    read: &mut ProjectRead<'_>,
    document_id: Uuid,
) -> Result<Option<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT document_id, version, filename, format, sha256, byte_size, created_at
         FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2 AND document_id = $3
         ORDER BY version DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(document_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(Some(json!({ "items": items })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T05:06:07.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T05:06:07.123456+00:00");
    }
}
