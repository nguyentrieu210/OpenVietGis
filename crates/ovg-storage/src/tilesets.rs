//! E10 immutable 3D Tiles catalog + manifest read side.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db(e: sqlx::Error) -> AppError {
    tracing::error!(%e,"tileset database operation failed");
    AppError::internal("tileset database operation failed")
}
fn iso(v: DateTime<Utc>) -> String {
    v.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

pub async fn list(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id,name,byte_size,tile_count,created_at,provenance
         FROM spatial.tilesets
         WHERE organization_id=$1 AND project_id=$2
         ORDER BY created_at DESC,id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db)?;
    let items = rows
        .into_iter()
        .map(|r| {
            Ok(json!({
              "id":r.try_get::<Uuid,_>("id").map_err(db)?,
              "name":r.try_get::<String,_>("name").map_err(db)?,
              "byte_size":r.try_get::<i64,_>("byte_size").map_err(db)?,
              "tile_count":r.try_get::<i32,_>("tile_count").map_err(db)?,
              "created_at":iso(r.try_get::<DateTime<Utc>,_>("created_at").map_err(db)?),
              "provenance":r.try_get::<Value,_>("provenance").map_err(db)?,
            }))
        })
        .collect::<Result<Vec<Value>, AppError>>()?;
    Ok(json!({"items":items}))
}

pub async fn manifest(read: &mut ProjectRead<'_>, id: Uuid) -> Result<Option<Value>, AppError> {
    sqlx::query_scalar::<_, Value>(
        "SELECT manifest FROM spatial.tilesets
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_matches_python() {
        let t = DateTime::parse_from_rfc3339("2026-11-26T10:11:12.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(t), "2026-11-26T10:11:12.123456+00:00");
    }
}
