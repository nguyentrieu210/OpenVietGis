//! E10 immutable 3D model/scene repository and scene lifecycle control.

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

#[derive(Debug, Clone)]
pub struct SceneModelPin {
    pub id: Uuid,
    pub version: i32,
}

#[derive(Debug, Clone)]
pub struct SceneDemRef {
    pub id: Uuid,
    pub vertical_reference: String,
    pub reference_label: String,
}

#[derive(Debug, Clone)]
pub struct SceneFootprintRef {
    pub layer_id: Uuid,
    pub height_field: String,
}

#[derive(Debug, Clone)]
pub struct SceneWrite {
    pub name: String,
    pub profile: Value,
    pub models: Vec<SceneModelPin>,
    pub dem: Option<SceneDemRef>,
    pub footprint: Option<SceneFootprintRef>,
}

#[derive(Debug)]
pub enum SceneStoreError {
    Database,
    NotFound,
    Changed,
    HistoryLimit,
    ProjectQuota,
    DemUnavailable,
    HeightFieldUnavailable,
    ModelUnavailable,
    ElevationMismatch,
}

fn scene_db(error: sqlx::Error) -> SceneStoreError {
    tracing::error!(%error, "scene database operation failed");
    SceneStoreError::Database
}

async fn resolve_scene(
    edit: &mut ProjectRead<'_>,
    input: &SceneWrite,
) -> Result<Vec<Value>, SceneStoreError> {
    if let Some(dem) = &input.dem {
        let metadata = sqlx::query_scalar::<_, Value>(
            "SELECT metadata FROM raster.layers
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(dem.id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(scene_db)?;
        let valid = metadata.is_some_and(|metadata| {
            metadata.get("method").and_then(Value::as_str) == Some("band")
                && metadata
                    .get("output_bands")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    == 1
        });
        if !valid {
            return Err(SceneStoreError::DemUnavailable);
        }
    }

    if let Some(footprint) = &input.footprint {
        let found = sqlx::query_scalar::<_, Uuid>(
            "SELECT l.id FROM spatial.layers l
             JOIN spatial.data_sources s
               ON s.organization_id=l.organization_id
              AND s.project_id=l.project_id
              AND s.id=l.source_id
             JOIN spatial.source_fields sf
               ON sf.organization_id=s.organization_id
              AND sf.project_id=s.project_id
              AND sf.source_id=s.id
              AND sf.schema_revision=s.schema_revision
             WHERE l.organization_id=$1 AND l.project_id=$2 AND l.id=$3
               AND l.lifecycle='active' AND s.lifecycle='active'
               AND s.validation_state='ready'
               AND s.geometry_family='Polygon'
               AND sf.field_id=$4
               AND sf.type IN ('integer','decimal')",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(footprint.layer_id)
        .bind(&footprint.height_field)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(scene_db)?;
        if found.is_none() {
            return Err(SceneStoreError::HeightFieldUnavailable);
        }
    }

    let mut models = Vec::with_capacity(input.models.len());
    for pin in &input.models {
        let row = sqlx::query(
            "SELECT a.id,a.filename,a.format,a.sha256,a.byte_size,p.version,p.profile,p.created_at
             FROM spatial.model_assets a JOIN spatial.model_placements p
               ON p.organization_id=a.organization_id
              AND p.project_id=a.project_id
              AND p.model_id=a.id
             WHERE a.organization_id=$1 AND a.project_id=$2
               AND a.id=$3 AND p.version=$4",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(pin.id)
        .bind(pin.version)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(scene_db)?
        .ok_or(SceneStoreError::ModelUnavailable)?;
        let profile = row.try_get::<Value, _>("profile").map_err(scene_db)?;
        let expected_dem = input.dem.as_ref().map(|dem| dem.id.to_string());
        let row_dem = profile
            .get("dem_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let elevation_ok = row_dem == expected_dem
            && input.dem.as_ref().is_none_or(|dem| {
                profile.get("vertical_reference").and_then(Value::as_str)
                    == Some(dem.vertical_reference.as_str())
                    && profile.get("reference_label").and_then(Value::as_str)
                        == Some(dem.reference_label.as_str())
            });
        if !elevation_ok {
            return Err(SceneStoreError::ElevationMismatch);
        }
        models.push(model(&row).map_err(|_| SceneStoreError::Database)?);
    }
    Ok(models)
}

async fn scene_audit(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    scene_id: Uuid,
    version: i32,
    input: &SceneWrite,
) -> Result<(), SceneStoreError> {
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id,organization_id,actor_type,actor_id,actor_display_snapshot,
            action,resource_type,resource_id,outcome,metadata,created_at
         ) VALUES ($1,$2,'human',$3,$4,'3d.scene.save','scene_3d',$5,
                   'success',$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(actor_id)
    .bind(actor_display)
    .bind(scene_id)
    .bind(json!({"version":version,"name":input.name,"profile":input.profile}))
    .bind(Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(scene_db)?;
    Ok(())
}

pub async fn save_scene(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    scene_id: Uuid,
    expected: i32,
    input: SceneWrite,
) -> Result<Value, SceneStoreError> {
    let key = format!("scene-vault:{}", edit.project_id());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(scene_db)?;

    let current = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT max(version) FROM spatial.scene_versions
         WHERE organization_id=$1 AND project_id=$2 AND scene_id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(scene_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(scene_db)?;

    if expected > 0 {
        let Some(current) = current else {
            return Err(SceneStoreError::NotFound);
        };
        if current != expected {
            return Err(SceneStoreError::Changed);
        }
        if expected >= 50 {
            return Err(SceneStoreError::HistoryLimit);
        }
    } else {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(DISTINCT scene_id) FROM spatial.scene_versions
             WHERE organization_id=$1 AND project_id=$2",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(scene_db)?;
        if count >= 100 {
            return Err(SceneStoreError::ProjectQuota);
        }
    }

    let _ = resolve_scene(edit, &input).await?;
    let version = expected + 1;
    let row = sqlx::query(
        "INSERT INTO spatial.scene_versions
         (organization_id,project_id,scene_id,version,name,profile,actor_id)
         VALUES($1,$2,$3,$4,$5,$6,$7)
         RETURNING scene_id,version,name,profile,created_at",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(scene_id)
    .bind(version)
    .bind(&input.name)
    .bind(&input.profile)
    .bind(actor_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(scene_db)?;
    scene_audit(edit, actor_id, actor_display, scene_id, version, &input).await?;
    scene(&row).map_err(|_| SceneStoreError::Database)
}

fn refs_from_profile(name: String, profile: &Value) -> Result<SceneWrite, SceneStoreError> {
    let models = profile
        .get("models")
        .and_then(Value::as_array)
        .ok_or(SceneStoreError::Database)?
        .iter()
        .map(|pin| {
            let id = pin
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())
                .ok_or(SceneStoreError::Database)?;
            let version = pin
                .get("version")
                .and_then(Value::as_i64)
                .and_then(|version| i32::try_from(version).ok())
                .ok_or(SceneStoreError::Database)?;
            Ok(SceneModelPin { id, version })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let dem = profile
        .get("dem")
        .filter(|value| !value.is_null())
        .map(|dem| {
            Ok(SceneDemRef {
                id: dem
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .ok_or(SceneStoreError::Database)?,
                vertical_reference: dem
                    .get("vertical_reference")
                    .and_then(Value::as_str)
                    .ok_or(SceneStoreError::Database)?
                    .to_owned(),
                reference_label: dem
                    .get("reference_label")
                    .and_then(Value::as_str)
                    .ok_or(SceneStoreError::Database)?
                    .to_owned(),
            })
        })
        .transpose()?;
    let footprint = profile
        .get("footprint")
        .filter(|value| !value.is_null())
        .map(|footprint| {
            Ok(SceneFootprintRef {
                layer_id: footprint
                    .get("layer_id")
                    .and_then(Value::as_str)
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .ok_or(SceneStoreError::Database)?,
                height_field: footprint
                    .get("height_field")
                    .and_then(Value::as_str)
                    .ok_or(SceneStoreError::Database)?
                    .to_owned(),
            })
        })
        .transpose()?;
    Ok(SceneWrite {
        name,
        profile: profile.clone(),
        models,
        dem,
        footprint,
    })
}

pub async fn open_scene(
    read: &mut ProjectRead<'_>,
    scene_id: Uuid,
    version: i32,
) -> Result<Option<Value>, SceneStoreError> {
    let row = sqlx::query(
        "SELECT scene_id,version,name,profile,created_at
         FROM spatial.scene_versions
         WHERE organization_id=$1 AND project_id=$2
           AND scene_id=$3 AND version=$4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(scene_id)
    .bind(version)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(scene_db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let name = row.try_get::<String, _>("name").map_err(scene_db)?;
    let profile = row.try_get::<Value, _>("profile").map_err(scene_db)?;
    let input = refs_from_profile(name, &profile)?;
    let models = resolve_scene(read, &input).await?;
    let scene = scene(&row).map_err(|_| SceneStoreError::Database)?;
    Ok(Some(json!({"scene":scene,"models":models})))
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
