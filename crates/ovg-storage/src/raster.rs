//! E09 raster catalog read path over canonical RLS-protected raster tables.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{Map, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "raster catalog database operation failed");
    AppError::internal("raster catalog database operation failed")
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn wire(row: &PgRow) -> Result<Value, AppError> {
    let id = row.try_get::<Uuid, _>("id").map_err(db_error)?;
    let name = row.try_get::<String, _>("name").map_err(db_error)?;
    let metadata = row.try_get::<Value, _>("metadata").map_err(db_error)?;
    let byte_size = row.try_get::<i64, _>("byte_size").map_err(db_error)?;
    let created_at = row
        .try_get::<DateTime<Utc>, _>("created_at")
        .map_err(db_error)?;
    merge_wire(id, name, metadata, byte_size, created_at)
}

fn merge_wire(
    id: Uuid,
    name: String,
    metadata: Value,
    byte_size: i64,
    created_at: DateTime<Utc>,
) -> Result<Value, AppError> {
    let Value::Object(metadata) = metadata else {
        return Err(AppError::internal("raster metadata must be a JSON object"));
    };
    let mut out = Map::with_capacity(metadata.len() + 4);
    out.insert("id".to_owned(), Value::String(id.to_string()));
    out.insert("name".to_owned(), Value::String(name));
    // Match Python {id, name, **metadata, byte_size, created_at}: metadata may
    // replace id/name, while canonical byte_size/created_at always win.
    out.extend(metadata);
    out.insert("byte_size".to_owned(), Value::from(byte_size));
    out.insert("created_at".to_owned(), Value::String(iso(created_at)));
    Ok(Value::Object(out))
}

pub async fn list_layers(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id, name, metadata, byte_size, created_at
         FROM raster.layers
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC, id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::json!({ "items": items }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn metadata_merge_matches_python_dict_unpack_order() {
        let id = Uuid::new_v4();
        let created = DateTime::parse_from_rfc3339("2026-11-26T03:04:05.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let value = merge_wire(
            id,
            "DB name".to_owned(),
            json!({
                "id": "metadata-id",
                "name": "metadata-name",
                "byte_size": 7,
                "created_at": "metadata-created",
                "crs": 4326
            }),
            99,
            created,
        )
        .unwrap();
        assert_eq!(value["id"], "metadata-id");
        assert_eq!(value["name"], "metadata-name");
        assert_eq!(value["byte_size"], 99);
        assert_eq!(value["created_at"], "2026-11-26T03:04:05.123456+00:00");
        assert_eq!(value["crs"], 4326);
    }
}
