//! Shared service ports for trusted module persistence.
//! All operations use the already-authorized Core project transaction.

use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

fn db_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &error {
        if db.code().as_deref() == Some("OV409") {
            return AppError::conflict("DATA_LOCKED", db.message().to_owned());
        }
    }
    tracing::error!(%error, "module spatial service database operation failed");
    AppError::internal("module spatial service database operation failed")
}

/// Append an audit entry using the authenticated scope, never a caller-provided
/// actor type or identity. The event commits atomically with the module write.
pub async fn append_audit(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    display_name: &str,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    metadata: Value,
) -> Result<(), AppError> {
    if actor_id != edit.actor_id() || edit.is_gis_scoped() {
        return Err(AppError::forbidden("module audit actor or project scope mismatch"));
    }
    crate::extensions::append_project_audit(
        edit, actor_id, display_name, action, resource_type, resource_id, metadata, chrono::Utc::now(),
    ).await
}

/// Validate a module's reference to a ready Core GIS feature without exposing
/// source table layout or duplicating geometry/CRS validation in the module.
pub async fn validate_linked_feature(
    edit: &mut ProjectRead<'_>,
    source_id: Uuid,
    feature_id: &str,
    expected: &[&str],
) -> Result<i32, AppError> {
    if edit.is_gis_scoped() {
        return Err(AppError::forbidden("unrestricted project scope required for module feature links"));
    }
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

/// Stable source identity and feature identity; layer display names are not keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpatialFeatureRef {
    pub source_id: Uuid,
    pub feature_id: String,
}

/// Pin existing feature rows (and optionally their sources) for the caller's
/// transaction. No module-specific business row or write is handled here.
/// Missing references are returned by omission, allowing caller-specific policy.
pub async fn lock_linked_features(
    edit: &mut ProjectRead<'_>,
    links: &[SpatialFeatureRef],
    with_sources: bool,
) -> Result<std::collections::HashSet<SpatialFeatureRef>, AppError> {
    if edit.is_gis_scoped() {
        return Err(AppError::forbidden("unrestricted project scope required for module feature locks"));
    }
    if links.len() > 100_000 || links.iter().any(|l|l.feature_id.is_empty()||l.feature_id.len()>128) {
        return Err(AppError::validation("INVALID_FEATURE_REF","invalid or oversized feature reference batch"));
    }
    if links.is_empty(){return Ok(std::collections::HashSet::new());}
    let sql = if with_sources {
        "SELECT f.source_id,f.feature_id FROM jsonb_to_recordset($3::jsonb) AS x(source_id uuid,feature_id text)
         JOIN spatial.reference_features f ON f.organization_id=$1 AND f.project_id=$2 AND f.source_id=x.source_id AND f.feature_id=x.feature_id
         JOIN spatial.data_sources ds ON (ds.organization_id,ds.project_id,ds.id)=(f.organization_id,f.project_id,f.source_id)
         ORDER BY f.source_id,f.feature_id FOR SHARE OF f,ds"
    } else {
        "SELECT f.source_id,f.feature_id FROM jsonb_to_recordset($3::jsonb) AS x(source_id uuid,feature_id text)
         JOIN spatial.reference_features f ON f.organization_id=$1 AND f.project_id=$2 AND f.source_id=x.source_id AND f.feature_id=x.feature_id
         ORDER BY f.source_id,f.feature_id FOR SHARE OF f"
    };
    let payload=serde_json::to_value(links).map_err(|_|AppError::internal("feature reference serialization failed"))?;
    let rows=sqlx::query(sql).bind(edit.organization_id()).bind(edit.project_id()).bind(payload)
        .fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    rows.iter().map(|row|Ok(SpatialFeatureRef{source_id:row.try_get("source_id").map_err(db_error)?,feature_id:row.try_get("feature_id").map_err(db_error)?})).collect()
}
