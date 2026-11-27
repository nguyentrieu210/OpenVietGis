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

#[derive(Clone, Copy)]
pub enum LinkedFilter<'a> {
    ByFeature {
        source_id: Uuid,
        feature_id: &'a str,
    },
    ByDocument {
        document_id: Uuid,
        version: i32,
        handle: &'a str,
    },
}

pub async fn linked_features(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    project_id: Uuid,
    filter: LinkedFilter<'_>,
) -> Result<Vec<Value>, AppError> {
    let (filter_sql, src_id, fid, doc_id, ver, handle) = match filter {
        LinkedFilter::ByFeature {
            source_id,
            feature_id,
        } => (
            "AND c.source_id = $3 AND f.feature_id = $4",
            Some(source_id),
            Some(feature_id),
            None,
            None,
            None,
        ),
        LinkedFilter::ByDocument {
            document_id,
            version,
            handle,
        } => (
            "AND d.document_id = $5 AND d.version = $6 AND upper(m.item->>'handle') = $7",
            None,
            None,
            Some(document_id),
            Some(version),
            Some(handle),
        ),
    };
    let sql = format!(
        "SELECT d.document_id, d.version, d.filename, c.source_id, c.layer_id,
                f.feature_id, m.item->>'handle' AS handle, l.name AS layer_name,
                (c.content_revision = s.content_revision) AS original_revision,
                c.id AS conversion_id, s.content_revision, s.schema_revision
         FROM cad.conversions c
         JOIN cad.document_versions d
           ON d.organization_id = c.organization_id
          AND d.project_id = c.project_id
          AND d.document_id::text = c.entity_summary->>'cad_document_id'
          AND d.version::text = c.entity_summary->>'cad_document_version'
          AND d.sha256 = c.sha256
         JOIN spatial.layers l
           ON l.organization_id = c.organization_id
          AND l.project_id = c.project_id
          AND l.id = c.layer_id
          AND l.source_id = c.source_id
          AND l.lifecycle = 'active'
         JOIN spatial.data_sources s
           ON s.organization_id = c.organization_id
          AND s.project_id = c.project_id
          AND s.id = c.source_id
          AND s.lifecycle = 'active'
          AND s.validation_state = 'ready'
         CROSS JOIN LATERAL jsonb_array_elements(c.entity_summary->'entity_mapping') m(item)
         JOIN spatial.reference_features f
           ON f.organization_id = c.organization_id
          AND f.project_id = c.project_id
          AND f.source_id = c.source_id
          AND f.feature_id = m.item->>'feature_id'
         WHERE c.organization_id = $1 AND c.project_id = $2
           AND c.direction = 'import' AND c.status = 'succeeded'
           AND m.item->>'disposition' IN ('mapped', 'approximated')
           {filter_sql}
         ORDER BY c.created_at, c.id
         LIMIT 101"
    );
    let rows = sqlx::query(&sql)
        .bind(organization_id)
        .bind(project_id)
        .bind(src_id)
        .bind(fid)
        .bind(doc_id)
        .bind(ver)
        .bind(handle)
        .fetch_all(&mut **tx)
        .await
        .map_err(db_error)?;
    if rows.len() > 100 {
        return Err(AppError::conflict(
            "TOO_MANY_LINKED_CONVERSIONS",
            "Too many linked conversions; narrow selection",
        ));
    }
    rows.into_iter()
        .map(|row| {
            Ok(json!({
                "document_id": row.try_get::<Uuid, _>("document_id").map_err(db_error)?.to_string(),
                "version": row.try_get::<i32, _>("version").map_err(db_error)?,
                "filename": row.try_get::<String, _>("filename").map_err(db_error)?,
                "source_id": row.try_get::<Uuid, _>("source_id").map_err(db_error)?.to_string(),
                "layer_id": row.try_get::<Uuid, _>("layer_id").map_err(db_error)?.to_string(),
                "feature_id": row.try_get::<String, _>("feature_id").map_err(db_error)?,
                "handle": row.try_get::<Option<String>, _>("handle").map_err(db_error)?,
                "layer_name": row.try_get::<String, _>("layer_name").map_err(db_error)?,
                "original_revision": row.try_get::<bool, _>("original_revision").map_err(db_error)?,
                "conversion_id": row.try_get::<Uuid, _>("conversion_id").map_err(db_error)?.to_string(),
                "content_revision": row.try_get::<String, _>("content_revision").map_err(db_error)?,
                "schema_revision": row.try_get::<String, _>("schema_revision").map_err(db_error)?,
            }))
        })
        .collect()
}

pub async fn list_conversions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id, direction, cad_format, filename, units, crs_code, status,
                entity_summary, loss_report, sha256, byte_size::bigint AS byte_size,
                source_id, layer_id, content_revision, created_at
         FROM cad.conversions
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(organization_id)
    .bind(project_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(db_error)?;
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<Uuid, _>("id").map_err(db_error)?.to_string(),
                "direction": row.try_get::<String, _>("direction").map_err(db_error)?,
                "cad_format": row.try_get::<String, _>("cad_format").map_err(db_error)?,
                "filename": row.try_get::<String, _>("filename").map_err(db_error)?,
                "units": row.try_get::<String, _>("units").map_err(db_error)?,
                "crs_code": row.try_get::<i32, _>("crs_code").map_err(db_error)?,
                "status": row.try_get::<String, _>("status").map_err(db_error)?,
                "entity_summary": row.try_get::<Value, _>("entity_summary").map_err(db_error)?,
                "loss_report": row.try_get::<Value, _>("loss_report").map_err(db_error)?,
                "sha256": row.try_get::<String, _>("sha256").map_err(db_error)?,
                "byte_size": row.try_get::<i64, _>("byte_size").map_err(db_error)?,
                "source_id": row.try_get::<Option<Uuid>, _>("source_id").map_err(db_error)?.map(|u| u.to_string()),
                "layer_id": row.try_get::<Option<Uuid>, _>("layer_id").map_err(db_error)?.map(|u| u.to_string()),
                "content_revision": row.try_get::<Option<String>, _>("content_revision").map_err(db_error)?,
                "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
            }))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(json!({ "items": items }))
}

#[derive(Debug)]
pub enum CadDocError {
    Detail(u16, &'static str),
    Internal(AppError),
}

impl From<AppError> for CadDocError {
    fn from(value: AppError) -> Self {
        Self::Internal(value)
    }
}

pub async fn prepare_cad_save(
    edit: &mut ProjectRead<'_>,
    doc_id: Uuid,
    expected: i32,
    payload_len: usize,
    fmt: &str,
) -> Result<(), CadDocError> {
    let lock_key = format!("cad-doc:{}", edit.project_id());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;

    let current: Option<i32> = sqlx::query_scalar(
        "SELECT max(version) FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2 AND document_id = $3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(doc_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    if expected == 0 {
        if current.is_some() {
            return Err(CadDocError::Detail(409, "Document already exists"));
        }
    } else if let Some(cur) = current {
        if cur != expected {
            return Err(CadDocError::Detail(409, "CAD document changed; reload"));
        }
    } else {
        return Err(CadDocError::Detail(404, "CAD document not found"));
    }

    if expected >= 20 {
        return Err(CadDocError::Detail(413, "CAD revision limit"));
    }

    let total: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(byte_size), 0)::bigint FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    if total + payload_len as i64 > 500 * 1024 * 1024 {
        return Err(CadDocError::Detail(413, "Project CAD quota exceeded"));
    }

    if expected == 0 {
        let doc_count: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT document_id)::bigint FROM cad.document_versions
             WHERE organization_id = $1 AND project_id = $2",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if doc_count >= 100 {
            return Err(CadDocError::Detail(413, "Project CAD document limit"));
        }
    } else {
        let prior_fmt: String = sqlx::query_scalar(
            "SELECT format FROM cad.document_versions
             WHERE organization_id = $1 AND project_id = $2
               AND document_id = $3 AND version = $4",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(doc_id)
        .bind(expected)
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if prior_fmt != fmt {
            return Err(CadDocError::Detail(422, "CAD revision format cannot change"));
        }
    }
    Ok(())
}

pub struct SaveCadVersionArgs<'a> {
    pub actor_id: Uuid,
    pub actor_display: &'a str,
    pub doc_id: Uuid,
    pub version: i32,
    pub filename: &'a str,
    pub format: &'a str,
    pub storage_key: &'a str,
    pub sha256: &'a str,
    pub byte_size: i64,
}

pub async fn insert_cad_version(
    edit: &mut ProjectRead<'_>,
    args: SaveCadVersionArgs<'_>,
) -> Result<Value, CadDocError> {
    let row = sqlx::query(
        "INSERT INTO cad.document_versions
         (organization_id, project_id, document_id, version, filename, format,
          storage_key, sha256, byte_size, actor_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING document_id, version, filename, format, sha256, byte_size, created_at",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(args.doc_id)
    .bind(args.version)
    .bind(args.filename)
    .bind(args.format)
    .bind(args.storage_key)
    .bind(args.sha256)
    .bind(i32::try_from(args.byte_size).map_err(|_| db_error_msg("size overflow"))?)
    .bind(args.actor_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, 'human', $3, $4, 'cad.document.save', 'cad_document', $5, 'success', $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(args.actor_id)
    .bind(args.actor_display)
    .bind(args.doc_id)
    .bind(json!({ "version": args.version, "sha256": args.sha256, "byte_size": args.byte_size }))
    .bind(Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    wire(&row).map_err(CadDocError::Internal)
}

fn db_error_msg(msg: &str) -> AppError {
    AppError::internal(msg)
}

pub async fn get_download_version(
    read: &mut ProjectRead<'_>,
    document_id: Uuid,
    version: i32,
) -> Result<Option<(String, String, String)>, AppError> {
    let row = sqlx::query(
        "SELECT filename, storage_key, sha256
         FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2
           AND document_id = $3 AND version = $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(document_id)
    .bind(version)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some((
        row.try_get("filename").map_err(db_error)?,
        row.try_get("storage_key").map_err(db_error)?,
        row.try_get("sha256").map_err(db_error)?,
    )))
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
