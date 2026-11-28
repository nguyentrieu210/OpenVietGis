//! E08 durable processing job repository over the canonical `processing.jobs` table.
//!
//! Callers must open [`ProjectRead`] or [`EditScope`] with the correct read/edit
//! policy first; this repository keeps every statement inside that already
//! authorized, RLS-bound transaction.

use std::collections::HashMap;

use chrono::{DateTime, SubsecRound, Utc};
use ovg_core::crs::{
    reference_transformation, FixtureRequirement, WEB_MERCATOR_MAX_COORDINATE_METRE,
    WEB_MERCATOR_MAX_LATITUDE,
};
use ovg_core::AppError;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::postgres::PgRow;
use sqlx::{Acquire, Row};
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::editing::domain::{
    canonical_attribute, py_eq, py_float, py_strip, sha256_py_json, validate_operation_id, FieldDef,
};
use crate::editing::{EditError, EditScope};
use crate::spatial_pins::{
    content_revision_digest, schema_revision_digest, validate_ready_wkb, FeaturePinInput, FieldSpec,
};

pub const MAX_PROCESSING_FEATURES: i64 = 50_000;

fn overlay_algorithm(algorithm: &str) -> bool {
    matches!(
        algorithm,
        "clip" | "difference" | "intersection" | "union" | "spatial_join"
    )
}
pub const MAX_PROCESSING_POSITIONS: i64 = 1_000_000;
const PROCESSING_STATEMENT_TIMEOUT: &str = "30s";

const RESULT_GEOMETRY_COLUMNS: &str = "
    ST_SRID(geom) AS srid,
    COUNT(*) OVER () AS total_count,
    COALESCE(SUM(ST_NPoints(geom)) OVER (), 0)::bigint AS total_positions,
    CASE WHEN COUNT(*) OVER () <= $max_features
        AND COALESCE(SUM(ST_NPoints(geom)) OVER (), 0) <= $max_positions
        AND ST_Zmflag(geom) = 0
        AND ST_NPoints(geom) BETWEEN 1 AND 4096
        AND ST_IsValid(geom)
      THEN ST_AsBinary(geom, 'NDR')
      ELSE NULL END AS wkb
";

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "processing job database operation failed");
    AppError::internal("processing job database operation failed")
}

fn edit_db_error(error: sqlx::Error) -> EditError {
    tracing::error!(%error, "processing job database operation failed");
    EditError::Internal("processing job database operation failed".into())
}

fn not_found() -> AppError {
    AppError::not_found("processing job not found")
}

fn iso(value: DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        value.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    }
}

fn now_micros() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}

fn row_wire(row: &PgRow) -> Result<Value, AppError> {
    let completed_at = row
        .try_get::<Option<DateTime<Utc>>, _>("completed_at")
        .map_err(db_error)?
        .map(iso);
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "organization_id": row.try_get::<Uuid, _>("organization_id").map_err(db_error)?,
        "project_id": row.try_get::<Uuid, _>("project_id").map_err(db_error)?,
        "algorithm_id": row.try_get::<String, _>("algorithm_id").map_err(db_error)?,
        "operation_id": row.try_get::<String, _>("operation_id").map_err(db_error)?,
        "status": row.try_get::<String, _>("status").map_err(db_error)?,
        "actor_type": row.try_get::<String, _>("actor_type").map_err(db_error)?,
        "actor_id": row.try_get::<Uuid, _>("actor_id").map_err(db_error)?,
        "input_refs": row.try_get::<Value, _>("input_refs").map_err(db_error)?,
        "parameters": row.try_get::<Value, _>("parameters").map_err(db_error)?,
        "output_name": row.try_get::<String, _>("output_name").map_err(db_error)?,
        "output_crs": row.try_get::<i32, _>("output_crs").map_err(db_error)?,
        "engine_version": row.try_get::<String, _>("engine_version").map_err(db_error)?,
        "attempt_count": row.try_get::<i32, _>("attempt_count").map_err(db_error)?,
        "output_source_id": row.try_get::<Option<Uuid>, _>("output_source_id").map_err(db_error)?,
        "output_layer_id": row.try_get::<Option<Uuid>, _>("output_layer_id").map_err(db_error)?,
        "output_content_revision": row.try_get::<Option<String>, _>("output_content_revision").map_err(db_error)?,
        "feature_count": row.try_get::<Option<i32>, _>("feature_count").map_err(db_error)?,
        "error_code": row.try_get::<Option<String>, _>("error_code").map_err(db_error)?,
        "error_message": row.try_get::<Option<String>, _>("error_message").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
        "completed_at": completed_at,
    }))
}

fn row_wire_edit(row: &PgRow) -> Result<Value, EditError> {
    row_wire(row).map_err(|e| EditError::Internal(e.to_string()))
}

pub async fn list_jobs(read: &mut ProjectRead<'_>, limit: i64) -> Result<Value, AppError> {
    if !(1..=200).contains(&limit) {
        return Err(AppError::validation(
            "INVALID_REQUEST",
            "limit must be between 1 and 200",
        ));
    }
    let rows = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(limit)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(row_wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn get_job(read: &mut ProjectRead<'_>, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(job_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(not_found)?;
    row_wire(&row)
}

pub async fn cancel_job(write: &mut ProjectRead<'_>, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT status FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         FOR UPDATE",
    )
    .bind(write.organization_id())
    .bind(write.project_id())
    .bind(job_id)
    .fetch_optional(&mut **write.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(not_found)?;
    let status: String = row.try_get("status").map_err(db_error)?;
    validate_cancel_status(&status)?;

    let row = sqlx::query(
        "UPDATE processing.jobs
         SET status = 'cancelled', updated_at = $4, completed_at = $4
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(write.organization_id())
    .bind(write.project_id())
    .bind(job_id)
    .bind(Utc::now())
    .fetch_one(&mut **write.transaction())
    .await
    .map_err(db_error)?;
    row_wire(&row)
}

pub async fn list_jobs_scoped(scope: &mut EditScope<'_>, limit: i64) -> Result<Value, EditError> {
    if !(1..=200).contains(&limit) {
        return Err(EditError::InvalidRequest);
    }
    let org = scope.organization_id();
    let proj = scope.project_id();
    let rows = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(org)
    .bind(proj)
    .bind(limit)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    let items = rows
        .iter()
        .map(row_wire_edit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn get_job_scoped(scope: &mut EditScope<'_>, job_id: Uuid) -> Result<Value, EditError> {
    let org = scope.organization_id();
    let proj = scope.project_id();
    let row = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org)
    .bind(proj)
    .bind(job_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?
    .ok_or_else(|| EditError::not_found("processing job not found"))?;
    row_wire_edit(&row)
}

pub async fn cancel_job_scoped(
    scope: &mut EditScope<'_>,
    job_id: Uuid,
) -> Result<Value, EditError> {
    let org = scope.organization_id();
    let proj = scope.project_id();
    let row = sqlx::query(
        "SELECT status FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         FOR UPDATE",
    )
    .bind(org)
    .bind(proj)
    .bind(job_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?
    .ok_or_else(|| EditError::not_found("processing job not found"))?;
    let status: String = row.try_get("status").map_err(edit_db_error)?;
    if matches!(status.as_str(), "succeeded" | "cancelled") {
        return Err(EditError::conflict(
            "JOB_TERMINAL",
            format!("cannot cancel job in status {status}"),
        ));
    }

    let row = sqlx::query(
        "UPDATE processing.jobs
         SET status = 'cancelled', updated_at = $4, completed_at = $4
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(org)
    .bind(proj)
    .bind(job_id)
    .bind(now_micros())
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    row_wire_edit(&row)
}

fn validate_cancel_status(status: &str) -> Result<(), AppError> {
    if matches!(status, "succeeded" | "cancelled") {
        return Err(AppError::conflict(
            "JOB_TERMINAL",
            format!("cannot cancel job in status {status}"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ResolvedSource {
    id: Uuid,
    stored_crs: i32,
    geometry_family: String,
    feature_count: i64,
    content_revision: String,
    schema_revision: String,
}

async fn resolve_layer(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
    project_id: Uuid,
    layer_id: Uuid,
) -> Result<ResolvedSource, EditError> {
    let row = sqlx::query(
        "SELECT s.id, s.lifecycle, s.validation_state, s.stored_crs, s.geometry_family,
                s.feature_count, s.content_revision, s.schema_revision
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
           AND l.lifecycle = 'active'
         FOR SHARE OF s, l",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(layer_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(edit_db_error)?
    .ok_or_else(|| EditError::not_found(format!("layer {layer_id} not found")))?;

    let lifecycle: String = row.try_get("lifecycle").map_err(edit_db_error)?;
    let validation_state: String = row.try_get("validation_state").map_err(edit_db_error)?;
    let stored_crs: Option<i32> = row.try_get("stored_crs").map_err(edit_db_error)?;
    let feature_count: Option<i32> = row.try_get("feature_count").map_err(edit_db_error)?;
    if lifecycle != "active"
        || validation_state != "ready"
        || stored_crs.is_none()
        || feature_count.is_none()
    {
        return Err(EditError::validation(
            "SOURCE_NOT_READY",
            "input layer source is not ready",
        ));
    }
    Ok(ResolvedSource {
        id: row.try_get("id").map_err(edit_db_error)?,
        stored_crs: stored_crs.unwrap_or_default(),
        geometry_family: row.try_get("geometry_family").map_err(edit_db_error)?,
        feature_count: i64::from(feature_count.unwrap_or_default()),
        content_revision: row.try_get("content_revision").map_err(edit_db_error)?,
        schema_revision: row.try_get("schema_revision").map_err(edit_db_error)?,
    })
}

async fn validate_pinned_source(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
    project_id: Uuid,
    input_refs: &Value,
    prefix: &str,
) -> Result<ResolvedSource, EditError> {
    let schema_key = format!("{prefix}_schema_revision");
    if input_refs
        .get(&schema_key)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(EditError::conflict(
            "MISSING_INPUT_PIN",
            "job input has no immutable schema pin; submit a new job",
        ));
    }
    let layer_id_str = input_refs
        .get(format!("{prefix}_layer_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            EditError::conflict("STALE_INPUT_PIN", "job input is no longer active and ready")
        })?;
    let layer_id = Uuid::parse_str(layer_id_str).map_err(|_| {
        EditError::conflict("STALE_INPUT_PIN", "job input is no longer active and ready")
    })?;
    let source = match resolve_layer(tx, org_id, project_id, layer_id).await {
        Ok(src) => src,
        Err(EditError::Platform {
            status: 404 | 422, ..
        }) => {
            return Err(EditError::conflict(
                "STALE_INPUT_PIN",
                "job input is no longer active and ready",
            ));
        }
        Err(err) => return Err(err),
    };

    let expected_src_id = input_refs
        .get(format!("{prefix}_source_id"))
        .and_then(Value::as_str);
    let expected_crev = input_refs
        .get(format!("{prefix}_content_revision"))
        .and_then(Value::as_str);
    let expected_srev = input_refs
        .get(format!("{prefix}_schema_revision"))
        .and_then(Value::as_str);
    let expected_crs = input_refs
        .get(format!("{prefix}_stored_crs"))
        .and_then(Value::as_i64);

    if expected_src_id != Some(source.id.to_string().as_str())
        || expected_crev != Some(source.content_revision.as_str())
        || expected_srev != Some(source.schema_revision.as_str())
        || expected_crs != Some(i64::from(source.stored_crs))
    {
        return Err(EditError::conflict(
            "STALE_INPUT_PIN",
            "job input changed; submit a new job",
        ));
    }

    let stats = sqlx::query(
        "SELECT COUNT(*)::bigint AS cnt,
                COALESCE(SUM(ST_NPoints(geometry)), 0)::bigint AS positions,
                COUNT(*) FILTER (WHERE
                    content_revision <> $4 OR schema_revision <> $5
                    OR geometry_state <> 'present' OR geometry IS NULL
                    OR ST_SRID(geometry) <> $6
                    OR ST_GeometryType(geometry) <> $7
                    OR ST_Zmflag(geometry) <> 0 OR ST_IsEmpty(geometry)
                    OR NOT ST_IsValid(geometry))::bigint AS invalid
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source.id)
    .bind(&source.content_revision)
    .bind(&source.schema_revision)
    .bind(source.stored_crs)
    .bind(format!("ST_{}", source.geometry_family))
    .fetch_one(&mut **tx)
    .await
    .map_err(edit_db_error)?;

    let cnt: i64 = stats.try_get("cnt").map_err(edit_db_error)?;
    let positions: i64 = stats.try_get("positions").map_err(edit_db_error)?;
    let invalid: i64 = stats.try_get("invalid").map_err(edit_db_error)?;
    if cnt != source.feature_count || invalid > 0 {
        return Err(EditError::validation(
            "INVALID_INPUT",
            "input content does not match ready pins",
        ));
    }
    if cnt > MAX_PROCESSING_FEATURES || positions > MAX_PROCESSING_POSITIONS {
        return Err(EditError::validation(
            "PROCESSING_LIMIT",
            "processing input exceeds resource bounds",
        ));
    }
    Ok(source)
}

#[derive(Debug)]
pub struct SubmitJobResult {
    pub wire: Value,
    pub already_existed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedSource {
    pub source_id: Uuid,
    pub content_revision: String,
    pub schema_revision: String,
}

impl ExpectedSource {
    fn valid(&self) -> bool {
        [&self.content_revision, &self.schema_revision]
            .iter()
            .all(|r| r.len() == 64 && r.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    }

    fn matches_source(&self, s: &ResolvedSource) -> bool {
        self.source_id == s.id
            && self.content_revision == s.content_revision
            && self.schema_revision == s.schema_revision
    }

    fn matches_refs(&self, refs: &Value, prefix: &str) -> bool {
        refs.get(format!("{prefix}_source_id"))
            .and_then(Value::as_str)
            == Some(self.source_id.to_string().as_str())
            && refs
                .get(format!("{prefix}_content_revision"))
                .and_then(Value::as_str)
                == Some(self.content_revision.as_str())
            && refs
                .get(format!("{prefix}_schema_revision"))
                .and_then(Value::as_str)
                == Some(self.schema_revision.as_str())
    }
}

/// Persist a new queued processing job (or return idempotent replay) inside scope.
/// Legacy callers without explicit expectations retain the existing contract.
pub async fn submit_job(
    scope: &mut EditScope<'_>,
    algorithm_id: &str,
    input_layer_id: Uuid,
    overlay_layer_id: Option<Uuid>,
    clean_params: Value,
    output_name: &str,
    operation_id: &str,
) -> Result<SubmitJobResult, EditError> {
    submit_job_pinned(
        scope,
        algorithm_id,
        input_layer_id,
        overlay_layer_id,
        clean_params,
        output_name,
        operation_id,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn submit_job_pinned(
    scope: &mut EditScope<'_>,
    algorithm_id: &str,
    input_layer_id: Uuid,
    overlay_layer_id: Option<Uuid>,
    clean_params: Value,
    output_name: &str,
    operation_id: &str,
    expected_input: Option<&ExpectedSource>,
    expected_overlay: Option<&ExpectedSource>,
) -> Result<SubmitJobResult, EditError> {
    if expected_input.is_some_and(|p| !p.valid())
        || expected_overlay.is_some_and(|p| !p.valid())
        || expected_overlay.is_some()
            && (!overlay_algorithm(algorithm_id) || expected_input.is_none())
        || expected_input.is_some() && overlay_algorithm(algorithm_id) && expected_overlay.is_none()
    {
        return Err(EditError::validation(
            "INVALID_INPUT_PIN",
            "invalid or incomplete expected source pins",
        ));
    }
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let clean_op_id = validate_operation_id(operation_id)?;
    let stripped_name = py_strip(output_name);
    let clean_out_name: String = if stripped_name.is_empty() {
        format!("{algorithm_id} output")
    } else {
        stripped_name.chars().take(200).collect()
    };

    if !overlay_algorithm(algorithm_id) && overlay_layer_id.is_some() {
        return Err(EditError::validation(
            "INVALID_PARAMETERS",
            "only polygon overlay algorithms accept overlay_layer_id",
        ));
    }

    let existing = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND operation_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(&clean_op_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;

    if let Some(row) = existing {
        let prev_algo: String = row.try_get("algorithm_id").map_err(edit_db_error)?;
        let prev_refs: Value = row.try_get("input_refs").map_err(edit_db_error)?;
        let prev_params: Value = row.try_get("parameters").map_err(edit_db_error)?;
        let prev_name: String = row.try_get("output_name").map_err(edit_db_error)?;
        let expected_overlay_id = overlay_layer_id.map(|id| id.to_string());
        if prev_algo != algorithm_id
            || prev_refs.get("input_layer_id").and_then(Value::as_str)
                != Some(input_layer_id.to_string().as_str())
            || prev_refs
                .get("overlay_layer_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                != expected_overlay_id
            || !py_eq(&prev_params, &clean_params)
            || prev_name != clean_out_name
            || expected_input.is_some_and(|p| !p.matches_refs(&prev_refs, "input"))
            || expected_overlay.is_some_and(|p| !p.matches_refs(&prev_refs, "overlay"))
        {
            return Err(EditError::conflict(
                "OPERATION_ID_CONFLICT",
                "operation_id already used for a different processing request",
            ));
        }
        return Ok(SubmitJobResult {
            wire: row_wire_edit(&row)?,
            already_existed: true,
        });
    }

    let input_source = resolve_layer(scope.tx_mut(), org_id, project_id, input_layer_id).await?;
    if input_source.feature_count > MAX_PROCESSING_FEATURES {
        return Err(EditError::validation(
            "PROCESSING_LIMIT",
            "processing input exceeds feature bounds",
        ));
    }
    if algorithm_id != "reproject"
        && u32::try_from(input_source.stored_crs)
            .ok()
            .is_some_and(ovg_core::crs::is_extended_native_vn2000)
    {
        return Err(EditError::validation("UNSUPPORTED_TRANSFORMATION","Extended native VN-2000 layers currently support only Reproject; convert to an existing analysis CRS first"));
    }
    if expected_input.is_some_and(|p| !p.matches_source(&input_source)) {
        return Err(EditError::conflict(
            "STALE_INPUT_PIN",
            "input changed after request preparation",
        ));
    }
    let mut input_refs = Map::new();
    input_refs.insert("input_layer_id".into(), json!(input_layer_id.to_string()));
    input_refs.insert("input_source_id".into(), json!(input_source.id.to_string()));
    input_refs.insert(
        "input_content_revision".into(),
        json!(input_source.content_revision),
    );
    input_refs.insert(
        "input_schema_revision".into(),
        json!(input_source.schema_revision),
    );
    input_refs.insert("input_stored_crs".into(), json!(input_source.stored_crs));

    if overlay_algorithm(algorithm_id) {
        let Some(ov_layer_id) = overlay_layer_id else {
            return Err(EditError::validation(
                "MISSING_OVERLAY_LAYER",
                "polygon overlay algorithm requires overlay_layer_id",
            ));
        };
        let overlay_source = resolve_layer(scope.tx_mut(), org_id, project_id, ov_layer_id).await?;
        if u32::try_from(overlay_source.stored_crs)
            .ok()
            .is_some_and(ovg_core::crs::is_extended_native_vn2000)
        {
            return Err(EditError::validation(
                "UNSUPPORTED_TRANSFORMATION",
                "Extended native VN-2000 overlay must be reprojected first",
            ));
        }
        if expected_overlay.is_some_and(|p| !p.matches_source(&overlay_source)) {
            return Err(EditError::conflict(
                "STALE_INPUT_PIN",
                "overlay changed after request preparation",
            ));
        }
        if overlay_source.id == input_source.id
            || (algorithm_id != "spatial_join" && overlay_source.geometry_family != "Polygon")
            || (algorithm_id == "union" && input_source.geometry_family != "Polygon")
        {
            return Err(EditError::validation(
                "INVALID_OVERLAY",
                "overlay requires distinct compatible sources; union requires two Polygon layers",
            ));
        }
        input_refs.insert("overlay_layer_id".into(), json!(ov_layer_id.to_string()));
        input_refs.insert(
            "overlay_source_id".into(),
            json!(overlay_source.id.to_string()),
        );
        input_refs.insert(
            "overlay_content_revision".into(),
            json!(overlay_source.content_revision),
        );
        input_refs.insert(
            "overlay_schema_revision".into(),
            json!(overlay_source.schema_revision),
        );
        input_refs.insert(
            "overlay_stored_crs".into(),
            json!(overlay_source.stored_crs),
        );
    }

    let pg_ver: String = sqlx::query_scalar("SELECT PostGIS_Lib_Version()")
        .fetch_one(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_error)?;
    let engine_version = format!("PostGIS/{pg_ver}");
    let output_crs = if algorithm_id == "reproject" {
        let target = clean_params
            .get("target_srid")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .filter(|v| {
                u32::try_from(*v)
                    .ok()
                    .is_some_and(ovg_core::crs::is_processing_crs)
            })
            .ok_or_else(|| EditError::validation("INVALID_PARAMETERS", "unsupported target CRS"))?;
        approved_reproject(
            scope.tx_mut(),
            org_id,
            project_id,
            input_source.id,
            input_source.stored_crs,
            target,
        )
        .await?;
        target
    } else {
        input_source.stored_crs
    };
    let now = now_micros();
    let job_id = Uuid::new_v4();
    let actor_type = scope.actor_kind();
    let actor_id = scope.actor_id();

    let row = sqlx::query(
        "INSERT INTO processing.jobs (
            id, organization_id, project_id, algorithm_id, operation_id, status,
            actor_type, actor_id, input_refs, parameters, output_name, output_crs,
            engine_version, attempt_count, created_at, updated_at
         ) VALUES (
            $1, $2, $3, $4, $5, 'queued',
            $6, $7, $8, $9, $10, $11, $12, 1, $13, $13
         )
         RETURNING *",
    )
    .bind(job_id)
    .bind(org_id)
    .bind(project_id)
    .bind(algorithm_id)
    .bind(&clean_op_id)
    .bind(actor_type)
    .bind(actor_id)
    .bind(Value::Object(input_refs))
    .bind(clean_params)
    .bind(&clean_out_name)
    .bind(output_crs)
    .bind(&engine_version)
    .bind(now)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;

    Ok(SubmitJobResult {
        wire: row_wire_edit(&row)?,
        already_existed: false,
    })
}

/// Lock job for retry, reset to `queued` with incremented `attempt_count`, or return immediately.
pub async fn retry_job_prepare(
    scope: &mut EditScope<'_>,
    job_id: Uuid,
) -> Result<Option<Value>, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let row = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         FOR UPDATE",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(job_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?
    .ok_or_else(|| EditError::not_found("processing job not found"))?;

    let error_code: Option<String> = row.try_get("error_code").map_err(edit_db_error)?;
    if error_code.as_deref() == Some("RECOVERY_QUARANTINED") {
        return Err(EditError::conflict(
            "RECOVERY_QUARANTINED",
            "historical restored processing job cannot retry; submit a new operation",
        ));
    }
    let status: String = row.try_get("status").map_err(edit_db_error)?;
    if status == "succeeded" {
        return Ok(Some(row_wire_edit(&row)?));
    }

    let now = now_micros();
    sqlx::query(
        "UPDATE processing.jobs
         SET status = 'queued', attempt_count = attempt_count + 1,
             error_code = NULL, error_message = NULL, updated_at = $4, completed_at = NULL
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(job_id)
    .bind(now)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    Ok(None)
}

#[derive(Debug, Clone)]
pub struct RawOutputRow {
    pub feature_id: String,
    pub attributes: Value,
    pub srid: Option<i32>,
    pub total_count: i64,
    pub total_positions: i64,
    pub wkb: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ExecutionInputs {
    pub job_id: Uuid,
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub algorithm_id: String,
    pub input_refs: Value,
    pub parameters: Value,
    pub output_name: String,
    pub engine_version: String,
    pub input_srid: i32,
    pub output_family: String,
    pub output_fields: Vec<(String, String, String, bool)>,
    pub raw_rows: Vec<RawOutputRow>,
}

pub enum ExecutionLockState {
    Terminal(Value),
    Ready(ExecutionInputs),
}

fn web_transform_expr(
    expression: &str,
    source_srid: i32,
    target_srid: i32,
) -> Result<(String, &'static str, i32), EditError> {
    let profile = u32::try_from(source_srid)
        .ok()
        .zip(u32::try_from(target_srid).ok())
        .and_then(|(src, dst)| reference_transformation(src, dst))
        .filter(|p| {
            matches!((source_srid, target_srid), (4326, 3857) | (3857, 4326))
                && p.fixture_requirement == FixtureRequirement::None
                && p.steps.len() == 1
        })
        .ok_or_else(|| {
            EditError::validation(
                "UNSUPPORTED_TRANSFORMATION",
                "no approved transformation for this operation; use a supported native CRS",
            )
        })?;
    let step = profile.steps[0];
    let fn_name = if step.inverse {
        "ST_InverseTransformPipeline"
    } else {
        "ST_TransformPipeline"
    };
    Ok((
        format!(
            "{fn_name}({expression}, '{}', {target_srid})",
            step.pipeline
        ),
        step.pipeline,
        target_srid,
    ))
}

async fn approved_reproject(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: Uuid,
    project: Uuid,
    source: Uuid,
    native: i32,
    target: i32,
) -> Result<String, EditError> {
    if let (Ok(a), Ok(b)) = (u32::try_from(native), u32::try_from(target)) {
        if ovg_core::crs::is_extended_native_vn2000(a) && b == 4326 {
            return crate::catalog::regional_crs::verify_extended_datum_source(
                tx,
                org,
                project,
                source,
                a,
                "rf.geometry",
            )
            .await
            .map_err(|error| EditError::Platform {
                status: error.status_code(),
                code: error.code(),
                message: error.to_string(),
                details: None,
            });
        }
        if crate::native_projection::is_native_pair(a, b) {
            let map_error = |error: ovg_core::AppError| EditError::Platform {
                status: error.status_code(),
                code: error.code(),
                message: error.to_string(),
                details: None,
            };
            let projection =
                crate::native_projection::NativeProjection::new(a, b).map_err(map_error)?;
            projection
                .verify_source(tx, org, project, source)
                .await
                .map_err(map_error)?;
            return Ok(projection.expression("rf.geometry"));
        }
    }
    if native == target && matches!(native, 4326 | 3857) {
        return Ok("rf.geometry".to_owned());
    }
    let profile = u32::try_from(native)
        .ok()
        .zip(u32::try_from(target).ok())
        .and_then(|(a, b)| reference_transformation(a, b))
        .ok_or_else(|| {
            EditError::validation(
                "UNSUPPORTED_TRANSFORMATION",
                "no approved directed CRS operation for this pair",
            )
        })?;
    if profile.fixture_requirement == FixtureRequirement::None {
        let (expr, _, _) = web_transform_expr("rf.geometry", native, target)?;
        require_web_domain(
            tx,
            org,
            project,
            WebDomainCheck {
                source_id: source,
                native_srid: native,
                expression: "geometry",
                expression_srid: native,
                margin: 0.0,
            },
        )
        .await?;
        Ok(expr)
    } else {
        // The immutable source chain is replayed before each datum operation.
        let profile =
            crate::catalog::regional_crs::verify_regional_source(tx, org, project, source, profile)
                .await
                .map_err(|error| EditError::Platform {
                    status: error.status_code(),
                    code: error.code(),
                    message: error.to_string(),
                    details: None,
                })?;
        Ok(crate::catalog::regional_crs::profile_expression(
            profile,
            "rf.geometry",
        ))
    }
}

struct WebDomainCheck<'a> {
    source_id: Uuid,
    native_srid: i32,
    expression: &'a str,
    expression_srid: i32,
    margin: f64,
}

async fn require_web_domain(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
    project_id: Uuid,
    check: WebDomainCheck<'_>,
) -> Result<(), EditError> {
    let (min_x, min_y, max_x, max_y) = match check.expression_srid {
        4326 => (
            -180.0_f64 + check.margin,
            -WEB_MERCATOR_MAX_LATITUDE + check.margin,
            180.0_f64 - check.margin,
            WEB_MERCATOR_MAX_LATITUDE - check.margin,
        ),
        3857 => (
            -WEB_MERCATOR_MAX_COORDINATE_METRE + check.margin,
            -WEB_MERCATOR_MAX_COORDINATE_METRE + check.margin,
            WEB_MERCATOR_MAX_COORDINATE_METRE - check.margin,
            WEB_MERCATOR_MAX_COORDINATE_METRE - check.margin,
        ),
        _ => {
            return Err(EditError::validation(
                "UNSUPPORTED_TRANSFORMATION",
                "unsupported transform CRS",
            ))
        }
    };
    let expr = check.expression;
    let sql = format!(
        "SELECT EXISTS (
            SELECT 1
            FROM spatial.reference_features
            WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
            AND (
                CASE WHEN geometry_state = 'present' AND geometry IS NOT NULL
                    AND ST_SRID(geometry) = $4 AND ST_IsValid(geometry)
                    AND ST_Zmflag(geometry) = 0 AND NOT ST_IsEmpty(geometry)
                THEN (
                    ST_XMin({expr}) >= $5
                    AND ST_XMax({expr}) <= $6
                    AND ST_YMin({expr}) >= $7
                    AND ST_YMax({expr}) <= $8
                ) ELSE false END
            ) IS NOT TRUE
        )"
    );
    let invalid: bool = sqlx::query_scalar(&sql)
        .bind(org_id)
        .bind(project_id)
        .bind(check.source_id)
        .bind(check.native_srid)
        .bind(min_x)
        .bind(max_x)
        .bind(min_y)
        .bind(max_y)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| {
            EditError::validation(
                "PROCESSING_FAILED",
                "processing could not safely publish output",
            )
        })?;
    if invalid {
        return Err(EditError::validation(
            "TRANSFORMATION_OUT_OF_AREA",
            "geometry is outside the verified transformation domain",
        ));
    }
    Ok(())
}

/// Lock the job row `FOR UPDATE`, check terminal/quarantine states, validate pins,
/// mark `running`, and fetch raw PostGIS output rows inside a savepoint.
pub async fn lock_and_prepare_execution(
    scope: &mut EditScope<'_>,
    job_id: Uuid,
) -> Result<ExecutionLockState, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let row = sqlx::query(
        "SELECT * FROM processing.jobs
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         FOR UPDATE",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(job_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?
    .ok_or_else(|| EditError::not_found("processing job not found"))?;

    let error_code: Option<String> = row.try_get("error_code").map_err(edit_db_error)?;
    if error_code.as_deref() == Some("RECOVERY_QUARANTINED") {
        return Err(EditError::conflict(
            "RECOVERY_QUARANTINED",
            "historical restored processing job cannot resume; submit a new operation",
        ));
    }
    let status: String = row.try_get("status").map_err(edit_db_error)?;
    if matches!(status.as_str(), "succeeded" | "cancelled") {
        return Ok(ExecutionLockState::Terminal(row_wire_edit(&row)?));
    }

    let algorithm_id: String = row.try_get("algorithm_id").map_err(edit_db_error)?;
    let input_refs: Value = row.try_get("input_refs").map_err(edit_db_error)?;
    let parameters: Value = row.try_get("parameters").map_err(edit_db_error)?;
    let output_name: String = row.try_get("output_name").map_err(edit_db_error)?;
    let engine_version: String = row.try_get("engine_version").map_err(edit_db_error)?;

    let mut sp = scope.tx_mut().begin().await.map_err(edit_db_error)?;
    let outcome = prepare_inside_savepoint(
        &mut sp,
        ProcessingJobArgs {
            job_id,
            org_id,
            project_id,
            algorithm_id,
            input_refs,
            parameters,
            output_name,
            engine_version,
        },
    )
    .await;

    match outcome {
        Ok(inputs) => {
            sp.commit().await.map_err(edit_db_error)?;
            Ok(ExecutionLockState::Ready(inputs))
        }
        Err(err) => {
            sp.rollback().await.map_err(edit_db_error)?;
            scope.rebind_project_context().await?;
            let (code, message) = failure_code_message(&err);
            let failed = fail_job(scope, job_id, &code, &message).await?;
            Ok(ExecutionLockState::Terminal(failed))
        }
    }
}

type ProcessingField = (String, String, String, bool);

struct ProcessingJobArgs {
    job_id: Uuid,
    org_id: Uuid,
    project_id: Uuid,
    algorithm_id: String,
    input_refs: Value,
    parameters: Value,
    output_name: String,
    engine_version: String,
}

async fn processing_source_fields(
    sp: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: Uuid,
    project: Uuid,
    source: Uuid,
) -> Result<Vec<ProcessingField>, EditError> {
    sqlx::query_as::<_, ProcessingField>(
        "SELECT field_id,name,type,nullable FROM spatial.source_fields
         WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 ORDER BY ordinal,field_id",
    )
    .bind(org)
    .bind(project)
    .bind(source)
    .fetch_all(&mut **sp)
    .await
    .map_err(edit_db_error)
}

fn processing_raw_row(
    row: &PgRow,
    feature_id: String,
    attributes: Value,
) -> Result<RawOutputRow, EditError> {
    Ok(RawOutputRow {
        feature_id,
        attributes,
        srid: row.try_get("srid").map_err(edit_db_error)?,
        total_count: row
            .try_get::<Option<i64>, _>("total_count")
            .map_err(edit_db_error)?
            .unwrap_or(0),
        total_positions: row
            .try_get::<Option<i64>, _>("total_positions")
            .map_err(edit_db_error)?
            .unwrap_or(0),
        wkb: row.try_get("wkb").map_err(edit_db_error)?,
    })
}

async fn prepare_inside_savepoint(
    sp: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    args: ProcessingJobArgs,
) -> Result<ExecutionInputs, EditError> {
    let ProcessingJobArgs {
        job_id,
        org_id,
        project_id,
        algorithm_id,
        input_refs,
        parameters,
        output_name,
        engine_version,
    } = args;
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(PROCESSING_STATEMENT_TIMEOUT)
        .execute(&mut **sp)
        .await
        .map_err(edit_db_error)?;

    let input_source = validate_pinned_source(sp, org_id, project_id, &input_refs, "input").await?;
    if overlay_algorithm(&algorithm_id) {
        let overlay_source =
            validate_pinned_source(sp, org_id, project_id, &input_refs, "overlay").await?;
        if overlay_source.id == input_source.id
            || (algorithm_id != "spatial_join" && overlay_source.geometry_family != "Polygon")
            || (algorithm_id == "union" && input_source.geometry_family != "Polygon")
        {
            return Err(EditError::validation(
                "INVALID_OVERLAY",
                "overlay requires distinct compatible sources; union requires two Polygon layers",
            ));
        }
    }

    sqlx::query(
        "UPDATE processing.jobs
         SET status = 'running', updated_at = $4, completed_at = NULL
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(job_id)
    .bind(now_micros())
    .execute(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    let input_src_id = Uuid::parse_str(
        input_refs
            .get("input_source_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .map_err(|_| EditError::conflict("STALE_INPUT_PIN", "invalid input source pin"))?;
    let input_srid = input_refs
        .get("input_stored_crs")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| EditError::conflict("STALE_INPUT_PIN", "invalid input crs pin"))?;

    let max_rows = MAX_PROCESSING_FEATURES + 1;
    let mut raw_rows = Vec::new();
    let mut output_family = input_source.geometry_family.clone();
    let output_fields;
    let mut output_srid = input_srid;

    let geom_cols = RESULT_GEOMETRY_COLUMNS
        .replace("$max_features", &MAX_PROCESSING_FEATURES.to_string())
        .replace("$max_positions", &MAX_PROCESSING_POSITIONS.to_string());

    match algorithm_id.as_str() {
        "buffer" => {
            let dist = parameters
                .get("distance_meters")
                .and_then(Value::as_f64)
                .ok_or_else(|| {
                    EditError::validation("INVALID_PARAMETERS", "missing distance_meters")
                })?;
            let quad_segs = parameters
                .get("quad_segs")
                .and_then(Value::as_i64)
                .unwrap_or(8);
            let buf_expr = if matches!(input_srid, 4326 | 4756) {
                let (projected, _, _) = web_transform_expr("geometry", input_srid, 3857)?;
                require_web_domain(
                    sp,
                    org_id,
                    project_id,
                    WebDomainCheck {
                        source_id: input_src_id,
                        native_srid: input_srid,
                        expression: "geometry",
                        expression_srid: input_srid,
                        margin: 0.0,
                    },
                )
                .await?;
                require_web_domain(
                    sp,
                    org_id,
                    project_id,
                    WebDomainCheck {
                        source_id: input_src_id,
                        native_srid: input_srid,
                        expression: &projected,
                        expression_srid: 3857,
                        margin: dist,
                    },
                )
                .await?;
                let (proj_rf, _, _) = web_transform_expr("rf.geometry", input_srid, 3857)?;
                let (inv_expr, _, _) =
                    web_transform_expr(&format!("ST_Buffer({proj_rf}, $4, $5)"), 3857, input_srid)?;
                inv_expr
            } else {
                "ST_Buffer(rf.geometry, $4, $5)".to_owned()
            };

            let sql = format!(
                "WITH dumped AS (
                    SELECT rf.feature_id, part.geom, part.path
                    FROM spatial.reference_features rf
                    CROSS JOIN LATERAL ST_Dump(ST_MakeValid({buf_expr})) AS part
                    WHERE rf.organization_id = $1 AND rf.project_id = $2
                      AND rf.source_id = $3
                 )
                 SELECT feature_id, {geom_cols}
                 FROM dumped
                 WHERE NOT ST_IsEmpty(geom)
                 ORDER BY feature_id, path LIMIT $6"
            );
            let qrows = sqlx::query(&sql)
                .bind(org_id)
                .bind(project_id)
                .bind(input_src_id)
                .bind(dist)
                .bind(format!("quad_segs={quad_segs}"))
                .bind(max_rows)
                .fetch_all(&mut **sp)
                .await
                .map_err(|_| {
                    EditError::validation(
                        "PROCESSING_FAILED",
                        "processing could not safely publish output",
                    )
                })?;

            output_family = "Polygon".to_owned();
            output_fields = vec![
                (
                    "source_feature_id".into(),
                    "Source Feature ID".into(),
                    "text".into(),
                    false,
                ),
                (
                    "buffer_meters".into(),
                    "Buffer Distance (m)".into(),
                    "decimal".into(),
                    false,
                ),
            ];
            let dist_str = py_float(dist);
            for (idx, qr) in qrows.iter().enumerate() {
                let src_fid: String = qr.try_get("feature_id").map_err(edit_db_error)?;
                raw_rows.push(RawOutputRow {
                    feature_id: format!("buf_{}", idx + 1),
                    attributes: json!({
                        "source_feature_id": src_fid,
                        "buffer_meters": dist_str,
                    }),
                    srid: qr.try_get("srid").map_err(edit_db_error)?,
                    total_count: qr
                        .try_get::<Option<i64>, _>("total_count")
                        .map_err(edit_db_error)?
                        .unwrap_or(0),
                    total_positions: qr
                        .try_get::<Option<i64>, _>("total_positions")
                        .map_err(edit_db_error)?
                        .unwrap_or(0),
                    wkb: qr.try_get("wkb").map_err(edit_db_error)?,
                });
            }
        }
        "clip" | "difference" | "intersection" => {
            let overlay_src_id = Uuid::parse_str(
                input_refs
                    .get("overlay_source_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|_| EditError::conflict("STALE_INPUT_PIN", "invalid overlay source pin"))?;
            let overlay_srid = input_refs
                .get("overlay_stored_crs")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| EditError::conflict("STALE_INPUT_PIN", "invalid overlay crs pin"))?;
            let transformed = if overlay_srid == input_srid {
                "ov.geometry".to_owned()
            } else {
                let (transformed, _, _) =
                    web_transform_expr("ov.geometry", overlay_srid, input_srid)?;
                require_web_domain(
                    sp,
                    org_id,
                    project_id,
                    WebDomainCheck {
                        source_id: overlay_src_id,
                        native_srid: overlay_srid,
                        expression: "geometry",
                        expression_srid: overlay_srid,
                        margin: 0.0,
                    },
                )
                .await?;
                transformed
            };
            let sql = if algorithm_id == "intersection" {
                format!(
                    "WITH mask AS (
                        SELECT ov.feature_id AS overlay_id, {transformed} AS mgeom
                        FROM spatial.reference_features ov
                        WHERE ov.organization_id = $1 AND ov.project_id = $2
                          AND ov.source_id = $4
                     ), clipped AS (
                        SELECT rf.feature_id, mask.overlay_id, part.geom, part.path
                        FROM spatial.reference_features rf JOIN mask
                          ON ST_Intersects(rf.geometry, mask.mgeom)
                        CROSS JOIN LATERAL ST_Dump(ST_Intersection(rf.geometry, mask.mgeom)) AS part
                        WHERE rf.organization_id = $1 AND rf.project_id = $2
                          AND rf.source_id = $3
                     )
                     SELECT feature_id, overlay_id, {geom_cols}
                     FROM clipped
                     WHERE ST_GeometryType(geom) = $5 AND NOT ST_IsEmpty(geom)
                     ORDER BY feature_id, overlay_id, path LIMIT $6"
                )
            } else {
                let geom = if algorithm_id == "difference" {
                    "CASE WHEN mask.mgeom IS NULL THEN rf.geometry ELSE ST_Difference(rf.geometry, mask.mgeom) END"
                } else {
                    "ST_Intersection(rf.geometry, mask.mgeom)"
                };
                let condition = if algorithm_id == "difference" {
                    "TRUE"
                } else {
                    "mask.mgeom IS NOT NULL AND ST_Intersects(rf.geometry, mask.mgeom)"
                };
                format!(
                    "WITH mask AS (
                        SELECT ST_Union({transformed}) AS mgeom
                        FROM spatial.reference_features ov
                        WHERE ov.organization_id = $1 AND ov.project_id = $2
                          AND ov.source_id = $4
                     ), clipped AS (
                        SELECT rf.feature_id, part.geom, part.path
                        FROM spatial.reference_features rf CROSS JOIN mask
                        CROSS JOIN LATERAL ST_Dump({geom}) AS part
                        WHERE rf.organization_id = $1 AND rf.project_id = $2
                          AND rf.source_id = $3 AND ({condition})
                     )
                     SELECT feature_id, NULL::text AS overlay_id, {geom_cols}
                     FROM clipped
                     WHERE ST_GeometryType(geom) = $5 AND NOT ST_IsEmpty(geom)
                     ORDER BY feature_id, path LIMIT $6"
                )
            };
            let qrows = sqlx::query(&sql)
                .bind(org_id)
                .bind(project_id)
                .bind(input_src_id)
                .bind(overlay_src_id)
                .bind(format!("ST_{output_family}"))
                .bind(max_rows)
                .fetch_all(&mut **sp)
                .await
                .map_err(|_| {
                    EditError::validation(
                        "PROCESSING_FAILED",
                        "processing could not safely publish output",
                    )
                })?;

            let mut fields = vec![(
                "source_feature_id".into(),
                "Source Feature ID".into(),
                "text".into(),
                false,
            )];
            if algorithm_id == "intersection" {
                fields.push((
                    "overlay_feature_id".into(),
                    "Overlay Feature ID".into(),
                    "text".into(),
                    false,
                ));
            }
            output_fields = fields;
            for (idx, qr) in qrows.iter().enumerate() {
                let src_fid: String = qr.try_get("feature_id").map_err(edit_db_error)?;
                let attributes = if algorithm_id == "intersection" {
                    let overlay_fid: String = qr.try_get("overlay_id").map_err(edit_db_error)?;
                    json!({ "source_feature_id": src_fid, "overlay_feature_id": overlay_fid })
                } else {
                    json!({ "source_feature_id": src_fid })
                };
                raw_rows.push(RawOutputRow {
                    feature_id: format!("{algorithm_id}_{}", idx + 1),
                    attributes,
                    srid: qr.try_get("srid").map_err(edit_db_error)?,
                    total_count: qr
                        .try_get::<Option<i64>, _>("total_count")
                        .map_err(edit_db_error)?
                        .unwrap_or(0),
                    total_positions: qr
                        .try_get::<Option<i64>, _>("total_positions")
                        .map_err(edit_db_error)?
                        .unwrap_or(0),
                    wkb: qr.try_get("wkb").map_err(edit_db_error)?,
                });
            }
        }
        "reproject" => {
            output_srid = parameters
                .get("target_srid")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .filter(|v| {
                    u32::try_from(*v)
                        .ok()
                        .is_some_and(ovg_core::crs::is_processing_crs)
                })
                .ok_or_else(|| {
                    EditError::validation("INVALID_PARAMETERS", "unsupported target CRS")
                })?;
            let expr = approved_reproject(
                sp,
                org_id,
                project_id,
                input_src_id,
                input_srid,
                output_srid,
            )
            .await?;
            output_fields = processing_source_fields(sp, org_id, project_id, input_src_id).await?;
            let sql = format!(
                "WITH transformed AS (
                   SELECT feature_id, attributes, {expr} AS geom
                   FROM spatial.reference_features rf
                   WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
                 ) SELECT feature_id, attributes, {geom_cols}
                   FROM transformed ORDER BY feature_id LIMIT $4"
            );
            let rows = sqlx::query(&sql)
                .bind(org_id)
                .bind(project_id)
                .bind(input_src_id)
                .bind(max_rows)
                .fetch_all(&mut **sp)
                .await
                .map_err(edit_db_error)?;
            for row in rows {
                raw_rows.push(processing_raw_row(
                    &row,
                    row.try_get("feature_id").map_err(edit_db_error)?,
                    row.try_get("attributes").map_err(edit_db_error)?,
                )?);
            }
        }
        "union" | "spatial_join" => {
            let overlay_src_id = Uuid::parse_str(
                input_refs
                    .get("overlay_source_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .map_err(|_| EditError::conflict("STALE_INPUT_PIN", "invalid overlay source pin"))?;
            let overlay_srid = input_refs
                .get("overlay_stored_crs")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| EditError::conflict("STALE_INPUT_PIN", "invalid overlay crs pin"))?;
            let transformed = if overlay_srid == input_srid {
                "ov.geometry".to_owned()
            } else {
                let (expr, _, _) = web_transform_expr("ov.geometry", overlay_srid, input_srid)?;
                require_web_domain(
                    sp,
                    org_id,
                    project_id,
                    WebDomainCheck {
                        source_id: overlay_src_id,
                        native_srid: overlay_srid,
                        expression: "geometry",
                        expression_srid: overlay_srid,
                        margin: 0.0,
                    },
                )
                .await?;
                expr
            };
            let input_fields =
                processing_source_fields(sp, org_id, project_id, input_src_id).await?;
            let overlay_fields =
                processing_source_fields(sp, org_id, project_id, overlay_src_id).await?;
            let mut fields = vec![
                (
                    "source_feature_id".into(),
                    "Source Feature ID".into(),
                    "text".into(),
                    true,
                ),
                (
                    "overlay_feature_id".into(),
                    "Overlay Feature ID".into(),
                    "text".into(),
                    true,
                ),
            ];
            for (prefix, original) in [("input", &input_fields), ("overlay", &overlay_fields)] {
                for (i, (_, name, kind, _)) in original.iter().enumerate() {
                    fields.push((
                        format!("{prefix}_{i}"),
                        format!("{prefix} {}: {name}", i + 1)
                            .chars()
                            .take(200)
                            .collect(),
                        kind.clone(),
                        true,
                    ));
                }
            }
            if fields.len() > 100 {
                return Err(EditError::validation(
                    "PROCESSING_LIMIT",
                    "combined schema exceeds 100 fields",
                ));
            }
            output_fields = fields;
            let cte = format!("WITH a AS (
                SELECT feature_id AS aid, attributes AS aa, geometry AS ag
                FROM spatial.reference_features WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
              ), b AS (
                SELECT feature_id AS bid, attributes AS ba, {transformed} AS bg
                FROM spatial.reference_features ov WHERE organization_id=$1 AND project_id=$2 AND source_id=$4
              )");
            let sql = if algorithm_id == "union" {
                format!(
                    "{cte}, am AS (SELECT ST_Union(ag) AS g FROM a),
                    bm AS (SELECT ST_Union(bg) AS g FROM b),
                    pieces AS (
                      SELECT aid, bid, aa, ba, ST_Intersection(ag,bg) AS g
                      FROM a JOIN b ON ST_Intersects(ag,bg)
                      UNION ALL
                      SELECT aid, NULL::text, aa, NULL::jsonb,
                        CASE WHEN bm.g IS NULL THEN ag ELSE ST_Difference(ag,bm.g) END
                      FROM a CROSS JOIN bm
                      UNION ALL
                      SELECT NULL::text, bid, NULL::jsonb, ba,
                        CASE WHEN am.g IS NULL THEN bg ELSE ST_Difference(bg,am.g) END
                      FROM b CROSS JOIN am
                    ), dumped AS (
                      SELECT aid,bid,aa,ba,part.geom,part.path
                      FROM pieces CROSS JOIN LATERAL ST_Dump(g) part
                    ) SELECT aid,bid,aa,ba,{geom_cols} FROM dumped
                      WHERE ST_GeometryType(geom)='ST_Polygon' AND NOT ST_IsEmpty(geom)
                      ORDER BY aid NULLS FIRST,bid NULLS FIRST,path LIMIT $5"
                )
            } else {
                let predicate = match parameters.get("predicate").and_then(Value::as_str) {
                    Some("intersects") => "ST_Intersects",
                    Some("within") => "ST_Within",
                    Some("contains") => "ST_Contains",
                    Some("touches") => "ST_Touches",
                    Some("overlaps") => "ST_Overlaps",
                    Some("crosses") => "ST_Crosses",
                    Some("equals") => "ST_Equals",
                    _ => {
                        return Err(EditError::validation(
                            "INVALID_PARAMETERS",
                            "unsupported spatial predicate",
                        ))
                    }
                };
                let join = match parameters.get("join_type").and_then(Value::as_str) {
                    Some("left") => "LEFT JOIN",
                    Some("inner") => "JOIN",
                    _ => {
                        return Err(EditError::validation(
                            "INVALID_PARAMETERS",
                            "unsupported join type",
                        ))
                    }
                };
                let limit = match parameters.get("match_mode").and_then(Value::as_str) {
                    Some("first") => "LIMIT 1",
                    Some("all") => "",
                    _ => {
                        return Err(EditError::validation(
                            "INVALID_PARAMETERS",
                            "unsupported match mode",
                        ))
                    }
                };
                format!("{cte}, joined AS (
                    SELECT aid, hit.bid, aa, hit.ba, ag AS geom
                    FROM a {join} LATERAL (
                      SELECT bid,ba FROM b WHERE {predicate}(ag,bg) ORDER BY bid {limit}
                    ) hit ON true
                ) SELECT aid,bid,aa,ba,{geom_cols} FROM joined ORDER BY aid,bid NULLS FIRST LIMIT $5")
            };
            let rows = sqlx::query(&sql)
                .bind(org_id)
                .bind(project_id)
                .bind(input_src_id)
                .bind(overlay_src_id)
                .bind(max_rows)
                .fetch_all(&mut **sp)
                .await
                .map_err(edit_db_error)?;
            for (i, row) in rows.iter().enumerate() {
                let aid: Option<String> = row.try_get("aid").map_err(edit_db_error)?;
                let bid: Option<String> = row.try_get("bid").map_err(edit_db_error)?;
                let aa: Option<Value> = row.try_get("aa").map_err(edit_db_error)?;
                let ba: Option<Value> = row.try_get("ba").map_err(edit_db_error)?;
                let mut attrs = Map::new();
                attrs.insert("source_feature_id".into(), json!(aid));
                attrs.insert("overlay_feature_id".into(), json!(bid));
                for (prefix, original, values) in [
                    ("input", &input_fields, aa.as_ref()),
                    ("overlay", &overlay_fields, ba.as_ref()),
                ] {
                    for (j, (id, _, _, _)) in original.iter().enumerate() {
                        attrs.insert(
                            format!("{prefix}_{j}"),
                            values
                                .and_then(|v| v.get(id))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                    }
                }
                raw_rows.push(processing_raw_row(
                    row,
                    format!("{algorithm_id}_{}", i + 1),
                    Value::Object(attrs),
                )?);
            }
        }
        "dissolve" => {
            let group_field = parameters
                .get("group_by_field")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(ref gf) = group_field {
                let exists: Option<i32> = sqlx::query_scalar(
                    "SELECT 1 FROM spatial.source_fields
                     WHERE organization_id = $1 AND project_id = $2
                       AND source_id = $3 AND field_id = $4",
                )
                .bind(org_id)
                .bind(project_id)
                .bind(input_src_id)
                .bind(gf)
                .fetch_optional(&mut **sp)
                .await
                .map_err(edit_db_error)?;
                if exists.is_none() {
                    return Err(EditError::validation(
                        "UNKNOWN_FIELD",
                        format!("unknown dissolve group_by_field '{gf}'"),
                    ));
                }
            }
            let sql = if group_field.is_some() {
                format!(
                    "WITH grouped AS (
                        SELECT rf.attributes ->> CAST($4 AS text) AS grp_val,
                               COUNT(*)::int AS merged_count,
                               ST_MakeValid(ST_Union(rf.geometry)) AS ugeom
                        FROM spatial.reference_features rf
                        WHERE rf.organization_id = $1 AND rf.project_id = $2
                          AND rf.source_id = $3
                        GROUP BY 1
                     ),
                     dumped AS (
                        SELECT grp_val, merged_count, part.geom, part.path
                        FROM grouped CROSS JOIN LATERAL ST_Dump(ugeom) AS part
                     )
                     SELECT grp_val, merged_count, {geom_cols}
                     FROM dumped
                     WHERE NOT ST_IsEmpty(geom)
                     ORDER BY grp_val NULLS FIRST, path LIMIT $5"
                )
            } else {
                format!(
                    "WITH grouped AS (
                        SELECT 'all'::text AS grp_val,
                               COUNT(*)::int AS merged_count,
                               ST_MakeValid(ST_Union(rf.geometry)) AS ugeom
                        FROM spatial.reference_features rf
                        WHERE rf.organization_id = $1 AND rf.project_id = $2
                          AND rf.source_id = $3
                        GROUP BY 1
                     ),
                     dumped AS (
                        SELECT grp_val, merged_count, part.geom, part.path
                        FROM grouped CROSS JOIN LATERAL ST_Dump(ugeom) AS part
                     )
                     SELECT grp_val, merged_count, {geom_cols}
                     FROM dumped
                     WHERE NOT ST_IsEmpty(geom)
                     ORDER BY grp_val NULLS FIRST, path LIMIT $4"
                )
            };
            let qrows = if let Some(ref gf) = group_field {
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(input_src_id)
                    .bind(gf)
                    .bind(max_rows)
                    .fetch_all(&mut **sp)
                    .await
            } else {
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(input_src_id)
                    .bind(max_rows)
                    .fetch_all(&mut **sp)
                    .await
            }
            .map_err(|_| {
                EditError::validation(
                    "PROCESSING_FAILED",
                    "processing could not safely publish output",
                )
            })?;

            output_fields = vec![
                (
                    "group_value".into(),
                    "Group Value".into(),
                    "text".into(),
                    true,
                ),
                (
                    "merged_count".into(),
                    "Merged Feature Count".into(),
                    "integer".into(),
                    false,
                ),
            ];
            for (idx, qr) in qrows.iter().enumerate() {
                let grp_val: Option<String> = qr.try_get("grp_val").map_err(edit_db_error)?;
                let merged_count: i32 = qr.try_get("merged_count").map_err(edit_db_error)?;
                raw_rows.push(RawOutputRow {
                    feature_id: format!("dis_{}", idx + 1),
                    attributes: json!({
                        "group_value": grp_val,
                        "merged_count": merged_count,
                    }),
                    srid: qr.try_get("srid").map_err(edit_db_error)?,
                    total_count: qr
                        .try_get::<Option<i64>, _>("total_count")
                        .map_err(edit_db_error)?
                        .unwrap_or(0),
                    total_positions: qr
                        .try_get::<Option<i64>, _>("total_positions")
                        .map_err(edit_db_error)?
                        .unwrap_or(0),
                    wkb: qr.try_get("wkb").map_err(edit_db_error)?,
                });
            }
        }
        _ => {
            return Err(EditError::validation(
                "INVALID_ALGORITHM",
                format!("unsupported algorithm '{algorithm_id}'"),
            ));
        }
    }

    Ok(ExecutionInputs {
        job_id,
        organization_id: org_id,
        project_id,
        algorithm_id,
        input_refs,
        parameters,
        output_name,
        engine_version,
        input_srid: output_srid,
        output_family,
        output_fields,
        raw_rows,
    })
}

#[derive(Debug, Clone)]
pub struct ValidatedFeatureRow {
    pub feature_id: String,
    pub attributes: Value,
    pub wkb: Vec<u8>,
    pub geometry: Value,
}

#[derive(Debug, Clone)]
pub struct CpuValidatedOutput {
    pub out_source_id: Uuid,
    pub out_layer_id: Uuid,
    pub manifest_digest: String,
    pub schema_rev: String,
    pub content_rev: String,
    pub schema_fields: Vec<FieldSpec>,
    pub rows: Vec<ValidatedFeatureRow>,
}

/// CPU-bound Rayon step: validate geometry/bounds, canonicalize attributes, and compute R003 pins.
pub fn validate_and_hash_cpu(
    inputs: &ExecutionInputs,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<CpuValidatedOutput, EditError> {
    let out_source_id = Uuid::new_v4();
    let out_layer_id = Uuid::new_v4();
    let dummy_rev = "0".repeat(64);

    let schema_fields: Vec<FieldSpec> = inputs
        .output_fields
        .iter()
        .enumerate()
        .map(|(ordinal, (fid, fname, ftype, nullable))| FieldSpec {
            field_id: fid.clone(),
            name: fname.clone(),
            ordinal: ordinal as i32,
            field_type: ftype.clone(),
            nullable: *nullable,
            schema_revision: dummy_rev.clone(),
        })
        .collect();

    let field_defs: Vec<FieldDef> = schema_fields
        .iter()
        .map(|f| FieldDef {
            field_id: f.field_id.clone(),
            field_type: f.field_type.clone(),
            nullable: f.nullable,
            schema_revision: dummy_rev.clone(),
        })
        .collect();

    let expected_keys: HashMap<&str, &FieldDef> = field_defs
        .iter()
        .map(|f| (f.field_id.as_str(), f))
        .collect();

    let mut rows = Vec::with_capacity(inputs.raw_rows.len());
    let mut pin_inputs = Vec::with_capacity(inputs.raw_rows.len());

    for raw in &inputs.raw_rows {
        if is_cancelled() {
            return Err(EditError::conflict(
                "CANCELLED",
                "job cancelled during execution",
            ));
        }
        if raw.total_count > MAX_PROCESSING_FEATURES
            || raw.total_positions > MAX_PROCESSING_POSITIONS
        {
            return Err(EditError::validation(
                "PROCESSING_LIMIT",
                "processing output exceeds resource bounds",
            ));
        }
        let Some(wkb) = raw.wkb.as_ref() else {
            return Err(EditError::validation(
                "INVALID_DERIVED_OUTPUT",
                "invalid derived output geometry",
            ));
        };
        if raw.srid != Some(inputs.input_srid) {
            return Err(EditError::validation(
                "INVALID_DERIVED_OUTPUT",
                "invalid derived output geometry",
            ));
        }
        let geometry =
            validate_ready_wkb(wkb, &inputs.output_family, inputs.input_srid).map_err(|_| {
                EditError::validation("INVALID_DERIVED_OUTPUT", "invalid derived output geometry")
            })?;

        let raw_obj = raw.attributes.as_object().ok_or_else(|| {
            EditError::validation(
                "INVALID_DERIVED_OUTPUT",
                "invalid derived output attributes",
            )
        })?;
        if raw_obj.len() != expected_keys.len()
            || raw_obj
                .keys()
                .any(|k| !expected_keys.contains_key(k.as_str()))
        {
            return Err(EditError::validation(
                "INVALID_DERIVED_OUTPUT",
                "invalid derived output attributes",
            ));
        }
        let mut canonical_obj = Map::new();
        for f in &field_defs {
            let val = raw_obj.get(&f.field_id).unwrap_or(&Value::Null);
            let canon = canonical_attribute(f, val).map_err(|_| {
                EditError::validation(
                    "INVALID_DERIVED_OUTPUT",
                    "invalid derived output attributes",
                )
            })?;
            canonical_obj.insert(f.field_id.clone(), canon);
        }
        let canonical_attrs = Value::Object(canonical_obj);
        pin_inputs.push(FeaturePinInput {
            feature_id: raw.feature_id.clone(),
            attributes: canonical_attrs.clone(),
            geometry: geometry.clone(),
        });
        rows.push(ValidatedFeatureRow {
            feature_id: raw.feature_id.clone(),
            attributes: canonical_attrs,
            wkb: wkb.clone(),
            geometry,
        });
    }

    let manifest_digest = sha256_py_json(&json!({
        "job_id": inputs.job_id.to_string(),
        "refs": inputs.input_refs,
        "params": inputs.parameters,
    }));
    let schema_rev = schema_revision_digest(&schema_fields, &inputs.output_family, "XY", "known");
    let content_rev = content_revision_digest(
        &pin_inputs,
        &schema_fields,
        inputs.input_srid,
        inputs.input_srid,
        &inputs.output_family,
        "XY",
    );

    Ok(CpuValidatedOutput {
        out_source_id,
        out_layer_id,
        manifest_digest,
        schema_rev,
        content_rev,
        schema_fields,
        rows,
    })
}

/// Atomically publish the derived source/layer/features/audit/catalog_changes inside a savepoint,
/// or durably mark the job `failed` on any failure.
pub async fn publish_execution(
    scope: &mut EditScope<'_>,
    inputs: &ExecutionInputs,
    output: &CpuValidatedOutput,
) -> Result<Value, EditError> {
    let actor = scope_actor(scope);
    let mut sp = scope.tx_mut().begin().await.map_err(edit_db_error)?;
    let result = publish_inside_savepoint(&mut sp, actor, inputs, output).await;
    match result {
        Ok(wire) => {
            sp.commit().await.map_err(edit_db_error)?;
            Ok(wire)
        }
        Err(err) => {
            sp.rollback().await.map_err(edit_db_error)?;
            scope.rebind_project_context().await?;
            let (code, message) = failure_code_message(&err);
            fail_job(scope, inputs.job_id, &code, &message).await
        }
    }
}

struct ActorMeta {
    actor_type: &'static str,
    actor_id: Uuid,
    display_name: String,
}

fn scope_actor(scope: &EditScope<'_>) -> ActorMeta {
    ActorMeta {
        actor_type: scope.actor_kind(),
        actor_id: scope.actor_id(),
        display_name: scope.display_name().to_owned(),
    }
}

async fn publish_inside_savepoint(
    sp: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: ActorMeta,
    inputs: &ExecutionInputs,
    output: &CpuValidatedOutput,
) -> Result<Value, EditError> {
    let org_id = inputs.organization_id;
    let project_id = inputs.project_id;
    let now = now_micros();
    let short_hex: String = output
        .out_source_id
        .simple()
        .to_string()
        .chars()
        .take(12)
        .collect();
    let ref_key = format!("proc_{}_{short_hex}", inputs.algorithm_id);
    let fver = format!("{}@{}", inputs.algorithm_id, inputs.engine_version);

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
            $6, $7, NULL, 'active',
            'ready', NULL, $8, $8,
            'known', $9, 'XY', 'known',
            0, 'empty', NULL, 1,
            $10, $11, $12, $12, NULL
         )",
    )
    .bind(output.out_source_id)
    .bind(org_id)
    .bind(project_id)
    .bind(&inputs.output_name)
    .bind(&ref_key)
    .bind(&fver)
    .bind(&output.manifest_digest)
    .bind(inputs.input_srid)
    .bind(&inputs.output_family)
    .bind(&output.content_rev)
    .bind(&output.schema_rev)
    .bind(now)
    .execute(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    for field in &output.schema_fields {
        sqlx::query(
            "INSERT INTO spatial.source_fields (
                organization_id, project_id, source_id, field_id,
                name, ordinal, type, nullable, schema_revision
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(output.out_source_id)
        .bind(&field.field_id)
        .bind(&field.name)
        .bind(field.ordinal)
        .bind(&field.field_type)
        .bind(field.nullable)
        .bind(&output.schema_rev)
        .execute(&mut **sp)
        .await
        .map_err(edit_db_error)?;
    }

    for row in &output.rows {
        sqlx::query(
            "INSERT INTO spatial.reference_features (
                organization_id, project_id, source_id, feature_id,
                attributes, geometry, geometry_state, content_revision, schema_revision
             ) VALUES (
                $1, $2, $3, $4,
                $5, ST_GeomFromWKB($6, $7), 'present', $8, $9
             )",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(output.out_source_id)
        .bind(&row.feature_id)
        .bind(&row.attributes)
        .bind(&row.wkb)
        .bind(inputs.input_srid)
        .bind(&output.content_rev)
        .bind(&output.schema_rev)
        .execute(&mut **sp)
        .await
        .map_err(edit_db_error)?;
    }

    if !output.rows.is_empty() {
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
        .bind(output.out_source_id)
        .fetch_one(&mut **sp)
        .await
        .map_err(edit_db_error)?;
        let cnt: i32 = stats.try_get("cnt").map_err(edit_db_error)?;
        let extent = json!({
            "min_x": stats.try_get::<f64, _>("min_x").map_err(edit_db_error)?,
            "min_y": stats.try_get::<f64, _>("min_y").map_err(edit_db_error)?,
            "max_x": stats.try_get::<f64, _>("max_x").map_err(edit_db_error)?,
            "max_y": stats.try_get::<f64, _>("max_y").map_err(edit_db_error)?,
            "crs_code": inputs.input_srid,
        });
        sqlx::query(
            "UPDATE spatial.data_sources
             SET feature_count = $4, extent_state = 'available', extent = $5
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(output.out_source_id)
        .bind(cnt)
        .bind(extent)
        .execute(&mut **sp)
        .await
        .map_err(edit_db_error)?;
    }

    sqlx::query(
        "INSERT INTO spatial.layers (
            id, organization_id, project_id, source_id, name,
            lifecycle, layer_revision, created_at, updated_at, retired_at
         ) VALUES ($1, $2, $3, $4, $5, 'active', 1, $6, $6, NULL)",
    )
    .bind(output.out_layer_id)
    .bind(org_id)
    .bind(project_id)
    .bind(output.out_source_id)
    .bind(&inputs.output_name)
    .bind(now)
    .execute(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    let audit_id = Uuid::new_v4();
    let actor_label: String = if actor.display_name.is_empty() {
        actor.actor_id.to_string()
    } else {
        actor.display_name.chars().take(200).collect()
    };
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, $3, $4, $5, 'processing.job.succeeded', 'data_source', $6, 'success', $7, $8)",
    )
    .bind(audit_id)
    .bind(org_id)
    .bind(actor.actor_type)
    .bind(actor.actor_id)
    .bind(&actor_label)
    .bind(output.out_source_id)
    .bind(json!({
        "job_id": inputs.job_id.to_string(),
        "algorithm_id": inputs.algorithm_id,
        "feature_count": output.rows.len(),
    }))
    .bind(now)
    .execute(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    let name_change = json!([{
        "field": "name",
        "old_value": Value::Null,
        "new_value": inputs.output_name,
    }]);
    sqlx::query(
        "INSERT INTO spatial.catalog_changes (
            id, organization_id, project_id, resource_type, resource_id,
            source_id, layer_id, actor_type, actor_id, actor_display_snapshot,
            operation, created_at, audit_event_id, prior_revision, new_revision, changes
         ) VALUES (
            $1, $2, $3, 'data_source', $4,
            $4, NULL, $5, $6, $7,
            'processing_publish', $8, $9, NULL, $10, $11
         )",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(project_id)
    .bind(output.out_source_id)
    .bind(actor.actor_type)
    .bind(actor.actor_id)
    .bind(&actor_label)
    .bind(now)
    .bind(audit_id)
    .bind(json!({
        "catalog_revision": 1,
        "content_revision": output.content_rev,
        "schema_revision": output.schema_rev,
    }))
    .bind(&name_change)
    .execute(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    sqlx::query(
        "INSERT INTO spatial.catalog_changes (
            id, organization_id, project_id, resource_type, resource_id,
            source_id, layer_id, actor_type, actor_id, actor_display_snapshot,
            operation, created_at, audit_event_id, prior_revision, new_revision, changes
         ) VALUES (
            $1, $2, $3, 'layer', $4,
            NULL, $4, $5, $6, $7,
            'create_layer', $8, $9, NULL, $10, $11
         )",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(project_id)
    .bind(output.out_layer_id)
    .bind(actor.actor_type)
    .bind(actor.actor_id)
    .bind(&actor_label)
    .bind(now)
    .bind(audit_id)
    .bind(json!({ "layer_revision": 1 }))
    .bind(&name_change)
    .execute(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    let done_at = now_micros();
    let done_row = sqlx::query(
        "UPDATE processing.jobs
         SET status = 'succeeded',
             output_source_id = $4,
             output_layer_id = $5,
             output_content_revision = $6,
             feature_count = $7,
             updated_at = $8,
             completed_at = $8
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(inputs.job_id)
    .bind(output.out_source_id)
    .bind(output.out_layer_id)
    .bind(&output.content_rev)
    .bind(i32::try_from(output.rows.len()).unwrap_or(i32::MAX))
    .bind(done_at)
    .fetch_one(&mut **sp)
    .await
    .map_err(edit_db_error)?;

    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut **sp)
        .await
        .map_err(edit_db_error)?;

    row_wire_edit(&done_row)
}

pub async fn fail_job(
    scope: &mut EditScope<'_>,
    job_id: Uuid,
    code: &str,
    message: &str,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let code_clipped: String = code.chars().take(64).collect();
    let msg_clipped: String = message.chars().take(500).collect();
    let now = now_micros();
    let row = sqlx::query(
        "UPDATE processing.jobs
         SET status = 'failed', error_code = $4, error_message = $5,
             output_source_id = NULL, output_layer_id = NULL,
             output_content_revision = NULL, feature_count = NULL,
             updated_at = $6, completed_at = $6
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(job_id)
    .bind(code_clipped)
    .bind(msg_clipped)
    .bind(now)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_error)?;
    row_wire_edit(&row)
}

#[must_use]
pub fn failure_code_message(err: &EditError) -> (String, String) {
    match err {
        EditError::Platform { code, message, .. } => ((*code).to_owned(), message.clone()),
        _ => (
            "PROCESSING_FAILED".to_owned(),
            "processing could not safely publish output".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_semantics_match_python_repository() {
        for status in ["queued", "running", "failed"] {
            assert!(validate_cancel_status(status).is_ok(), "{status}");
        }
        for status in ["succeeded", "cancelled"] {
            let error = validate_cancel_status(status).unwrap_err();
            assert_eq!(error.code(), "JOB_TERMINAL");
            assert_eq!(
                error.to_string(),
                format!("cannot cancel job in status {status}")
            );
        }
    }

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T01:02:03.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T01:02:03.123456+00:00");
    }
}
