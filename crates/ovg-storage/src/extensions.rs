//! E14 extension activation read repository over canonical RLS tables.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "extension activation database operation failed");
    AppError::internal("extension activation database operation failed")
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

#[derive(Debug)]
pub struct ActivationManifestRow {
    pub wire: Value,
    pub extension_key: String,
    pub manifest_version: String,
    pub manifest_enabled: bool,
    pub manifest_contract: Value,
}

pub async fn list_activations(
    read: &mut ProjectRead<'_>,
) -> Result<Vec<ActivationManifestRow>, AppError> {
    let rows = sqlx::query(
        "SELECT
            a.id, a.manifest_id, a.extension_key, a.status, a.revision,
            a.activated_at, a.disabled_at, a.updated_at,
            m.version, m.api_compatibility, m.display_name, m.description,
            m.permissions, m.commands, m.capabilities, m.entity_schemas,
            m.ui_contributions, m.enabled
         FROM extensions.project_activations a
         JOIN extensions.manifests m
           ON m.organization_id = a.organization_id AND m.id = a.manifest_id
         WHERE a.organization_id = $1 AND a.project_id = $2
         ORDER BY a.extension_key ASC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    rows.into_iter()
        .map(|row| {
            let extension_key: String = row.try_get("extension_key").map_err(db_error)?;
            let version: String = row.try_get("version").map_err(db_error)?;
            let disabled_at = row
                .try_get::<Option<DateTime<Utc>>, _>("disabled_at")
                .map_err(db_error)?
                .map(iso);
            let wire = json!({
                "id": row.try_get::<uuid::Uuid, _>("id").map_err(db_error)?,
                "manifest_id": row.try_get::<uuid::Uuid, _>("manifest_id").map_err(db_error)?,
                "extension_key": extension_key,
                "status": row.try_get::<String, _>("status").map_err(db_error)?,
                "revision": row.try_get::<i64, _>("revision").map_err(db_error)?,
                "activated_at": iso(row.try_get::<DateTime<Utc>, _>("activated_at").map_err(db_error)?),
                "disabled_at": disabled_at,
                "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
            });
            let manifest_contract = json!({
                "extension_key": extension_key,
                "version": version,
                "api_compatibility": row.try_get::<String, _>("api_compatibility").map_err(db_error)?,
                "display_name": row.try_get::<String, _>("display_name").map_err(db_error)?,
                "description": row.try_get::<String, _>("description").map_err(db_error)?,
                "permissions": row.try_get::<Value, _>("permissions").map_err(db_error)?,
                "commands": row.try_get::<Value, _>("commands").map_err(db_error)?,
                "capabilities": row.try_get::<Value, _>("capabilities").map_err(db_error)?,
                "entity_schemas": row.try_get::<Value, _>("entity_schemas").map_err(db_error)?,
                "ui_contributions": row.try_get::<Value, _>("ui_contributions").map_err(db_error)?,
            });
            Ok(ActivationManifestRow {
                wire,
                extension_key,
                manifest_version: version,
                manifest_enabled: row.try_get("enabled").map_err(db_error)?,
                manifest_contract,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T03:04:05.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T03:04:05.123456+00:00");
    }
}
