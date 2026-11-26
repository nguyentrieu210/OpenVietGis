//! E14 persistent partner contributions and project-scoped integration event journal.

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

use super::validation::validate_contributions;
use super::{db_error, iso, iso_z};

pub async fn list_contributions(read: &mut ProjectRead<'_>) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT c.manifest_id, c.document, c.revision, c.updated_at, m.display_name
         FROM extensions.partner_contributions c
         JOIN extensions.project_activations a
           ON a.organization_id = c.organization_id
          AND a.project_id = c.project_id
          AND a.manifest_id = c.manifest_id
          AND a.status = 'active'
         JOIN extensions.manifests m
           ON m.id = c.manifest_id
          AND m.organization_id = c.organization_id
         WHERE c.organization_id = $1 AND c.project_id = $2 AND m.enabled
         ORDER BY c.manifest_id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    rows.into_iter()
        .map(|row| {
            Ok(json!({
                "manifest_id": row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?,
                "document": row.try_get::<Value, _>("document").map_err(db_error)?,
                "revision": row.try_get::<i32, _>("revision").map_err(db_error)?,
                "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
                "display_name": row.try_get::<String, _>("display_name").map_err(db_error)?,
            }))
        })
        .collect()
}

pub async fn save_contributions(
    edit: &mut ProjectRead<'_>,
    manifest_id: Uuid,
    expected_revision: i32,
    document: &Value,
) -> Result<Value, AppError> {
    let meta = sqlx::query(
        "SELECT m.capabilities, m.display_name
         FROM extensions.project_activations a
         JOIN extensions.manifests m
           ON a.manifest_id = m.id AND a.organization_id = m.organization_id
         WHERE a.organization_id = $1 AND a.project_id = $2 AND a.status = 'active'
           AND m.enabled AND left(m.extension_key, 8) = 'partner_' AND m.id = $3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(manifest_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("active partner manifest not found"))?;

    let caps_val: Value = meta.try_get("capabilities").map_err(db_error)?;
    let capabilities: Vec<String> = serde_json::from_value(caps_val).unwrap_or_default();
    let display_name: String = meta.try_get("display_name").map_err(db_error)?;

    let cleaned = validate_contributions(document, &capabilities)?;

    let existing: Option<i32> = sqlx::query_scalar(
        "SELECT revision FROM extensions.partner_contributions
         WHERE organization_id = $1 AND project_id = $2 AND manifest_id = $3 FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(manifest_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    if existing.unwrap_or(0) != expected_revision {
        return Err(AppError::conflict(
            "CONFLICT",
            "contribution revision changed",
        ));
    }

    let row = sqlx::query(
        "INSERT INTO extensions.partner_contributions
            (organization_id, project_id, manifest_id, document, revision)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (organization_id, project_id, manifest_id)
         DO UPDATE SET document = EXCLUDED.document, revision = EXCLUDED.revision, updated_at = now()
         RETURNING manifest_id, document, revision, updated_at",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(manifest_id)
    .bind(&cleaned)
    .bind(expected_revision + 1)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    Ok(json!({
        "manifest_id": row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?,
        "document": row.try_get::<Value, _>("document").map_err(db_error)?,
        "revision": row.try_get::<i32, _>("revision").map_err(db_error)?,
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
        "display_name": display_name,
    }))
}

pub async fn list_events(
    read: &mut ProjectRead<'_>,
    after: i64,
    limit: i64,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT sequence, id, event_kind, resource_type, resource_id, revision_ref, created_at
         FROM extensions.integration_events
         WHERE organization_id = $1 AND project_id = $2 AND sequence > $3
         ORDER BY sequence LIMIT $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(after)
    .bind(limit)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    let head: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence), 0)
         FROM extensions.integration_events
         WHERE organization_id = $1 AND project_id = $2",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    let mut next_after = after;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let seq: i64 = row.try_get("sequence").map_err(db_error)?;
        next_after = seq;
        items.push(json!({
            "sequence": seq,
            "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
            "event_kind": row.try_get::<String, _>("event_kind").map_err(db_error)?,
            "resource_type": row.try_get::<String, _>("resource_type").map_err(db_error)?,
            "resource_id": row.try_get::<Uuid, _>("resource_id").map_err(db_error)?,
            "revision_ref": row.try_get::<Option<String>, _>("revision_ref").map_err(db_error)?,
            "created_at": iso_z(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
        }));
    }

    Ok(json!({
        "items": items,
        "head_sequence": head,
        "next_after_sequence": next_after,
    }))
}
