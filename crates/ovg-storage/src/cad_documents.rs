//! E10 CAD document read-side repository.

use std::fmt::Write as _;

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::editing::{EditError, EditScope};

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
            return Err(CadDocError::Detail(
                422,
                "CAD revision format cannot change",
            ));
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

fn edit_cad_db_err(error: sqlx::Error) -> EditError {
    tracing::error!(%error, "CAD storage query failed");
    EditError::Internal("CAD storage query failed".to_owned())
}

fn sha256_hex_str(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn conversion_wire(row: &PgRow) -> Result<Value, EditError> {
    let id: Uuid = row.try_get("id").map_err(edit_cad_db_err)?;
    let source_id: Option<Uuid> = row.try_get("source_id").map_err(edit_cad_db_err)?;
    let layer_id: Option<Uuid> = row.try_get("layer_id").map_err(edit_cad_db_err)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(edit_cad_db_err)?;
    Ok(json!({
        "id": id.to_string(),
        "direction": row.try_get::<String, _>("direction").map_err(edit_cad_db_err)?,
        "cad_format": row.try_get::<String, _>("cad_format").map_err(edit_cad_db_err)?,
        "filename": row.try_get::<String, _>("filename").map_err(edit_cad_db_err)?,
        "units": row.try_get::<String, _>("units").map_err(edit_cad_db_err)?,
        "crs_code": row.try_get::<i32, _>("crs_code").map_err(edit_cad_db_err)?,
        "status": row.try_get::<String, _>("status").map_err(edit_cad_db_err)?,
        "entity_summary": row.try_get::<Value, _>("entity_summary").map_err(edit_cad_db_err)?,
        "loss_report": row.try_get::<Value, _>("loss_report").map_err(edit_cad_db_err)?,
        "sha256": row.try_get::<String, _>("sha256").map_err(edit_cad_db_err)?,
        "byte_size": row.try_get::<i64, _>("byte_size").map_err(edit_cad_db_err)?,
        "source_id": source_id.map(|u| u.to_string()),
        "layer_id": layer_id.map(|u| u.to_string()),
        "content_revision": row.try_get::<Option<String>, _>("content_revision").map_err(edit_cad_db_err)?,
        "created_at": iso(created_at),
    }))
}

pub async fn validate_wkts_postgis(
    scope: &mut EditScope<'_>,
    wkts: &[String],
) -> Result<(), EditError> {
    let json_wkts =
        serde_json::to_string(wkts).map_err(|err| EditError::Internal(err.to_string()))?;
    let invalid: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM jsonb_array_elements_text($1::jsonb) AS a(wkt)
         WHERE NOT ST_IsValid(ST_GeomFromText(a.wkt)) OR ST_IsEmpty(ST_GeomFromText(a.wkt))",
    )
    .bind(json_wkts)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;
    if invalid > 0 {
        return Err(EditError::validation(
            "INVALID_GEOMETRY",
            "invalid DXF geometry; publication rejected",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_rejected_dwg_conversion(
    scope: &mut EditScope<'_>,
    conv_id: Uuid,
    filename: &str,
    units: &str,
    crs_code: i32,
    storage_key: &str,
    sha256: &str,
    byte_size: i64,
) -> Result<Uuid, EditError> {
    let fname: String = filename.chars().take(255).collect();
    let loss = json!({
        "reason": "DWG_ADAPTER_REQUIRED",
        "message": "Native DWG requires a separately licensed adapter; convert to 2D DXF R2010+ for direct import.",
    });
    let row = sqlx::query(
        "INSERT INTO cad.conversions (
             id, organization_id, project_id, direction, cad_format, filename,
             units, crs_code, status, entity_summary, loss_report, storage_key,
             sha256, byte_size, actor_id, created_at
         ) VALUES (
             $1, $2, $3, 'import', 'dwg', $4,
             $5, $6, 'rejected', $7, $8,
             $9, $10, $11, $12, $13
         )
         RETURNING id",
    )
    .bind(conv_id)
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(fname)
    .bind(units)
    .bind(crs_code)
    .bind(json!({ "detected_format": "dwg" }))
    .bind(loss)
    .bind(storage_key)
    .bind(sha256)
    .bind(byte_size)
    .bind(scope.actor_id())
    .bind(Utc::now())
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;
    row.try_get("id").map_err(edit_cad_db_err)
}

pub async fn find_existing_georef_conversion(
    scope: &mut EditScope<'_>,
    inspection_fingerprint: &str,
) -> Result<Option<Value>, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let key = format!("cad-georef:{org_id}:{project_id}:{inspection_fingerprint}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(key)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_cad_db_err)?;

    let row = sqlx::query(
        "SELECT id, direction, cad_format, filename, units, crs_code, status,
                entity_summary, loss_report, sha256, byte_size::bigint AS byte_size,
                source_id, layer_id, content_revision, created_at
         FROM cad.conversions
         WHERE organization_id = $1 AND project_id = $2
           AND direction = 'import' AND status = 'succeeded'
           AND entity_summary->>'inspection_fingerprint' = $3
         ORDER BY created_at LIMIT 1",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(inspection_fingerprint)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    row.as_ref().map(conversion_wire).transpose()
}

pub struct PublishCadImportArgs<'a> {
    pub conv_id: Uuid,
    pub filename: &'a str,
    pub layer_name: &'a str,
    pub units: &'a str,
    pub crs_code: i32,
    pub family: &'a str,
    pub sha256: &'a str,
    pub byte_size: i64,
    pub storage_key: &'a str,
    pub schema_rev: &'a str,
    pub inspection_fingerprint: &'a str,
    pub entity_summary: Value,
    pub loss_report: Value,
    pub features: &'a [Value],
    pub doc_placement: Option<(Uuid, i32)>,
}

pub async fn publish_cad_import(
    scope: &mut EditScope<'_>,
    args: PublishCadImportArgs<'_>,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_kind = scope.actor_kind().to_owned();
    let actor_display = scope.display_name().to_owned();

    let out_source_id = Uuid::new_v4();
    let out_layer_id = Uuid::new_v4();
    let clean_name = {
        let trimmed: String = args.layer_name.trim().chars().take(200).collect();
        if trimmed.is_empty() {
            "CAD DXF Layer".to_owned()
        } else {
            trimmed
        }
    };
    let content_rev = sha256_hex_str(&format!("{out_source_id}:{}", args.sha256));
    let ref_key = format!("cad_dxf_{}", &out_source_id.simple().to_string()[..12]);
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO spatial.data_sources (
             id, organization_id, project_id, name, kind, reference_key,
             fixture_version, manifest_digest, source_asset_id, lifecycle,
             validation_state, validation_reason, source_crs, stored_crs,
             schema_state, geometry_family, dimensions, feature_count_state,
             feature_count, extent_state, extent, catalog_revision,
             content_revision, schema_revision, created_at, updated_at, retired_at
         ) VALUES (
             $1, $2, $3, $4, 'derived_processing', $5,
             'cad_dxf_2d', $6, NULL, 'active',
             'ready', NULL, $7, $7,
             'known', $8, 'XY', 'known',
             0, 'empty', NULL, 1,
             $9, $10, $11, $11, NULL
         )",
    )
    .bind(out_source_id)
    .bind(org_id)
    .bind(project_id)
    .bind(&clean_name)
    .bind(ref_key)
    .bind(args.sha256)
    .bind(args.crs_code)
    .bind(args.family)
    .bind(&content_rev)
    .bind(args.schema_rev)
    .bind(now)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    for (ordinal, (fid, fname)) in [("cad_layer", "CAD Layer"), ("cad_entity", "CAD Entity")]
        .into_iter()
        .enumerate()
    {
        sqlx::query(
            "INSERT INTO spatial.source_fields (
                 organization_id, project_id, source_id, field_id,
                 name, ordinal, type, nullable, schema_revision
             ) VALUES ($1, $2, $3, $4, $5, $6, 'text', false, $7)",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(out_source_id)
        .bind(fid)
        .bind(fname)
        .bind(ordinal as i32)
        .bind(args.schema_rev)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_cad_db_err)?;
    }

    for feat in args.features {
        let fid = feat["feature_id"].as_str().unwrap_or_default();
        let attrs = &feat["attributes"];
        let wkt = feat["wkt"].as_str().unwrap_or_default();
        sqlx::query(
            "INSERT INTO spatial.reference_features (
                 organization_id, project_id, source_id, feature_id,
                 attributes, geometry, geometry_state, content_revision, schema_revision
             ) VALUES (
                 $1, $2, $3, $4,
                 $5,
                 ST_SetSRID(ST_GeomFromText($6), $7),
                 'present', $8, $9
             )",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(out_source_id)
        .bind(fid)
        .bind(attrs)
        .bind(wkt)
        .bind(args.crs_code)
        .bind(&content_rev)
        .bind(args.schema_rev)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_cad_db_err)?;
    }

    let invalid: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
           AND (ST_IsEmpty(geometry) OR NOT ST_IsValid(geometry))",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(out_source_id)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;
    if invalid > 0 {
        return Err(EditError::validation(
            "INVALID_GEOMETRY",
            "invalid DXF geometry; publication rejected",
        ));
    }

    let stats = sqlx::query(
        "SELECT COUNT(*)::int AS cnt,
                ST_XMin(ST_Extent(geometry)) AS min_x,
                ST_YMin(ST_Extent(geometry)) AS min_y,
                ST_XMax(ST_Extent(geometry)) AS max_x,
                ST_YMax(ST_Extent(geometry)) AS max_y
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(out_source_id)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    let cnt: i32 = stats.try_get("cnt").map_err(edit_cad_db_err)?;
    let min_x: f64 = stats.try_get("min_x").map_err(edit_cad_db_err)?;
    let min_y: f64 = stats.try_get("min_y").map_err(edit_cad_db_err)?;
    let max_x: f64 = stats.try_get("max_x").map_err(edit_cad_db_err)?;
    let max_y: f64 = stats.try_get("max_y").map_err(edit_cad_db_err)?;

    sqlx::query(
        "UPDATE spatial.data_sources
         SET feature_count = $4,
             extent_state = 'available',
             extent = $5
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(out_source_id)
    .bind(cnt)
    .bind(json!({
        "min_x": min_x,
        "min_y": min_y,
        "max_x": max_x,
        "max_y": max_y,
        "crs_code": args.crs_code,
    }))
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    sqlx::query(
        "INSERT INTO spatial.layers (
             id, organization_id, project_id, source_id, name,
             lifecycle, layer_revision, created_at, updated_at, retired_at
         ) VALUES ($1, $2, $3, $4, $5, 'active', 1, $6, $6, NULL)",
    )
    .bind(out_layer_id)
    .bind(org_id)
    .bind(project_id)
    .bind(out_source_id)
    .bind(&clean_name)
    .bind(now)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    let fname_trunc: String = args.filename.chars().take(255).collect();
    let mut audit_meta = json!({
        "filename": fname_trunc,
        "crs_code": args.crs_code,
        "units": args.units,
    });
    if let (Some((doc_id, ver)), Some(obj)) = (args.doc_placement, audit_meta.as_object_mut()) {
        obj.insert(
            "cad_document_id".to_owned(),
            Value::String(doc_id.to_string()),
        );
        obj.insert("cad_document_version".to_owned(), Value::from(ver));
        obj.insert(
            "inspection_fingerprint".to_owned(),
            Value::String(args.inspection_fingerprint.to_owned()),
        );
    }

    sqlx::query(
        "INSERT INTO audit.audit_events (
             id, organization_id, actor_type, actor_id, actor_display_snapshot,
             action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, $3, $4, $5, 'cad.dxf.import', 'data_source', $6, 'success', $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(&actor_kind)
    .bind(actor_id)
    .bind(&actor_display)
    .bind(out_source_id)
    .bind(audit_meta)
    .bind(now)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    let conv_row = sqlx::query(
        "INSERT INTO cad.conversions (
             id, organization_id, project_id, direction, cad_format, filename,
             units, crs_code, status, entity_summary, loss_report, storage_key,
             sha256, byte_size, source_id, layer_id, content_revision,
             actor_id, created_at
         ) VALUES (
             $1, $2, $3, 'import', 'dxf', $4,
             $5, $6, 'succeeded', $7, $8,
             $9, $10, $11, $12, $13, $14, $15, $16
         )
         RETURNING id, direction, cad_format, filename, units, crs_code, status,
                   entity_summary, loss_report, sha256, byte_size::bigint AS byte_size,
                   source_id, layer_id, content_revision, created_at",
    )
    .bind(args.conv_id)
    .bind(org_id)
    .bind(project_id)
    .bind(fname_trunc)
    .bind(args.units)
    .bind(args.crs_code)
    .bind(args.entity_summary)
    .bind(args.loss_report)
    .bind(args.storage_key)
    .bind(args.sha256)
    .bind(args.byte_size)
    .bind(out_source_id)
    .bind(out_layer_id)
    .bind(&content_rev)
    .bind(actor_id)
    .bind(now)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    conversion_wire(&conv_row)
}

pub struct DxfExportSource {
    pub layer_name: String,
    pub source_id: Uuid,
    pub stored_crs: i32,
    pub content_revision: String,
    pub features: Vec<Value>,
}

pub async fn load_dxf_export_source(
    scope: &mut EditScope<'_>,
    layer_id: Uuid,
    expected_content_revision: &str,
    acknowledge_attribute_loss: bool,
) -> Result<DxfExportSource, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let meta = sqlx::query(
        "SELECT l.id AS layer_id, l.name AS layer_name,
                s.id AS source_id, s.stored_crs, s.content_revision, s.dimensions
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
           AND s.validation_state = 'ready'
         FOR SHARE OF s, l",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(layer_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?
    .ok_or_else(|| EditError::not_found("layer not found"))?;

    let stored_crs: i32 = meta.try_get("stored_crs").map_err(edit_cad_db_err)?;
    if !matches!(stored_crs, 3857 | 5899) {
        return Err(EditError::validation(
            "CAD_PROJECTED_CRS_REQUIRED",
            "DXF export requires a metric projected source; no implicit degree-to-meter conversion",
        ));
    }
    let content_revision: String = meta.try_get("content_revision").map_err(edit_cad_db_err)?;
    if expected_content_revision != content_revision {
        return Err(EditError::conflict(
            "STALE_CAD_REVISION",
            "requested CAD source revision changed",
        ));
    }
    let dimensions: String = meta.try_get("dimensions").map_err(edit_cad_db_err)?;
    if dimensions != "XY" {
        return Err(EditError::validation(
            "CAD_3D_UNSUPPORTED",
            "DXF export requires XY geometry",
        ));
    }
    if !acknowledge_attribute_loss {
        return Err(EditError::validation(
            "CAD_LOSS_ACK_REQUIRED",
            "DXF export omits source attributes and original curve semantics; explicit acknowledgement required",
        ));
    }

    let source_id: Uuid = meta.try_get("source_id").map_err(edit_cad_db_err)?;
    let layer_name: String = meta.try_get("layer_name").map_err(edit_cad_db_err)?;
    let frows = sqlx::query(
        "SELECT feature_id, ST_AsGeoJSON(geometry, 15, 0)::jsonb AS geom
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
         ORDER BY feature_id ASC LIMIT 10001",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    if frows.len() > 10_000 {
        return Err(EditError::validation(
            "CAD_FEATURE_LIMIT",
            "DXF source exceeds 10000 features",
        ));
    }
    let mut features = Vec::with_capacity(frows.len());
    for row in frows {
        let fid: String = row.try_get("feature_id").map_err(edit_cad_db_err)?;
        let geom: Option<Value> = row.try_get("geom").map_err(edit_cad_db_err)?;
        let Some(geom) = geom else {
            return Err(EditError::validation(
                "CAD_GEOMETRY_UNSUPPORTED",
                "DXF cannot preserve null geometry",
            ));
        };
        features.push(json!({ "feature_id": fid, "geometry": geom }));
    }

    Ok(DxfExportSource {
        layer_name,
        source_id,
        stored_crs,
        content_revision,
        features,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_dxf_export_conversion(
    scope: &mut EditScope<'_>,
    conv_id: Uuid,
    fname: &str,
    units: &str,
    crs_code: i32,
    storage_key: &str,
    sha256: &str,
    byte_size: i64,
    source_id: Uuid,
    layer_id: Uuid,
    content_revision: &str,
    feature_count: usize,
) -> Result<(), EditError> {
    sqlx::query(
        "INSERT INTO cad.conversions (
             id, organization_id, project_id, direction, cad_format, filename,
             units, crs_code, status, entity_summary, loss_report, storage_key,
             sha256, byte_size, source_id, layer_id, content_revision,
             actor_id, created_at
         ) VALUES (
             $1, $2, $3, 'export', 'dxf', $4,
             $5, $6, 'succeeded', $7, $8,
             $9, $10, $11, $12, $13, $14, $15, $16
         )",
    )
    .bind(conv_id)
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(fname)
    .bind(units)
    .bind(crs_code)
    .bind(json!({ "exported_features": feature_count, "dxf_version": "R2010" }))
    .bind(json!({
        "attributes_omitted_in_dxf_geometry": true,
        "original_curve_semantics_not_preserved": true,
        "acknowledged": true,
        "silent_loss": false,
    }))
    .bind(storage_key)
    .bind(sha256)
    .bind(byte_size)
    .bind(source_id)
    .bind(layer_id)
    .bind(content_revision)
    .bind(scope.actor_id())
    .bind(Utc::now())
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;
    Ok(())
}

pub async fn load_cad_document_version_blob(
    scope: &mut EditScope<'_>,
    document_id: Uuid,
    version: i32,
) -> Result<(String, String, String, String), EditError> {
    let row = sqlx::query(
        "SELECT filename, format, storage_key, sha256
         FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2
           AND document_id = $3 AND version = $4",
    )
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(document_id)
    .bind(version)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?
    .ok_or_else(|| EditError::detail(404, "CAD version not found"))?;

    let fmt: String = row.try_get("format").map_err(edit_cad_db_err)?;
    if fmt != "dxf" {
        return Err(EditError::detail(
            422,
            "Control-point import supports 2D DXF only",
        ));
    }
    Ok((
        row.try_get("filename").map_err(edit_cad_db_err)?,
        fmt,
        row.try_get("storage_key").map_err(edit_cad_db_err)?,
        row.try_get("sha256").map_err(edit_cad_db_err)?,
    ))
}

pub struct GeorefDraftContext {
    pub sha256: String,
    pub storage_key: String,
    pub entity_summary: Value,
    pub source_id: Uuid,
    pub layer_id: Uuid,
    pub content_revision: String,
    pub schema_revision: String,
    pub layer_name: String,
    pub features: Vec<Value>,
}

pub async fn load_georef_draft_context(
    scope: &mut EditScope<'_>,
    document_id: Uuid,
    version: i32,
    conversion_id: Uuid,
    expected_doc_sha: &str,
    expected_content_rev: &str,
    expected_schema_rev: &str,
) -> Result<GeorefDraftContext, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended($1, 0))")
        .bind(format!("cad-doc:{project_id}"))
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_cad_db_err)?;

    let row = sqlx::query(
        "SELECT d.sha256, d.storage_key, d.format, c.entity_summary,
                c.source_id, c.layer_id, s.content_revision, s.schema_revision,
                s.stored_crs, s.dimensions, s.feature_count, l.name AS layer_name
         FROM cad.conversions c
         JOIN cad.document_versions d ON d.organization_id = c.organization_id
           AND d.project_id = c.project_id AND d.document_id = $3
           AND d.version = $4 AND d.sha256 = c.sha256
           AND d.document_id::text = c.entity_summary->>'cad_document_id'
           AND d.version::text = c.entity_summary->>'cad_document_version'
         JOIN spatial.data_sources s ON s.organization_id = c.organization_id
           AND s.project_id = c.project_id AND s.id = c.source_id
         JOIN spatial.layers l ON l.organization_id = c.organization_id
           AND l.project_id = c.project_id AND l.id = c.layer_id AND l.source_id = s.id
         WHERE c.organization_id = $1 AND c.project_id = $2 AND c.id = $5
           AND c.direction = 'import' AND c.status = 'succeeded'
           AND s.lifecycle = 'active' AND s.validation_state = 'ready' AND l.lifecycle = 'active'
         FOR SHARE OF s, l",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(document_id)
    .bind(version)
    .bind(conversion_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?
    .ok_or_else(|| EditError::detail(404, "Version-pinned CAD conversion not found"))?;

    let latest: Option<i32> = sqlx::query_scalar(
        "SELECT max(version) FROM cad.document_versions
         WHERE organization_id = $1 AND project_id = $2 AND document_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(document_id)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    let sha256: String = row.try_get("sha256").map_err(edit_cad_db_err)?;
    let content_revision: String = row.try_get("content_revision").map_err(edit_cad_db_err)?;
    let schema_revision: String = row.try_get("schema_revision").map_err(edit_cad_db_err)?;
    if latest != Some(version)
        || sha256 != expected_doc_sha
        || content_revision != expected_content_rev
        || schema_revision != expected_schema_rev
    {
        return Err(EditError::detail(409, "CAD or GIS source revision changed"));
    }

    let fmt: String = row.try_get("format").map_err(edit_cad_db_err)?;
    let stored_crs: i32 = row.try_get("stored_crs").map_err(edit_cad_db_err)?;
    let dimensions: String = row.try_get("dimensions").map_err(edit_cad_db_err)?;
    if fmt != "dxf" || stored_crs != 3857 || dimensions != "XY" {
        return Err(EditError::detail(
            422,
            "Only calibrated local XY DXF supported",
        ));
    }

    let feature_count: i32 = row.try_get("feature_count").map_err(edit_cad_db_err)?;
    if !(1..=10_000).contains(&feature_count) {
        return Err(EditError::detail(422, "Draft requires 1–10000 features"));
    }

    let source_id: Uuid = row.try_get("source_id").map_err(edit_cad_db_err)?;
    let counts = sqlx::query(
        "SELECT count(*)::int AS n, coalesce(sum(ST_NPoints(geometry)), 0)::bigint AS positions,
                coalesce(max(ST_NPoints(geometry)), 0)::int AS largest
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    let cnt: i32 = counts.try_get("n").map_err(edit_cad_db_err)?;
    let positions: i64 = counts.try_get("positions").map_err(edit_cad_db_err)?;
    let largest: i32 = counts.try_get("largest").map_err(edit_cad_db_err)?;
    if cnt != feature_count || positions > 200_000 || largest > 4096 {
        return Err(EditError::detail(
            422,
            "Derived DXF position limit exceeded",
        ));
    }

    let records = sqlx::query(
        "SELECT feature_id, ST_AsGeoJSON(geometry, 15, 0)::jsonb AS geom,
                ST_IsValid(geometry) AND NOT ST_IsEmpty(geometry) AS valid
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
         ORDER BY feature_id LIMIT 10001",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;

    if records.len() != feature_count as usize {
        return Err(EditError::detail(
            422,
            "Source contains invalid, missing or excessive geometry",
        ));
    }
    let mut features = Vec::with_capacity(records.len());
    for r in records {
        let valid: bool = r.try_get("valid").map_err(edit_cad_db_err)?;
        let geom: Option<Value> = r.try_get("geom").map_err(edit_cad_db_err)?;
        let Some(geom) = geom.filter(|_| valid) else {
            return Err(EditError::detail(
                422,
                "Source contains invalid, missing or excessive geometry",
            ));
        };
        let fid: String = r.try_get("feature_id").map_err(edit_cad_db_err)?;
        features.push(json!({ "feature_id": fid, "geometry": geom }));
    }

    Ok(GeorefDraftContext {
        sha256,
        storage_key: row.try_get("storage_key").map_err(edit_cad_db_err)?,
        entity_summary: row.try_get("entity_summary").map_err(edit_cad_db_err)?,
        source_id,
        layer_id: row.try_get("layer_id").map_err(edit_cad_db_err)?,
        content_revision,
        schema_revision,
        layer_name: row.try_get("layer_name").map_err(edit_cad_db_err)?,
        features,
    })
}

pub struct NativeImportContext {
    pub entity_summary: Value,
    pub source_id: Uuid,
    pub imported_content_revision: String,
    pub content_revision: String,
    pub schema_revision: String,
    pub original_sha: String,
    pub original_key: String,
    pub candidate_sha: String,
    pub candidate_key: String,
}

pub async fn load_native_import_context(
    scope: &mut EditScope<'_>,
    document_id: Uuid,
    version: i32,
    conversion_id: Uuid,
    expected_candidate_sha256: &str,
) -> Result<NativeImportContext, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended($1, 0))")
        .bind(format!("cad-doc:{project_id}"))
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_cad_db_err)?;

    let row = sqlx::query(
        "SELECT c.entity_summary, c.source_id, c.content_revision AS imported_content_revision,
                s.content_revision, s.schema_revision,
                original.sha256 AS original_sha, original.storage_key AS original_key,
                candidate.sha256 AS candidate_sha, candidate.storage_key AS candidate_key
         FROM cad.conversions c
         JOIN cad.document_versions original ON original.organization_id = c.organization_id
           AND original.project_id = c.project_id AND original.document_id = $3
           AND original.version = $4 AND original.sha256 = c.sha256
           AND original.document_id::text = c.entity_summary->>'cad_document_id'
           AND original.version::text = c.entity_summary->>'cad_document_version'
         JOIN cad.document_versions candidate ON candidate.organization_id = c.organization_id
           AND candidate.project_id = c.project_id AND candidate.document_id = $3
           AND candidate.version = original.version + 1
         JOIN spatial.data_sources s ON s.organization_id = c.organization_id
           AND s.project_id = c.project_id AND s.id = c.source_id
         JOIN spatial.layers l ON l.organization_id = c.organization_id
           AND l.project_id = c.project_id AND l.id = c.layer_id AND l.source_id = s.id
         WHERE c.organization_id = $1 AND c.project_id = $2 AND c.id = $5
           AND c.direction = 'import' AND c.status = 'succeeded'
           AND original.format = 'dxf' AND candidate.format = 'dxf'
           AND s.lifecycle = 'active' AND s.validation_state = 'ready'
           AND s.stored_crs = 3857 AND s.dimensions = 'XY' AND l.lifecycle = 'active'
           AND NOT EXISTS (
             SELECT 1 FROM cad.document_versions newer
             WHERE newer.organization_id = c.organization_id AND newer.project_id = c.project_id
               AND newer.document_id = $3 AND newer.version > candidate.version
           )
         FOR SHARE OF s, l",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(document_id)
    .bind(version)
    .bind(conversion_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?
    .ok_or_else(|| {
        EditError::detail(
            404,
            "Current next DXF revision and pinned CAD conversion required",
        )
    })?;

    let candidate_sha: String = row.try_get("candidate_sha").map_err(edit_cad_db_err)?;
    if candidate_sha != expected_candidate_sha256 {
        return Err(EditError::detail(409, "Saved CAD candidate changed"));
    }

    Ok(NativeImportContext {
        entity_summary: row.try_get("entity_summary").map_err(edit_cad_db_err)?,
        source_id: row.try_get("source_id").map_err(edit_cad_db_err)?,
        imported_content_revision: row
            .try_get("imported_content_revision")
            .map_err(edit_cad_db_err)?,
        content_revision: row.try_get("content_revision").map_err(edit_cad_db_err)?,
        schema_revision: row.try_get("schema_revision").map_err(edit_cad_db_err)?,
        original_sha: row.try_get("original_sha").map_err(edit_cad_db_err)?,
        original_key: row.try_get("original_key").map_err(edit_cad_db_err)?,
        candidate_sha,
        candidate_key: row.try_get("candidate_key").map_err(edit_cad_db_err)?,
    })
}

pub async fn validate_geojson_ops_postgis(
    scope: &mut EditScope<'_>,
    ops: &[Value],
) -> Result<(), EditError> {
    if ops.is_empty() {
        return Ok(());
    }
    let payload: Vec<Value> = ops
        .iter()
        .map(|op| {
            json!({
                "feature_id": op["feature_id"],
                "geometry": op["geometry"],
            })
        })
        .collect();
    let serialized =
        serde_json::to_string(&payload).map_err(|e| EditError::Internal(e.to_string()))?;
    let invalid: Option<String> = sqlx::query_scalar(
        "SELECT item->>'feature_id'
         FROM jsonb_array_elements($1::jsonb) AS item
         CROSS JOIN LATERAL (
             SELECT ST_GeomFromGeoJSON(item->'geometry') AS geom
         ) AS shape
         WHERE ST_IsEmpty(shape.geom) OR NOT ST_IsValid(shape.geom)
            OR ST_NPoints(shape.geom) > 4096
         LIMIT 1",
    )
    .bind(serialized)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_cad_db_err)?;
    if let Some(fid) = invalid {
        return Err(EditError::detail(
            422,
            format!("Invalid or oversized CAD geometry for {fid}"),
        ));
    }
    Ok(())
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
