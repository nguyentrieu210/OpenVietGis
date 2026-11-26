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

const MAX_ACTIVATION_REVISIONS: i64 = 1_000;

pub async fn activation_history(
    read: &mut ProjectRead<'_>,
    activation_id: uuid::Uuid,
) -> Result<Value, AppError> {
    let visible = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id
         FROM extensions.project_activations
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(activation_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if visible.is_none() {
        return Err(AppError::not_found("extension activation not found"));
    }

    let rows = sqlx::query(
        "SELECT id, event, previous_manifest_id, manifest_id, status, actor_id, occurred_at
         FROM extensions.activation_events
         WHERE organization_id = $1 AND project_id = $2 AND activation_id = $3
         ORDER BY occurred_at, id
         LIMIT $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(activation_id)
    .bind(MAX_ACTIVATION_REVISIONS + 1)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    if rows.len() > MAX_ACTIVATION_REVISIONS as usize {
        return Err(AppError::validation(
            "EXTENSION_TRANSFER_LIMIT",
            "activation history exceeds portable limit",
        ));
    }

    let items = rows
        .into_iter()
        .map(|row| {
            let previous_manifest_id = row
                .try_get::<Option<uuid::Uuid>, _>("previous_manifest_id")
                .map_err(db_error)?;
            Ok(json!({
                "id": row.try_get::<uuid::Uuid, _>("id").map_err(db_error)?,
                "event": row.try_get::<String, _>("event").map_err(db_error)?,
                "previous_manifest_id": previous_manifest_id,
                "manifest_id": row.try_get::<uuid::Uuid, _>("manifest_id").map_err(db_error)?,
                "status": row.try_get::<String, _>("status").map_err(db_error)?,
                "actor_id": row.try_get::<uuid::Uuid, _>("actor_id").map_err(db_error)?,
                "occurred_at": iso(
                    row.try_get::<DateTime<Utc>, _>("occurred_at")
                        .map_err(db_error)?
                ),
            }))
        })
        .collect::<Result<Vec<Value>, AppError>>()?;

    Ok(json!({ "items": items }))
}

#[derive(Debug, Clone)]
pub struct ManifestContractRow {
    pub id: uuid::Uuid,
    pub extension_key: String,
    pub version: String,
    pub api_compatibility: String,
    pub display_name: String,
    pub description: String,
    pub permissions: Value,
    pub commands: Value,
    pub capabilities: Value,
    pub entity_schemas: Value,
    pub ui_contributions: Value,
    pub enabled: bool,
}

impl ManifestContractRow {
    pub fn contract(&self) -> Value {
        json!({
            "extension_key": self.extension_key,
            "version": self.version,
            "api_compatibility": self.api_compatibility,
            "display_name": self.display_name,
            "description": self.description,
            "permissions": self.permissions,
            "commands": self.commands,
            "capabilities": self.capabilities,
            "entity_schemas": self.entity_schemas,
            "ui_contributions": self.ui_contributions,
        })
    }
}

pub async fn manifest(
    read: &mut ProjectRead<'_>,
    manifest_id: uuid::Uuid,
) -> Result<Option<ManifestContractRow>, AppError> {
    let row = sqlx::query(
        "SELECT id,extension_key,version,api_compatibility,display_name,description,
                permissions,commands,capabilities,entity_schemas,ui_contributions,enabled
         FROM extensions.manifests
         WHERE organization_id=$1 AND id=$2",
    )
    .bind(read.organization_id())
    .bind(manifest_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    row.map(|row| {
        Ok(ManifestContractRow {
            id: row.try_get("id").map_err(db_error)?,
            extension_key: row.try_get("extension_key").map_err(db_error)?,
            version: row.try_get("version").map_err(db_error)?,
            api_compatibility: row.try_get("api_compatibility").map_err(db_error)?,
            display_name: row.try_get("display_name").map_err(db_error)?,
            description: row.try_get("description").map_err(db_error)?,
            permissions: row.try_get("permissions").map_err(db_error)?,
            commands: row.try_get("commands").map_err(db_error)?,
            capabilities: row.try_get("capabilities").map_err(db_error)?,
            entity_schemas: row.try_get("entity_schemas").map_err(db_error)?,
            ui_contributions: row.try_get("ui_contributions").map_err(db_error)?,
            enabled: row.try_get("enabled").map_err(db_error)?,
        })
    })
    .transpose()
}

fn activation_wire(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<uuid::Uuid,_>("id").map_err(db_error)?,
        "manifest_id": row.try_get::<uuid::Uuid,_>("manifest_id").map_err(db_error)?,
        "extension_key": row.try_get::<String,_>("extension_key").map_err(db_error)?,
        "status": row.try_get::<String,_>("status").map_err(db_error)?,
        "revision": row.try_get::<i64,_>("revision").map_err(db_error)?,
        "activated_at": iso(row.try_get::<DateTime<Utc>,_>("activated_at").map_err(db_error)?),
        "disabled_at": row.try_get::<Option<DateTime<Utc>>,_>("disabled_at").map_err(db_error)?.map(iso),
        "updated_at": iso(row.try_get::<DateTime<Utc>,_>("updated_at").map_err(db_error)?),
    }))
}

async fn activation_event(
    edit: &mut ProjectRead<'_>,
    row: &sqlx::postgres::PgRow,
    actor_id: uuid::Uuid,
    event: &str,
    previous_manifest_id: Option<uuid::Uuid>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO extensions.activation_events (
           id,organization_id,project_id,activation_id,event,previous_manifest_id,
           manifest_id,status,actor_id,occurred_at
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(row.try_get::<uuid::Uuid, _>("id").map_err(db_error)?)
    .bind(event)
    .bind(previous_manifest_id)
    .bind(
        row.try_get::<uuid::Uuid, _>("manifest_id")
            .map_err(db_error)?,
    )
    .bind(row.try_get::<String, _>("status").map_err(db_error)?)
    .bind(actor_id)
    .bind(
        row.try_get::<DateTime<Utc>, _>("updated_at")
            .map_err(db_error)?,
    )
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn activation_audit(
    edit: &mut ProjectRead<'_>,
    actor_id: uuid::Uuid,
    actor_display: &str,
    action: &str,
    activation_id: uuid::Uuid,
    metadata: Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO audit.audit_events (
          id,organization_id,actor_type,actor_id,actor_display_snapshot,
          action,resource_type,resource_id,outcome,metadata,created_at
         ) VALUES($1,$2,'human',$3,$4,$5,'extension_activation',$6,'success',$7,$8)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(actor_id)
    .bind(actor_display)
    .bind(action)
    .bind(activation_id)
    .bind(metadata)
    .bind(occurred_at)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

pub async fn activate(
    edit: &mut ProjectRead<'_>,
    actor_id: uuid::Uuid,
    actor_display: &str,
    manifest: &ManifestContractRow,
) -> Result<Value, AppError> {
    let existing = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT manifest_id FROM extensions.project_activations
         WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3
         FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(&manifest.extension_key)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if existing.is_some_and(|id| id != manifest.id) {
        return Err(AppError::conflict(
            "EXTENSION_VERSION_TRANSITION_REQUIRED",
            "version transitions require a verified migration; implicit replacement is blocked",
        ));
    }
    let now = Utc::now();
    let row = sqlx::query(
        "INSERT INTO extensions.project_activations (
           id,organization_id,project_id,manifest_id,extension_key,status,revision,
           activated_by_actor_id,activated_at,disabled_by_actor_id,disabled_at,updated_at
         ) VALUES($1,$2,$3,$4,$5,'active',1,$6,$7,NULL,NULL,$7)
         ON CONFLICT (organization_id,project_id,extension_key) DO UPDATE
         SET status='active',
             revision=extensions.project_activations.revision+1,
             activated_by_actor_id=EXCLUDED.activated_by_actor_id,
             activated_at=EXCLUDED.activated_at,
             disabled_by_actor_id=NULL,disabled_at=NULL,updated_at=EXCLUDED.updated_at
         RETURNING *",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(manifest.id)
    .bind(&manifest.extension_key)
    .bind(actor_id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let activation_id = row.try_get::<uuid::Uuid, _>("id").map_err(db_error)?;
    activation_audit(
        edit,
        actor_id,
        actor_display,
        "extension.activate",
        activation_id,
        json!({"manifest_id":manifest.id,"extension_key":manifest.extension_key}),
        now,
    )
    .await?;
    activation_event(edit, &row, actor_id, "activate", existing).await?;
    activation_wire(&row)
}

pub async fn disable(
    edit: &mut ProjectRead<'_>,
    actor_id: uuid::Uuid,
    actor_display: &str,
    extension_key: &str,
) -> Result<Value, AppError> {
    let now = Utc::now();
    let changed = sqlx::query(
        "UPDATE extensions.project_activations
         SET status='disabled',revision=revision+1,disabled_by_actor_id=$4,
             disabled_at=$5,updated_at=$5
         WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3 AND status='active'
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(extension_key)
    .bind(actor_id)
    .bind(now)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let (row, changed) = if let Some(row) = changed {
        (row, true)
    } else {
        let row = sqlx::query(
            "SELECT * FROM extensions.project_activations
         WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(extension_key)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("project extension activation not found"))?;
        (row, false)
    };
    if changed {
        let activation_id = row.try_get::<uuid::Uuid, _>("id").map_err(db_error)?;
        let manifest_id = row
            .try_get::<uuid::Uuid, _>("manifest_id")
            .map_err(db_error)?;
        let revision = row.try_get::<i64, _>("revision").map_err(db_error)?;
        activation_audit(
            edit,
            actor_id,
            actor_display,
            "extension.disable",
            activation_id,
            json!({"manifest_id":manifest_id,"extension_key":extension_key,"revision":revision}),
            now,
        )
        .await?;
        activation_event(edit, &row, actor_id, "deactivate", Some(manifest_id)).await?;
    }
    activation_wire(&row)
}

pub async fn deactivate(
    edit: &mut ProjectRead<'_>,
    actor_id: uuid::Uuid,
    actor_display: &str,
    activation_id: uuid::Uuid,
) -> Result<Value, AppError> {
    let now = Utc::now();
    let row = sqlx::query(
        "UPDATE extensions.project_activations
         SET status='disabled',updated_at=$4
         WHERE id=$3 AND organization_id=$1 AND project_id=$2
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(activation_id)
    .bind(now)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("extension activation not found"))?;
    let manifest_id = row
        .try_get::<uuid::Uuid, _>("manifest_id")
        .map_err(db_error)?;
    let extension_key = row
        .try_get::<String, _>("extension_key")
        .map_err(db_error)?;
    activation_audit(
        edit,
        actor_id,
        actor_display,
        "extension.deactivate",
        activation_id,
        json!({"manifest_id":manifest_id,"extension_key":extension_key}),
        now,
    )
    .await?;
    activation_event(edit, &row, actor_id, "deactivate", Some(manifest_id)).await?;
    activation_wire(&row)
}

pub async fn transition(
    edit: &mut ProjectRead<'_>,
    actor_id: uuid::Uuid,
    actor_display: &str,
    activation_id: uuid::Uuid,
    expected_manifest_id: uuid::Uuid,
    target_manifest_id: uuid::Uuid,
) -> Result<Value, AppError> {
    let current = sqlx::query(
        "SELECT * FROM extensions.project_activations
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(activation_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("extension activation not found"))?;
    let current_manifest = current
        .try_get::<uuid::Uuid, _>("manifest_id")
        .map_err(db_error)?;
    if current_manifest != expected_manifest_id {
        return Err(AppError::conflict(
            "STALE_EXTENSION_VERSION",
            "activation version changed",
        ));
    }
    let now = Utc::now();
    let row = sqlx::query(
        "UPDATE extensions.project_activations
         SET manifest_id=$4,updated_at=$5
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND manifest_id=$6
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(activation_id)
    .bind(target_manifest_id)
    .bind(now)
    .bind(expected_manifest_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    activation_event(
        edit,
        &row,
        actor_id,
        "transition",
        Some(expected_manifest_id),
    )
    .await?;
    activation_audit(edit,actor_id,actor_display,"extension.transition",activation_id,
      json!({"previous_manifest_id":expected_manifest_id,"manifest_id":target_manifest_id,"mode":"metadata_only"}),now).await?;
    activation_wire(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_history_limit_matches_python() {
        assert_eq!(MAX_ACTIVATION_REVISIONS, 1_000);
    }

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T03:04:05.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T03:04:05.123456+00:00");
    }
}
