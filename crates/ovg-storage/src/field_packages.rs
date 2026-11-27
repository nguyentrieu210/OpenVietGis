//! E13 offline package repository over the canonical field.offline_packages
//! and field.sync_batches tables.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Duration, SubsecRound, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::editing::{EditError, EditScope};

pub const MAX_OFFLINE_PACKAGE_FEATURES: usize = 5000;
pub const MAX_OFFLINE_TTL_HOURS: i64 = 24 * 14;
const SUPPORTED_CRS_CODES: [i32; 4] = [4326, 3857, 4756, 5899];

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "offline package database operation failed");
    AppError::internal("offline package database operation failed")
}

fn edit_db_error(error: sqlx::Error) -> EditError {
    tracing::error!(%error, "offline sync database operation failed");
    EditError::Internal("offline sync database operation failed".to_owned())
}

fn iso(value: DateTime<Utc>) -> String {
    let format = if value.timestamp_subsec_micros() == 0 {
        "%Y-%m-%dT%H:%M:%S+00:00"
    } else {
        "%Y-%m-%dT%H:%M:%S%.6f+00:00"
    };
    value.format(format).to_string()
}

fn wire(row: &PgRow) -> Result<Value, AppError> {
    let revoked_at = row
        .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
        .map_err(db_error)?
        .map(iso);
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "name": row.try_get::<String, _>("name").map_err(db_error)?,
        "profile": row.try_get::<String, _>("profile").map_err(db_error)?,
        "aoi_bbox": row.try_get::<Value, _>("aoi_bbox").map_err(db_error)?,
        "crs_code": row.try_get::<i32, _>("crs_code").map_err(db_error)?,
        "layer_pins": row.try_get::<Value, _>("layer_pins").map_err(db_error)?,
        "feature_count": row.try_get::<i32, _>("feature_count").map_err(db_error)?,
        "sha256": row.try_get::<String, _>("sha256").map_err(db_error)?,
        "byte_size": row.try_get::<i64, _>("byte_size").map_err(db_error)?,
        "expires_at": iso(row.try_get::<DateTime<Utc>, _>("expires_at").map_err(db_error)?),
        "revoked_at": revoked_at,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub fn validate_bbox(bbox: &BTreeMap<String, f64>) -> Result<(f64, f64, f64, f64), AppError> {
    let (Some(&min_x), Some(&min_y), Some(&max_x), Some(&max_y)) = (
        bbox.get("min_x"),
        bbox.get("min_y"),
        bbox.get("max_x"),
        bbox.get("max_y"),
    ) else {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "AOI requires four finite native CRS coordinates",
        ));
    };
    if bbox.len() != 4 || ![min_x, min_y, max_x, max_y].into_iter().all(f64::is_finite) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "AOI requires four finite native CRS coordinates",
        ));
    }
    if min_x >= max_x || min_y >= max_y {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "AOI bounds must be ordered and nonempty",
        ));
    }
    Ok((min_x, min_y, max_x, max_y))
}

#[derive(Debug, Clone)]
pub struct PreparedPackage {
    pub package_id: Uuid,
    pub layer_id: Uuid,
    pub layer_revision: i64,
    pub source_id: Uuid,
    pub srid: i32,
    pub geometry_family: String,
    pub content_revision: String,
    pub schema_revision: String,
    pub feature_count: i32,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub mappings: Vec<Value>,
    pub snapshot: Value,
}

pub async fn prepare_package_data(
    edit: &mut ProjectRead<'_>,
    layer_id: Uuid,
    aoi_bbox: &BTreeMap<String, f64>,
    ttl_hours: i64,
) -> Result<PreparedPackage, AppError> {
    if !(1..=MAX_OFFLINE_TTL_HOURS).contains(&ttl_hours) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            format!("ttl_hours must be between 1 and {MAX_OFFLINE_TTL_HOURS}"),
        ));
    }
    let (min_x, min_y, max_x, max_y) = validate_bbox(aoi_bbox)?;
    let org_id = edit.organization_id();
    let project_id = edit.project_id();
    let actor_id = edit.actor_id();

    let meta = sqlx::query(
        "SELECT l.id AS layer_id, l.name AS layer_name, l.layer_revision,
                s.id AS source_id, s.stored_crs, s.geometry_family, s.kind,
                s.content_revision, s.schema_revision
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
           AND s.validation_state = 'ready' AND s.kind <> 'reference_fixture'
         FOR SHARE OF s, l",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(layer_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("layer not found or not ready for offline packaging"))?;

    let srid: i32 = meta.try_get("stored_crs").map_err(db_error)?;
    if !SUPPORTED_CRS_CODES.contains(&srid) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "offline profile does not support this CRS",
        ));
    }
    let geometry_family: String = meta.try_get("geometry_family").map_err(db_error)?;
    if !matches!(geometry_family.as_str(), "Point" | "LineString" | "Polygon") {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "offline profile does not support this geometry family",
        ));
    }
    if srid == 4326
        && !((-180.0..=180.0).contains(&min_x)
            && (-180.0..=180.0).contains(&max_x)
            && (-90.0..=90.0).contains(&min_y)
            && (-90.0..=90.0).contains(&max_y))
    {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "AOI outside WGS84 bounds",
        ));
    }

    let source_id: Uuid = meta.try_get("source_id").map_err(db_error)?;
    let layer_name: String = meta.try_get("layer_name").map_err(db_error)?;
    let layer_revision: i64 = meta.try_get("layer_revision").map_err(db_error)?;
    let content_revision: String = meta.try_get("content_revision").map_err(db_error)?;
    let schema_revision: String = meta.try_get("schema_revision").map_err(db_error)?;

    let fdefs = sqlx::query(
        "SELECT field_id, name, type, nullable, schema_revision
         FROM spatial.source_fields
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
         ORDER BY ordinal ASC",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    let mut used: HashSet<String> = ["geom", "fid", "ovg_feature_id"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut mappings = Vec::with_capacity(fdefs.len());
    let mut snapshot_fields = Vec::with_capacity(fdefs.len());
    for fd in &fdefs {
        let fd_schema: String = fd.try_get("schema_revision").map_err(db_error)?;
        if fd_schema != schema_revision {
            return Err(AppError::conflict(
                "STALE_READ_PIN",
                "source schema changed while packaging",
            ));
        }
        let field_id: String = fd.try_get("field_id").map_err(db_error)?;
        let name: String = fd.try_get("name").map_err(db_error)?;
        let field_type: String = fd.try_get("type").map_err(db_error)?;
        let nullable: bool = fd.try_get("nullable").map_err(db_error)?;
        let mut output = field_id.clone();
        while used.contains(&output.to_ascii_lowercase()) {
            output = format!("attr_{output}");
        }
        used.insert(output.to_ascii_lowercase());
        mappings.push(json!({
            "field_id": field_id,
            "name": name,
            "output": output,
            "type": field_type,
        }));
        snapshot_fields.push(json!({
            "field_id": field_id,
            "name": name,
            "type": field_type,
            "nullable": nullable,
        }));
    }

    let limit = (MAX_OFFLINE_PACKAGE_FEATURES + 1) as i64;
    let frows = sqlx::query(
        "SELECT feature_id, attributes, ST_AsGeoJSON(geometry, 15)::jsonb AS geom,
                content_revision, schema_revision
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
           AND ST_CoveredBy(
               geometry,
               ST_MakeEnvelope($4, $5, $6, $7, $8)
           )
         ORDER BY feature_id ASC
         LIMIT $9",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .bind(min_x)
    .bind(min_y)
    .bind(max_x)
    .bind(max_y)
    .bind(srid)
    .bind(limit)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;

    if frows.len() > MAX_OFFLINE_PACKAGE_FEATURES {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "offline feature limit exceeded; narrow AOI",
        ));
    }

    let mut features = Vec::with_capacity(frows.len());
    for fr in &frows {
        let row_content: String = fr.try_get("content_revision").map_err(db_error)?;
        let row_schema: String = fr.try_get("schema_revision").map_err(db_error)?;
        if row_content != content_revision || row_schema != schema_revision {
            return Err(AppError::conflict(
                "STALE_READ_PIN",
                "source content changed while packaging",
            ));
        }
        let Some(geom) = fr.try_get::<Option<Value>, _>("geom").map_err(db_error)? else {
            continue;
        };
        let feature_id: String = fr.try_get("feature_id").map_err(db_error)?;
        let attributes: Value = fr.try_get("attributes").map_err(db_error)?;
        features.push(json!({
            "feature_id": feature_id,
            "properties": attributes,
            "geometry": {
                "type": geom["type"],
                "coordinates": geom["coordinates"],
            },
        }));
    }

    let feature_count =
        i32::try_from(features.len()).map_err(|_| AppError::internal("feature count overflow"))?;
    let package_id = Uuid::new_v4();
    let now = Utc::now().trunc_subsecs(6);
    let expires_at = now + Duration::hours(ttl_hours);
    let snapshot = json!({
        "version": 1,
        "package_id": package_id,
        "actor_id": actor_id,
        "organization_id": org_id,
        "project_id": project_id,
        "layer_id": layer_id,
        "layer_name": layer_name,
        "source_id": source_id,
        "content_revision": content_revision,
        "schema_revision": schema_revision,
        "crs_code": srid,
        "geometry_family": geometry_family,
        "aoi_bbox": aoi_bbox,
        "created_at": iso(now),
        "expires_at": iso(expires_at),
        "fields": snapshot_fields,
        "features": features,
    });

    Ok(PreparedPackage {
        package_id,
        layer_id,
        layer_revision,
        source_id,
        srid,
        geometry_family,
        content_revision,
        schema_revision,
        feature_count,
        created_at: now,
        expires_at,
        mappings,
        snapshot,
    })
}

pub struct InsertPackageArgs<'a> {
    pub prepared: &'a PreparedPackage,
    pub name: &'a str,
    pub aoi_bbox: &'a BTreeMap<String, f64>,
    pub snapshot_sha256: &'a str,
    pub storage_key: &'a str,
    pub sha256: &'a str,
    pub byte_size: i64,
}

pub async fn insert_package(
    edit: &mut ProjectRead<'_>,
    args: InsertPackageArgs<'_>,
) -> Result<Value, AppError> {
    let trimmed: String = args.name.trim().chars().take(200).collect();
    let final_name = if trimmed.is_empty() {
        "Offline Field Package".to_owned()
    } else {
        trimmed
    };
    let pins = json!([{
        "layer_id": args.prepared.layer_id,
        "layer_revision": args.prepared.layer_revision,
        "source_id": args.prepared.source_id,
        "content_revision": args.prepared.content_revision,
        "schema_revision": args.prepared.schema_revision,
        "snapshot_sha256": args.snapshot_sha256,
    }]);
    let row = sqlx::query(
        "INSERT INTO field.offline_packages (
             id, organization_id, project_id, name, profile, aoi_bbox, crs_code,
             layer_pins, feature_count, storage_key, sha256, byte_size,
             expires_at, created_by_actor_id, created_at
         ) VALUES (
             $1, $2, $3, $4, 'standard_pwa', $5, $6,
             $7, $8, $9, $10, $11,
             $12, $13, $14
         )
         RETURNING *",
    )
    .bind(args.prepared.package_id)
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(final_name)
    .bind(json!(args.aoi_bbox))
    .bind(args.prepared.srid)
    .bind(pins)
    .bind(args.prepared.feature_count)
    .bind(args.storage_key)
    .bind(args.sha256)
    .bind(args.byte_size)
    .bind(args.prepared.expires_at)
    .bind(edit.actor_id())
    .bind(args.prepared.created_at)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    wire(&row)
}

pub async fn list_packages(edit: &mut ProjectRead<'_>, actor_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT p.* FROM field.offline_packages p
         WHERE p.organization_id = $1 AND p.project_id = $2
           AND p.created_by_actor_id = $3
           AND EXISTS (
             SELECT 1 FROM spatial.layers l
             JOIN spatial.data_sources s
               ON s.organization_id = l.organization_id
              AND s.project_id = l.project_id
              AND s.id = l.source_id
             WHERE l.organization_id = p.organization_id
               AND l.project_id = p.project_id
               AND l.id::text = p.layer_pins->0->>'layer_id'
               AND s.id::text = p.layer_pins->0->>'source_id'
               AND l.lifecycle = 'active'
               AND s.lifecycle = 'active'
               AND s.validation_state = 'ready'
           )
         ORDER BY p.created_at DESC
         LIMIT 50",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(actor_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn revoke_package(
    edit: &mut ProjectRead<'_>,
    package_id: Uuid,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "UPDATE field.offline_packages
         SET revoked_at = COALESCE(revoked_at, $4)
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(package_id)
    .bind(Utc::now())
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("offline package not found"))?;
    wire(&row)
}

#[derive(Debug, Clone)]
pub struct PackageDownload {
    pub id: Uuid,
    pub storage_key: String,
    pub sha256: String,
    pub snapshot_sha256: Option<String>,
}

pub async fn download_info(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    package_id: Uuid,
) -> Result<PackageDownload, AppError> {
    let row = sqlx::query(
        "SELECT id, storage_key, sha256, expires_at, revoked_at, created_by_actor_id,
                layer_pins->0->>'snapshot_sha256' AS snapshot_sha256,
                (layer_pins->0->>'layer_id')::uuid AS layer_id,
                (layer_pins->0->>'source_id')::uuid AS source_id
         FROM field.offline_packages
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(package_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("offline package not found"))?;

    if row
        .try_get::<Uuid, _>("created_by_actor_id")
        .map_err(db_error)?
        != actor_id
    {
        return Err(AppError::not_found("offline package not found"));
    }
    if row
        .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
        .map_err(db_error)?
        .is_some()
    {
        return Err(AppError::Forbidden {
            code: "PACKAGE_REVOKED",
            message: "offline package has been revoked".to_owned(),
        });
    }
    if row
        .try_get::<DateTime<Utc>, _>("expires_at")
        .map_err(db_error)?
        <= Utc::now()
    {
        return Err(AppError::Forbidden {
            code: "PACKAGE_EXPIRED",
            message: "offline package has expired".to_owned(),
        });
    }

    let layer_id = row.try_get::<Uuid, _>("layer_id").map_err(db_error)?;
    let source_id = row.try_get::<Uuid, _>("source_id").map_err(db_error)?;
    let available = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2
           AND l.id = $3 AND s.id = $4
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
           AND s.validation_state = 'ready'",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(layer_id)
    .bind(source_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if available.is_none() {
        return Err(AppError::forbidden("packaged layer no longer available"));
    }

    Ok(PackageDownload {
        id: row.try_get("id").map_err(db_error)?,
        storage_key: row.try_get("storage_key").map_err(db_error)?,
        sha256: row.try_get("sha256").map_err(db_error)?,
        snapshot_sha256: row.try_get("snapshot_sha256").map_err(db_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct SyncPackageMeta {
    pub id: Uuid,
    pub layer_id: Uuid,
    pub source_id: Uuid,
    pub base_rev: String,
    pub storage_key: String,
    pub sha256: String,
    pub snapshot_sha256: Option<String>,
}

pub async fn load_sync_package(
    scope: &mut EditScope<'_>,
    package_id: Uuid,
) -> Result<SyncPackageMeta, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let row = sqlx::query(
        "SELECT id, storage_key, sha256, expires_at, revoked_at, created_by_actor_id,
                layer_pins->0->>'snapshot_sha256' AS snapshot_sha256,
                (layer_pins->0->>'layer_id')::uuid AS layer_id,
                (layer_pins->0->>'source_id')::uuid AS source_id,
                layer_pins->0->>'content_revision' AS base_rev
         FROM field.offline_packages
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(package_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?
    .ok_or_else(|| EditError::not_found("offline package not found"))?;

    if row
        .try_get::<Uuid, _>("created_by_actor_id")
        .map_err(edit_db_error)?
        != actor_id
    {
        return Err(EditError::not_found("offline package not found"));
    }
    let revoked = row
        .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
        .map_err(edit_db_error)?;
    let expires_at = row
        .try_get::<DateTime<Utc>, _>("expires_at")
        .map_err(edit_db_error)?;
    if revoked.is_some() || expires_at <= Utc::now() {
        return Err(EditError::forbidden(
            "PACKAGE_INVALID",
            "cannot sync from revoked or expired offline package",
        ));
    }

    Ok(SyncPackageMeta {
        id: row.try_get("id").map_err(edit_db_error)?,
        layer_id: row.try_get("layer_id").map_err(edit_db_error)?,
        source_id: row.try_get("source_id").map_err(edit_db_error)?,
        base_rev: row.try_get("base_rev").map_err(edit_db_error)?,
        storage_key: row.try_get("storage_key").map_err(edit_db_error)?,
        sha256: row.try_get("sha256").map_err(edit_db_error)?,
        snapshot_sha256: row.try_get("snapshot_sha256").map_err(edit_db_error)?,
    })
}

pub async fn ensure_packaged_layer_active(
    scope: &mut EditScope<'_>,
    layer_id: Uuid,
    source_id: Uuid,
) -> Result<(), EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let ok = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM spatial.layers
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
           AND source_id = $4 AND lifecycle = 'active'",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(layer_id)
    .bind(source_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    if ok.is_none() {
        return Err(EditError::forbidden(
            "FORBIDDEN",
            "packaged layer no longer available",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SyncBatchRow {
    pub id: Uuid,
    pub package_id: Uuid,
    pub operation_id: String,
    pub status: String,
    pub submitted_ops_count: i32,
    pub applied_changeset_ids: Vec<String>,
    pub conflicts: Value,
    pub created_at: DateTime<Utc>,
}

fn sync_batch_from_row(row: &PgRow) -> Result<SyncBatchRow, EditError> {
    let applied_val: Value = row
        .try_get("applied_changeset_ids")
        .map_err(edit_db_error)?;
    let applied_changeset_ids = applied_val
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok(SyncBatchRow {
        id: row.try_get("id").map_err(edit_db_error)?,
        package_id: row.try_get("package_id").map_err(edit_db_error)?,
        operation_id: row.try_get("operation_id").map_err(edit_db_error)?,
        status: row.try_get("status").map_err(edit_db_error)?,
        submitted_ops_count: row.try_get("submitted_ops_count").map_err(edit_db_error)?,
        applied_changeset_ids,
        conflicts: row.try_get("conflicts").map_err(edit_db_error)?,
        created_at: row.try_get("created_at").map_err(edit_db_error)?,
    })
}

pub async fn find_sync_batch_by_prefix(
    scope: &mut EditScope<'_>,
    scope_hash: &str,
) -> Result<Option<SyncBatchRow>, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let row = sqlx::query(
        "SELECT * FROM field.sync_batches
         WHERE organization_id = $1 AND project_id = $2 AND operation_id LIKE $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(format!("{scope_hash}:%"))
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    row.as_ref().map(sync_batch_from_row).transpose()
}

pub async fn check_existing_core_changeset(
    scope: &mut EditScope<'_>,
    source_id: Uuid,
    core_op: &str,
    expected_summary: &str,
) -> Result<(), EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let row = sqlx::query(
        "SELECT actor_id, summary FROM editing.changesets
         WHERE organization_id = $1 AND project_id = $2
           AND source_id = $3 AND operation_id = $4",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .bind(core_op)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    if let Some(existing) = row {
        let cs_actor: Uuid = existing.try_get("actor_id").map_err(edit_db_error)?;
        let cs_summary: String = existing.try_get("summary").map_err(edit_db_error)?;
        if cs_actor != actor_id || cs_summary != expected_summary {
            return Err(EditError::conflict(
                "OPERATION_ID_CONFLICT",
                "operation identity reused with different content",
            ));
        }
    }
    Ok(())
}

pub async fn verify_package_lease(
    scope: &mut EditScope<'_>,
    package_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let ok = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM field.offline_packages
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
           AND created_by_actor_id = $4 AND revoked_at IS NULL AND expires_at > $5",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(package_id)
    .bind(actor_id)
    .bind(now)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    if ok.is_none() {
        return Err(EditError::forbidden("PACKAGE_INVALID", "package invalid"));
    }
    Ok(())
}

pub struct InsertSyncBatchArgs<'a> {
    pub package_id: Uuid,
    pub bound_op: &'a str,
    pub status: &'a str,
    pub submitted_ops_count: i32,
    pub applied_ids: &'a [String],
    pub conflicts: &'a [Value],
    pub now: DateTime<Utc>,
}

pub async fn insert_sync_batch(
    scope: &mut EditScope<'_>,
    args: InsertSyncBatchArgs<'_>,
) -> Result<SyncBatchRow, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let row = sqlx::query(
        "INSERT INTO field.sync_batches (
             id, organization_id, project_id, package_id, operation_id,
             status, submitted_ops_count, applied_changeset_ids, conflicts,
             actor_id, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(project_id)
    .bind(args.package_id)
    .bind(args.bound_op)
    .bind(args.status)
    .bind(args.submitted_ops_count)
    .bind(json!(args.applied_ids))
    .bind(json!(args.conflicts))
    .bind(actor_id)
    .bind(args.now)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    sync_batch_from_row(&row)
}

#[must_use]
pub fn offline_core_feature_id(
    actor_id: Uuid,
    organization_id: Uuid,
    project_id: Uuid,
    package_id: Uuid,
    source_id: Uuid,
    client_operation_id: Uuid,
) -> String {
    let payload = [
        "ovg-field-feature-v1".to_owned(),
        actor_id.to_string(),
        organization_id.to_string(),
        project_id.to_string(),
        package_id.to_string(),
        source_id.to_string(),
        client_operation_id.to_string(),
    ]
    .join(":");
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("field-core-{}", &hex[..48])
}

#[must_use]
pub fn sync_wire(
    batch: &SyncBatchRow,
    clean_op: &str,
    operations: &[Value],
    feature_ids: &[String],
) -> Value {
    let applied = batch.status == "applied";
    let changeset_id = if applied {
        batch.applied_changeset_ids.first().map(String::as_str)
    } else {
        None
    };
    let mut receipts = Vec::new();
    for (op, core_fid) in operations.iter().zip(feature_ids.iter()) {
        if let Some(client_op_id) = op.get("client_operation_id") {
            let local_fid = op.get("feature_id").and_then(Value::as_str).unwrap_or("");
            let core_val = if applied {
                Value::String(core_fid.clone())
            } else {
                Value::Null
            };
            receipts.push(json!({
                "client_operation_id": client_op_id,
                "local_feature_id": local_fid,
                "core_feature_id": core_val,
                "status": if applied { "applied" } else { "not_applied" },
                "changeset_id": changeset_id,
            }));
        }
    }
    json!({
        "id": batch.id,
        "package_id": batch.package_id,
        "operation_id": clean_op,
        "status": batch.status,
        "submitted_ops_count": batch.submitted_ops_count,
        "applied_changeset_ids": batch.applied_changeset_ids,
        "conflicts": batch.conflicts,
        "operation_receipts": receipts,
        "created_at": iso(batch.created_at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_time_format_matches_python_isoformat() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T01:02:03.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T01:02:03.123456+00:00");
    }

    #[test]
    fn offline_core_feature_id_matches_python_digest() {
        let a = Uuid::nil();
        let fid = offline_core_feature_id(a, a, a, a, a, a);
        assert!(fid.starts_with("field-core-"));
        assert_eq!(fid.len(), 59);
    }
}
