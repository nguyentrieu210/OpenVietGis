//! E10 immutable 3D model/scene read-side repositories.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "3D catalog database operation failed");
    AppError::internal("3D catalog database operation failed")
}
fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}
fn model(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid,_>("id").map_err(db)?,
        "filename": row.try_get::<String,_>("filename").map_err(db)?,
        "format": row.try_get::<String,_>("format").map_err(db)?,
        "sha256": row.try_get::<String,_>("sha256").map_err(db)?,
        "byte_size": row.try_get::<i32,_>("byte_size").map_err(db)?,
        "version": row.try_get::<i32,_>("version").map_err(db)?,
        "profile": row.try_get::<Value,_>("profile").map_err(db)?,
        "created_at": iso(row.try_get::<DateTime<Utc>,_>("created_at").map_err(db)?),
    }))
}
fn scene(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid,_>("scene_id").map_err(db)?,
        "version": row.try_get::<i32,_>("version").map_err(db)?,
        "name": row.try_get::<String,_>("name").map_err(db)?,
        "profile": row.try_get::<Value,_>("profile").map_err(db)?,
        "created_at": iso(row.try_get::<DateTime<Utc>,_>("created_at").map_err(db)?),
    }))
}

pub async fn list_models(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT a.id,a.filename,a.format,a.sha256,a.byte_size,p.version,p.profile,p.created_at
         FROM spatial.model_assets a JOIN spatial.model_placements p
           ON p.organization_id=a.organization_id AND p.project_id=a.project_id AND p.model_id=a.id
         WHERE a.organization_id=$1 AND a.project_id=$2
           AND p.version=(SELECT max(x.version) FROM spatial.model_placements x
             WHERE x.organization_id=a.organization_id AND x.project_id=a.project_id
               AND x.model_id=a.id)
         ORDER BY p.created_at DESC,p.version DESC,a.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db)?;
    Ok(json!({"items":rows.iter().map(model).collect::<Result<Vec<_>,_>>()?}))
}
pub async fn model_history(
    read: &mut ProjectRead<'_>,
    id: Uuid,
) -> Result<Option<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT a.id,a.filename,a.format,a.sha256,a.byte_size,p.version,p.profile,p.created_at
         FROM spatial.model_assets a JOIN spatial.model_placements p
           ON p.organization_id=a.organization_id AND p.project_id=a.project_id AND p.model_id=a.id
         WHERE a.organization_id=$1 AND a.project_id=$2 AND a.id=$3
         ORDER BY p.created_at DESC,p.version DESC,a.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db)?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({"items":rows.iter().map(model).collect::<Result<Vec<_>,_>>()?}),
    ))
}
pub async fn list_scenes(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT scene_id,version,name,profile,created_at
         FROM spatial.scene_versions s
         WHERE organization_id=$1 AND project_id=$2
           AND version=(SELECT max(v.version) FROM spatial.scene_versions v
             WHERE v.organization_id=s.organization_id AND v.project_id=s.project_id
               AND v.scene_id=s.scene_id)
         ORDER BY created_at DESC,version DESC,scene_id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db)?;
    Ok(json!({"items":rows.iter().map(scene).collect::<Result<Vec<_>,_>>()?}))
}
pub async fn scene_history(
    read: &mut ProjectRead<'_>,
    id: Uuid,
) -> Result<Option<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT scene_id,version,name,profile,created_at
         FROM spatial.scene_versions
         WHERE organization_id=$1 AND project_id=$2 AND scene_id=$3
         ORDER BY created_at DESC,version DESC,scene_id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db)?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({"items":rows.iter().map(scene).collect::<Result<Vec<_>,_>>()?}),
    ))
}

#[derive(Debug, Clone)]
pub struct ModelBinary {
    pub filename: String,
    pub payload: Vec<u8>,
}

pub async fn model_binary(
    read: &mut ProjectRead<'_>,
    id: Uuid,
) -> Result<Option<ModelBinary>, AppError> {
    use sha2::{Digest, Sha256};
    let row = sqlx::query(
        "SELECT filename,sha256,payload FROM spatial.model_assets
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let filename = row.try_get::<String, _>("filename").map_err(db)?;
    let expected = row.try_get::<String, _>("sha256").map_err(db)?;
    let payload = row.try_get::<Vec<u8>, _>("payload").map_err(db)?;
    let actual = format!("{:x}", Sha256::digest(&payload));
    if actual != expected {
        return Err(AppError::conflict(
            "MODEL_INTEGRITY_FAILED",
            "Model integrity check failed",
        ));
    }
    Ok(Some(ModelBinary { filename, payload }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_matches_python_isoformat() {
        let t = DateTime::parse_from_rfc3339("2026-11-26T08:10:11.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(t), "2026-11-26T08:10:11.123456+00:00");
    }
}
