//! E14 extension data records, revision history, and portable export/import.

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

use super::validation::{validate_generic, validate_record_against_schema};
use super::{
    activation_history_items, activation_row, active_manifest, append_project_audit, bundle_digest,
    canonical_bytes, db_error, iso, manifest, semantic_eq, MAX_BUNDLE_BYTES, MAX_RECORDS,
    MAX_REVISIONS, PORTABLE_FORMAT,
};

fn record_wire(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    let linked_layer_id = row
        .try_get::<Option<Uuid>, _>("linked_layer_id")
        .map_err(db_error)?;
    let linked_feature_id = row
        .try_get::<Option<String>, _>("linked_feature_id")
        .map_err(db_error)?
        .filter(|s| !s.is_empty());
    let operation_id = row
        .try_get::<Option<Uuid>, _>("operation_id")
        .map_err(db_error)?;
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "extension_key": row.try_get::<String, _>("extension_key").map_err(db_error)?,
        "entity_name": row.try_get::<String, _>("entity_name").map_err(db_error)?,
        "payload": row.try_get::<Value, _>("payload").map_err(db_error)?,
        "linked_layer_id": linked_layer_id,
        "linked_feature_id": linked_feature_id,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
        "manifest_id": row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?,
        "revision": row.try_get::<i32, _>("revision").map_err(db_error)?,
        "operation_id": operation_id,
        "link_context": row.try_get::<Option<Value>, _>("link_context").map_err(db_error)?,
    }))
}

async fn current_link(
    scope: &mut ProjectRead<'_>,
    linked_layer_id: Option<Uuid>,
    linked_feature_id: Option<&str>,
) -> Result<Option<Value>, AppError> {
    if linked_feature_id.is_some() && linked_layer_id.is_none() {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "linked feature requires a layer",
        ));
    }
    let Some(layer_id) = linked_layer_id else {
        return Ok(None);
    };
    if let Some(fid) = linked_feature_id {
        if fid.is_empty() || fid.chars().count() > 128 {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid linked feature identifier",
            ));
        }
    }

    let link = sqlx::query(
        "SELECT l.source_id, s.content_revision, s.schema_revision, s.stored_crs
         FROM spatial.layers l
         JOIN spatial.data_sources s ON s.id = l.source_id
           AND s.organization_id = l.organization_id AND s.project_id = l.project_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
           AND s.validation_state = 'ready' AND s.stored_crs IS NOT NULL",
    )
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(layer_id)
    .fetch_optional(&mut **scope.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("linked layer not found or unavailable"))?;

    let source_id: Uuid = link.try_get("source_id").map_err(db_error)?;
    let content_revision: String = link.try_get("content_revision").map_err(db_error)?;
    let schema_revision: String = link.try_get("schema_revision").map_err(db_error)?;
    let stored_crs: i32 = link.try_get("stored_crs").map_err(db_error)?;

    if let Some(fid) = linked_feature_id {
        let found: Option<String> = sqlx::query_scalar(
            "SELECT feature_id FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = $3 AND feature_id = $4
               AND content_revision = $5 AND schema_revision = $6",
        )
        .bind(scope.organization_id())
        .bind(scope.project_id())
        .bind(source_id)
        .bind(fid)
        .bind(&content_revision)
        .bind(&schema_revision)
        .fetch_optional(&mut **scope.transaction())
        .await
        .map_err(db_error)?;
        if found.is_none() {
            return Err(AppError::not_found("linked feature not found"));
        }
    }

    Ok(Some(json!({
        "status": "pinned",
        "source_id": source_id.to_string(),
        "content_revision": content_revision,
        "schema_revision": schema_revision,
        "stored_crs": stored_crs,
    })))
}

async fn assert_link(
    scope: &mut ProjectRead<'_>,
    linked_layer_id: Option<Uuid>,
    linked_feature_id: Option<&str>,
    expected_link_context: Option<&Value>,
) -> Result<Option<Value>, AppError> {
    let link = current_link(scope, linked_layer_id, linked_feature_id).await?;
    let matches = match (&link, expected_link_context) {
        (None, None) => true,
        (Some(a), Some(b)) => semantic_eq(a, b),
        _ => false,
    };
    if !matches {
        return Err(AppError::conflict(
            "STALE_EXTENSION_LINK",
            "linked Core revision differs or was not explicitly supplied",
        ));
    }
    Ok(link)
}

async fn record_row(
    scope: &mut ProjectRead<'_>,
    extension_key: &str,
    record_id: Uuid,
) -> Result<sqlx::postgres::PgRow, AppError> {
    sqlx::query(
        "SELECT * FROM extensions.records
         WHERE organization_id = $1 AND project_id = $2
           AND extension_key = $3 AND id = $4",
    )
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(extension_key)
    .bind(record_id)
    .fetch_optional(&mut **scope.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("extension record not found"))
}

async fn append_revision(
    edit: &mut ProjectRead<'_>,
    row: &sqlx::postgres::PgRow,
    actor_id: Uuid,
    event: &str,
    import_origin: Option<&Value>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO extensions.record_revisions (
            organization_id, project_id, record_id, revision, manifest_id, entity_name,
            payload, linked_layer_id, linked_feature_id, link_context, actor_id, event,
            occurred_at, import_origin
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(row.try_get::<Uuid, _>("id").map_err(db_error)?)
    .bind(row.try_get::<i32, _>("revision").map_err(db_error)?)
    .bind(row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?)
    .bind(row.try_get::<String, _>("entity_name").map_err(db_error)?)
    .bind(row.try_get::<Value, _>("payload").map_err(db_error)?)
    .bind(
        row.try_get::<Option<Uuid>, _>("linked_layer_id")
            .map_err(db_error)?,
    )
    .bind(
        row.try_get::<Option<String>, _>("linked_feature_id")
            .map_err(db_error)?,
    )
    .bind(
        row.try_get::<Option<Value>, _>("link_context")
            .map_err(db_error)?,
    )
    .bind(actor_id)
    .bind(event)
    .bind(
        row.try_get::<DateTime<Utc>, _>("updated_at")
            .map_err(db_error)?,
    )
    .bind(import_origin)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_record(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    manifest: &super::ManifestContractRow,
    entity_name: &str,
    payload: &Value,
    linked_layer_id: Option<Uuid>,
    linked_feature_id: Option<&str>,
    link_context: Option<&Value>,
    import_origin: Option<&Value>,
    operation_id: Option<Uuid>,
) -> Result<Value, AppError> {
    let cleaned = validate_record_against_schema(entity_name, payload, &manifest.entity_schemas)?;
    let now = Utc::now();
    let row = sqlx::query(
        "INSERT INTO extensions.records (
            id, organization_id, project_id, extension_key, entity_name,
            payload, linked_layer_id, linked_feature_id, created_by_actor_id,
            updated_by_actor_id, created_at, updated_at, manifest_id, revision,
            link_context, operation_id
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$10,$11,1,$12,$13)
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(&manifest.extension_key)
    .bind(entity_name)
    .bind(&cleaned)
    .bind(linked_layer_id)
    .bind(linked_feature_id)
    .bind(actor_id)
    .bind(now)
    .bind(manifest.id)
    .bind(link_context)
    .bind(operation_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    let event = if import_origin.is_some() {
        "import"
    } else {
        "create"
    };
    append_revision(edit, &row, actor_id, event, import_origin).await?;
    let record_id = row.try_get::<Uuid, _>("id").map_err(db_error)?;
    append_project_audit(
        edit,
        actor_id,
        actor_display,
        &format!("extension.record.{event}"),
        "extension_record",
        record_id,
        json!({"manifest_id": manifest.id.to_string(), "revision": 1}),
        now,
    )
    .await?;
    record_wire(&row)
}

pub struct CreateRecordInput<'a> {
    pub entity_name: &'a str,
    pub payload: &'a Value,
    pub linked_layer_id: Option<Uuid>,
    pub linked_feature_id: Option<&'a str>,
    pub expected_link_context: Option<&'a Value>,
    pub operation_id: Option<Uuid>,
}

pub struct UpdateRecordInput<'a> {
    pub record_id: Uuid,
    pub expected_revision: i32,
    pub expected_manifest_id: Option<Uuid>,
    pub payload: &'a Value,
    pub expected_link_context: Option<&'a Value>,
    pub operation_id: Option<Uuid>,
}

pub async fn list_records(
    read: &mut ProjectRead<'_>,
    extension_key: &str,
    entity_name: Option<&str>,
) -> Result<Vec<Value>, AppError> {
    activation_row(read, extension_key).await?;
    let rows = match entity_name.filter(|s| !s.is_empty()) {
        Some(ename) => sqlx::query(
            "SELECT * FROM extensions.records
                 WHERE organization_id = $1 AND project_id = $2
                   AND extension_key = $3 AND entity_name = $4
                 ORDER BY created_at DESC LIMIT 200",
        )
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(extension_key)
        .bind(ename)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?,
        None => sqlx::query(
            "SELECT * FROM extensions.records
                 WHERE organization_id = $1 AND project_id = $2 AND extension_key = $3
                 ORDER BY created_at DESC LIMIT 200",
        )
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(extension_key)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?,
    };
    rows.iter().map(record_wire).collect()
}

pub async fn create_record(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    extension_key: &str,
    input: CreateRecordInput<'_>,
) -> Result<Value, AppError> {
    let manifest = active_manifest(edit, extension_key).await?;
    if let Some(op_id) = input.operation_id {
        let existing = sqlx::query(
            "SELECT * FROM extensions.records
             WHERE organization_id = $1 AND project_id = $2
               AND extension_key = $3 AND operation_id = $4",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(extension_key)
        .bind(op_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if let Some(row) = existing {
            return record_wire(&row);
        }
    }
    let link = assert_link(
        edit,
        input.linked_layer_id,
        input.linked_feature_id,
        input.expected_link_context,
    )
    .await?;
    insert_record(
        edit,
        actor_id,
        actor_display,
        &manifest,
        input.entity_name,
        input.payload,
        input.linked_layer_id,
        input.linked_feature_id,
        link.as_ref(),
        None,
        input.operation_id,
    )
    .await
}

pub async fn update_record(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    extension_key: &str,
    input: UpdateRecordInput<'_>,
) -> Result<Value, AppError> {
    let manifest = active_manifest(edit, extension_key).await?;
    if let Some(op_id) = input.operation_id {
        let prior = sqlx::query(
            "SELECT * FROM extensions.records
             WHERE organization_id = $1 AND project_id = $2
               AND extension_key = $3 AND operation_id = $4",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(extension_key)
        .bind(op_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if let Some(row) = prior {
            let prior_id: Uuid = row.try_get("id").map_err(db_error)?;
            if prior_id != input.record_id {
                return Err(AppError::conflict(
                    "OPERATION_REPLAY_CONFLICT",
                    "operation_id already used by another record",
                ));
            }
            return record_wire(&row);
        }
    }

    let old = record_row(edit, extension_key, input.record_id).await?;
    let old_revision: i32 = old.try_get("revision").map_err(db_error)?;
    let effective_expected_manifest = input.expected_manifest_id.unwrap_or(manifest.id);
    if old_revision != input.expected_revision || manifest.id != effective_expected_manifest {
        return Err(AppError::conflict(
            "STALE_EXTENSION_RECORD",
            "record or active manifest version changed",
        ));
    }

    let old_lid: Option<Uuid> = old.try_get("linked_layer_id").map_err(db_error)?;
    let old_fid: Option<String> = old.try_get("linked_feature_id").map_err(db_error)?;
    let link = assert_link(
        edit,
        old_lid,
        old_fid.as_deref(),
        input.expected_link_context,
    )
    .await?;

    let entity_name: String = old.try_get("entity_name").map_err(db_error)?;
    let cleaned =
        validate_record_against_schema(&entity_name, input.payload, &manifest.entity_schemas)?;

    let old_payload: Value = old.try_get("payload").map_err(db_error)?;
    let old_manifest_id: Uuid = old.try_get("manifest_id").map_err(db_error)?;
    let old_link_context: Option<Value> = old.try_get("link_context").map_err(db_error)?;
    let link_same = match (&old_link_context, &link) {
        (None, None) => true,
        (Some(a), Some(b)) => semantic_eq(a, b),
        _ => false,
    };
    if semantic_eq(&cleaned, &old_payload) && old_manifest_id == manifest.id && link_same {
        return record_wire(&old);
    }

    let now = Utc::now();
    let row = sqlx::query(
        "UPDATE extensions.records
         SET payload = $4, revision = revision + 1, manifest_id = $5,
             link_context = $6, operation_id = $7, updated_by_actor_id = $8, updated_at = $9
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 AND revision = $10
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.record_id)
    .bind(&cleaned)
    .bind(manifest.id)
    .bind(link.as_ref())
    .bind(input.operation_id)
    .bind(actor_id)
    .bind(now)
    .bind(input.expected_revision)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    append_revision(edit, &row, actor_id, "update", None).await?;
    let new_revision: i32 = row.try_get("revision").map_err(db_error)?;
    append_project_audit(
        edit,
        actor_id,
        actor_display,
        "extension.record.update",
        "extension_record",
        input.record_id,
        json!({
            "manifest_id": manifest.id.to_string(),
            "revision": new_revision,
            "previous_revision": input.expected_revision,
        }),
        now,
    )
    .await?;
    record_wire(&row)
}

async fn history_items(
    read: &mut ProjectRead<'_>,
    record_id: Uuid,
) -> Result<Vec<Value>, AppError> {
    let (count, total_bytes): (i64, i64) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(octet_length(payload::text)
            + coalesce(octet_length(link_context::text), 0)
            + coalesce(octet_length(import_origin::text), 0)), 0)
         FROM extensions.record_revisions
         WHERE organization_id = $1 AND project_id = $2 AND record_id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(record_id)
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if count > MAX_REVISIONS || total_bytes > MAX_BUNDLE_BYTES as i64 {
        return Err(AppError::validation(
            "EXTENSION_TRANSFER_LIMIT",
            "record history exceeds portable limit",
        ));
    }

    let rows = sqlx::query(
        "SELECT * FROM extensions.record_revisions
         WHERE organization_id = $1 AND project_id = $2 AND record_id = $3
         ORDER BY revision ASC LIMIT $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(record_id)
    .bind(MAX_REVISIONS + 1)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if rows.len() > MAX_REVISIONS as usize {
        return Err(AppError::validation(
            "EXTENSION_TRANSFER_LIMIT",
            "record history exceeds portable limit",
        ));
    }

    rows.into_iter()
        .map(|r| {
            let linked_layer_id = r
                .try_get::<Option<Uuid>, _>("linked_layer_id")
                .map_err(db_error)?;
            let linked_feature_id = r
                .try_get::<Option<String>, _>("linked_feature_id")
                .map_err(db_error)?;
            Ok(json!({
                "revision": r.try_get::<i32, _>("revision").map_err(db_error)?,
                "manifest_id": r.try_get::<Uuid, _>("manifest_id").map_err(db_error)?.to_string(),
                "entity_name": r.try_get::<String, _>("entity_name").map_err(db_error)?,
                "payload": r.try_get::<Value, _>("payload").map_err(db_error)?,
                "linked_layer_id": linked_layer_id.map(|id| id.to_string()),
                "linked_feature_id": linked_feature_id,
                "link_context": r.try_get::<Option<Value>, _>("link_context").map_err(db_error)?,
                "actor_id": r.try_get::<Uuid, _>("actor_id").map_err(db_error)?.to_string(),
                "occurred_at": iso(r.try_get::<DateTime<Utc>, _>("occurred_at").map_err(db_error)?),
                "event": r.try_get::<String, _>("event").map_err(db_error)?,
                "import_origin": r.try_get::<Option<Value>, _>("import_origin").map_err(db_error)?,
            }))
        })
        .collect()
}

pub async fn record_history(
    read: &mut ProjectRead<'_>,
    extension_key: &str,
    record_id: Uuid,
) -> Result<Vec<Value>, AppError> {
    record_row(read, extension_key, record_id).await?;
    history_items(read, record_id).await
}

pub async fn export_bundle(
    read: &mut ProjectRead<'_>,
    extension_key: &str,
) -> Result<Value, AppError> {
    let activation = activation_row(read, extension_key).await?;
    let activation_id: Uuid = activation.try_get("id").map_err(db_error)?;
    let active_manifest_id: Uuid = activation.try_get("manifest_id").map_err(db_error)?;
    let activation_status: String = activation.try_get("status").map_err(db_error)?;

    let events = activation_history_items(read, activation_id).await?;
    let rows = sqlx::query(
        "SELECT * FROM extensions.records
         WHERE organization_id = $1 AND project_id = $2 AND extension_key = $3
         ORDER BY id LIMIT $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(extension_key)
    .bind(MAX_RECORDS as i64 + 1)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if rows.len() > MAX_RECORDS {
        return Err(AppError::validation(
            "EXTENSION_TRANSFER_LIMIT",
            "record count exceeds portable limit",
        ));
    }

    let mut manifest_ids = BTreeSet::new();
    manifest_ids.insert(active_manifest_id);
    for e in &events {
        if let Some(mid) = e
            .get("manifest_id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            manifest_ids.insert(mid);
        }
        if let Some(mid) = e
            .get("previous_manifest_id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            manifest_ids.insert(mid);
        }
    }

    let mut count = 0usize;
    let mut bytes_used = canonical_bytes(&Value::Array(events.clone()), MAX_BUNDLE_BYTES)?.len();
    let mut records = Vec::with_capacity(rows.len());

    for row in rows {
        let rid: Uuid = row.try_get("id").map_err(db_error)?;
        let history = history_items(read, rid).await?;
        count += history.len();
        if count as i64 > MAX_REVISIONS {
            return Err(AppError::validation(
                "EXTENSION_TRANSFER_LIMIT",
                "revision count exceeds portable limit",
            ));
        }

        let rev: i32 = row.try_get("revision").map_err(db_error)?;
        let mid: Uuid = row.try_get("manifest_id").map_err(db_error)?;
        let entity_name: String = row.try_get("entity_name").map_err(db_error)?;
        let payload: Value = row.try_get("payload").map_err(db_error)?;
        let lid: Option<Uuid> = row.try_get("linked_layer_id").map_err(db_error)?;
        let fid: Option<String> = row.try_get("linked_feature_id").map_err(db_error)?;
        let link_context: Option<Value> = row.try_get("link_context").map_err(db_error)?;

        let latest = json!({
            "revision": rev,
            "manifest_id": mid.to_string(),
            "entity_name": entity_name,
            "payload": payload,
            "linked_layer_id": lid.map(|id| id.to_string()),
            "linked_feature_id": fid,
            "link_context": link_context,
        });
        let Some(last) = history.last() else {
            return Err(AppError::conflict(
                "CONFLICT",
                "record history is inconsistent",
            ));
        };
        let last_projected = json!({
            "revision": last.get("revision").cloned().unwrap_or(Value::Null),
            "manifest_id": last.get("manifest_id").cloned().unwrap_or(Value::Null),
            "entity_name": last.get("entity_name").cloned().unwrap_or(Value::Null),
            "payload": last.get("payload").cloned().unwrap_or(Value::Null),
            "linked_layer_id": last.get("linked_layer_id").cloned().unwrap_or(Value::Null),
            "linked_feature_id": last.get("linked_feature_id").cloned().unwrap_or(Value::Null),
            "link_context": last.get("link_context").cloned().unwrap_or(Value::Null),
        });
        if !semantic_eq(&latest, &last_projected) {
            return Err(AppError::conflict(
                "CONFLICT",
                "record history is inconsistent",
            ));
        }
        bytes_used += canonical_bytes(&Value::Array(history.clone()), MAX_BUNDLE_BYTES)?.len()
            + canonical_bytes(&latest, MAX_BUNDLE_BYTES)?.len();
        if bytes_used > MAX_BUNDLE_BYTES {
            return Err(AppError::validation(
                "EXTENSION_TRANSFER_LIMIT",
                "portable bundle exceeds byte limit",
            ));
        }

        for h in &history {
            if let Some(hmid) = h
                .get("manifest_id")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                manifest_ids.insert(hmid);
            }
        }

        records.push(json!({
            "record_id": rid.to_string(),
            "revision": rev,
            "manifest_id": mid.to_string(),
            "entity_name": entity_name,
            "payload": payload,
            "linked_layer_id": lid.map(|id| id.to_string()),
            "linked_feature_id": fid,
            "link_context": link_context,
            "history": history,
        }));
    }

    let mut manifests = Vec::with_capacity(manifest_ids.len());
    for mid in manifest_ids {
        let record = manifest(read, mid)
            .await?
            .ok_or_else(|| AppError::not_found("extension manifest not found"))?;
        let portable = record.portable_contract();
        bytes_used += canonical_bytes(&portable, MAX_BUNDLE_BYTES)?.len();
        if bytes_used > MAX_BUNDLE_BYTES {
            return Err(AppError::validation(
                "EXTENSION_TRANSFER_LIMIT",
                "portable bundle exceeds byte limit",
            ));
        }
        let mut entry = Map::new();
        entry.insert("manifest_id".to_owned(), json!(mid.to_string()));
        if let Some(obj) = portable.as_object() {
            for (k, v) in obj {
                entry.insert(k.clone(), v.clone());
            }
        }
        entry.insert("display_name".to_owned(), json!(record.display_name));
        entry.insert("description".to_owned(), json!(record.description));
        manifests.push(Value::Object(entry));
    }

    let bundle = json!({
        "format": PORTABLE_FORMAT,
        "extension_key": extension_key,
        "origin": {
            "organization_id": read.organization_id().to_string(),
            "project_id": read.project_id().to_string(),
        },
        "active_manifest_id": active_manifest_id.to_string(),
        "activation_status": activation_status,
        "manifests": manifests,
        "activation_history": events,
        "records": records,
    });
    let sha256 = bundle_digest(&bundle)?;
    Ok(json!({ "bundle": bundle, "sha256": sha256 }))
}

struct ParsedImportItem {
    source_record_id: Uuid,
    revision: i32,
    entity_name: String,
    payload: Value,
    linked_layer_id: Option<Uuid>,
    linked_feature_id: Option<String>,
    link_context: Option<Value>,
}

pub async fn import_bundle(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    extension_key: &str,
    expected_manifest_id: Uuid,
    bundle: &Value,
    sha256: &str,
) -> Result<Value, AppError> {
    let manifest = active_manifest(edit, extension_key).await?;
    if !validate_generic(&manifest) {
        return Err(AppError::conflict(
            "EXTENSION_MANIFEST_REVALIDATION_REQUIRED",
            "stored manifest no longer satisfies the metadata contract",
        ));
    }
    if manifest.id != expected_manifest_id {
        return Err(AppError::conflict(
            "STALE_EXTENSION_VERSION",
            "active manifest changed",
        ));
    }
    if bundle_digest(bundle)? != sha256 {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "portable bundle digest mismatch",
        ));
    }

    let prior = sqlx::query(
        "SELECT id, mappings FROM extensions.import_receipts
         WHERE organization_id = $1 AND project_id = $2 AND extension_key = $3
           AND bundle_sha256 = $4",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(extension_key)
    .bind(sha256)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if let Some(row) = prior {
        let id: Uuid = row.try_get("id").map_err(db_error)?;
        let mappings: Value = row.try_get("mappings").map_err(db_error)?;
        return Ok(json!({
            "id": id.to_string(),
            "sha256": sha256,
            "mappings": mappings,
            "replayed": true,
        }));
    }

    let invalid = || AppError::validation("VALIDATION_ERROR", "invalid portable bundle");
    let b_obj = bundle.as_object().ok_or_else(invalid)?;
    let required_bundle_keys = [
        "format",
        "extension_key",
        "origin",
        "active_manifest_id",
        "activation_status",
        "manifests",
        "activation_history",
        "records",
    ];
    if b_obj.len() != required_bundle_keys.len()
        || required_bundle_keys.iter().any(|k| !b_obj.contains_key(*k))
        || b_obj["format"].as_str() != Some(PORTABLE_FORMAT)
        || b_obj["extension_key"].as_str() != Some(extension_key)
    {
        return Err(invalid());
    }
    let origin = b_obj["origin"].as_object().ok_or_else(invalid)?;
    if origin.len() != 2
        || !origin.contains_key("organization_id")
        || !origin.contains_key("project_id")
    {
        return Err(invalid());
    }
    let source_org = origin["organization_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(invalid)?;
    let source_project = origin["project_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(invalid)?;
    let manifests = b_obj["manifests"].as_array().ok_or_else(invalid)?;
    let records = b_obj["records"].as_array().ok_or_else(invalid)?;
    if records.is_empty() || records.len() > MAX_RECORDS {
        return Err(invalid());
    }

    let active_mid_str = b_obj["active_manifest_id"].as_str().ok_or_else(invalid)?;
    let matching_manifests: Vec<&Map<String, Value>> = manifests
        .iter()
        .filter_map(Value::as_object)
        .filter(|m| m.get("manifest_id").and_then(Value::as_str) == Some(active_mid_str))
        .collect();
    if matching_manifests.len() != 1 {
        return Err(AppError::conflict(
            "EXTENSION_IMPORT_INCOMPATIBLE",
            "portable bundle requires exact destination manifest contract",
        ));
    }
    let dest_contract = manifest.portable_contract();
    let dest_obj = dest_contract.as_object().expect("object");
    let src_active = matching_manifests[0];
    if dest_obj
        .iter()
        .any(|(k, v)| src_active.get(k).is_none_or(|sv| !semantic_eq(sv, v)))
    {
        return Err(AppError::conflict(
            "EXTENSION_IMPORT_INCOMPATIBLE",
            "portable bundle requires exact destination manifest contract",
        ));
    }

    let mut items = Vec::with_capacity(records.len());
    let mut seen = HashSet::new();
    let required_item_keys = [
        "record_id",
        "revision",
        "manifest_id",
        "entity_name",
        "payload",
        "linked_layer_id",
        "linked_feature_id",
        "link_context",
        "history",
    ];
    for item in records {
        let item_obj = item.as_object().ok_or_else(invalid)?;
        if item_obj.len() != required_item_keys.len()
            || required_item_keys
                .iter()
                .any(|k| !item_obj.contains_key(*k))
        {
            return Err(invalid());
        }
        let rid = item_obj["record_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(invalid)?;
        let rev_i64 = item_obj["revision"].as_i64().ok_or_else(invalid)?;
        let revision = i32::try_from(rev_i64).map_err(|_| invalid())?;
        if revision < 1 || !seen.insert(rid) {
            return Err(invalid());
        }
        let entity_name = item_obj["entity_name"].as_str().ok_or_else(invalid)?;
        if !item_obj["payload"].is_object() {
            return Err(invalid());
        }
        let history = item_obj["history"].as_array().ok_or_else(invalid)?;
        let Some(last) = history.last().and_then(Value::as_object) else {
            return Err(invalid());
        };
        if last.get("revision").and_then(Value::as_i64) != Some(rev_i64) {
            return Err(invalid());
        }
        let item_proj = json!({
            "revision": item_obj["revision"],
            "manifest_id": item_obj["manifest_id"],
            "entity_name": item_obj["entity_name"],
            "payload": item_obj["payload"],
            "linked_layer_id": item_obj["linked_layer_id"],
            "linked_feature_id": item_obj["linked_feature_id"],
            "link_context": item_obj["link_context"],
        });
        let last_proj = json!({
            "revision": last.get("revision").cloned().unwrap_or(Value::Null),
            "manifest_id": last.get("manifest_id").cloned().unwrap_or(Value::Null),
            "entity_name": last.get("entity_name").cloned().unwrap_or(Value::Null),
            "payload": last.get("payload").cloned().unwrap_or(Value::Null),
            "linked_layer_id": last.get("linked_layer_id").cloned().unwrap_or(Value::Null),
            "linked_feature_id": last.get("linked_feature_id").cloned().unwrap_or(Value::Null),
            "link_context": last.get("link_context").cloned().unwrap_or(Value::Null),
        });
        if !semantic_eq(&item_proj, &last_proj) {
            return Err(invalid());
        }

        let lid = match &item_obj["linked_layer_id"] {
            Value::Null => None,
            Value::String(s) => Some(Uuid::parse_str(s).map_err(|_| invalid())?),
            _ => return Err(invalid()),
        };
        let fid = match &item_obj["linked_feature_id"] {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return Err(invalid()),
        };
        if lid.is_some()
            && (source_org != edit.organization_id() || source_project != edit.project_id())
        {
            return Err(AppError::conflict(
                "EXTENSION_IMPORT_LINK_REMAP_REQUIRED",
                "portable link remapping is not supported across scopes",
            ));
        }
        let expected_link = if item_obj["link_context"].is_null() {
            None
        } else {
            Some(&item_obj["link_context"])
        };
        let link = assert_link(edit, lid, fid.as_deref(), expected_link).await?;
        let cleaned_payload = validate_record_against_schema(
            entity_name,
            &item_obj["payload"],
            &manifest.entity_schemas,
        )?;
        items.push(ParsedImportItem {
            source_record_id: rid,
            revision,
            entity_name: entity_name.to_owned(),
            payload: cleaned_payload,
            linked_layer_id: lid,
            linked_feature_id: fid,
            link_context: link,
        });
    }

    let mut mappings = Vec::with_capacity(items.len());
    for item in items {
        let origin_meta = json!({
            "claimed_source_organization_id": source_org.to_string(),
            "claimed_source_project_id": source_project.to_string(),
            "claimed_source_record_id": item.source_record_id.to_string(),
            "claimed_source_revision": item.revision,
            "bundle_sha256": sha256,
            "verified_origin": false,
        });
        let made = insert_record(
            edit,
            actor_id,
            actor_display,
            &manifest,
            &item.entity_name,
            &item.payload,
            item.linked_layer_id,
            item.linked_feature_id.as_deref(),
            item.link_context.as_ref(),
            Some(&origin_meta),
            None,
        )
        .await?;
        mappings.push(json!({
            "source_record_id": item.source_record_id.to_string(),
            "record_id": made["id"],
        }));
    }

    let receipt_id = Uuid::new_v4();
    let now = Utc::now();
    let mappings_val = Value::Array(mappings.clone());
    sqlx::query(
        "INSERT INTO extensions.import_receipts (
            id, organization_id, project_id, extension_key, manifest_id,
            bundle_sha256, mappings, actor_id, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(receipt_id)
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(extension_key)
    .bind(manifest.id)
    .bind(sha256)
    .bind(&mappings_val)
    .bind(actor_id)
    .bind(now)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    append_project_audit(
        edit,
        actor_id,
        actor_display,
        "extension.import",
        "extension_import",
        receipt_id,
        json!({ "bundle_sha256": sha256, "record_count": mappings.len() }),
        now,
    )
    .await?;

    Ok(json!({
        "id": receipt_id.to_string(),
        "sha256": sha256,
        "mappings": mappings_val,
        "replayed": false,
    }))
}
