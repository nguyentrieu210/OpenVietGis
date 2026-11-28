//! E07 import/export and durable transfer job routes (R005), matching the FastAPI wire
//! in `apps/api/openvietgis_api/adapters/http/transfers.py`.
//!
//! GeoJSON is native end to end; GeoPackage and ZIP Shapefile imports use a
//! bounded codec worker with Rust confirmation and PostGIS validation. Binary
//! exports use the same worker with explicit conversion disclosures.

mod codec;
pub(crate) mod store;

use std::collections::HashMap;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Duration;
use ovg_storage::transfers::{
    now_micros, Asset, JobUpdate, NewArtifact, NewImportedSource, NewJob, TransferError,
    TransferScope,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use self::codec::{
    build_geojson_inspection, build_native_inspection, canonical_attribute, content_digest,
    detect_format, extent_value, fingerprint, is_ready, plan_export_mapping, push_geojson_feature,
    sanitize_filename, schema_digest, sorted_unique, Field, InspectOptions, Inspection, Mapping,
    SchemaField, Transform, GEOJSON_HEAD, GEOJSON_TAIL, MAX_TRANSFER_FEATURES, MAX_UPLOAD_BYTES,
};
use self::store::{Staged, Store};
use crate::identity::HumanSession;
use crate::AppState;

const PAGE_ROWS: i64 = 1000;
const FLUSH_BYTES: usize = 64 * 1024;
/// Multipart envelope allowance on top of the 25 MiB file limit.
const MULTIPART_SLACK: usize = 64 * 1024;

pub(crate) fn router() -> Router<AppState> {
    const P: &str = "/organizations/{organization_id}/projects/{project_id}";
    Router::new()
        .route(&format!("{P}/imports/upload"), post(upload_import_asset))
        .route(&format!("{P}/imports/inspect"), post(inspect_import_asset))
        .route(&format!("{P}/imports/confirm"), post(confirm_import))
        .route(&format!("{P}/exports/preview"), post(preview_export))
        .route(&format!("{P}/exports/confirm"), post(confirm_export))
        .route(&format!("{P}/jobs"), get(list_transfer_jobs))
        .route(&format!("{P}/jobs/{{job_id}}"), get(get_transfer_job))
        .route(
            &format!("{P}/jobs/{{job_id}}/cancel"),
            post(cancel_transfer_job),
        )
        .route(
            &format!("{P}/jobs/{{job_id}}/retry"),
            post(retry_transfer_job),
        )
        .route(
            &format!("{P}/artifacts/{{artifact_id}}"),
            get(get_derived_artifact),
        )
        .route(
            &format!("{P}/artifacts/{{artifact_id}}/download"),
            get(download_derived_artifact),
        )
}

// ----- errors -----

pub(crate) struct Fail(TransferError);

impl From<TransferError> for Fail {
    fn from(error: TransferError) -> Self {
        Self(error)
    }
}

impl From<ovg_core::AppError> for Fail {
    fn from(error: ovg_core::AppError) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for Fail {
    fn into_response(self) -> Response {
        let status =
            |code: u16| StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        match self.0 {
            TransferError::Coded {
                status: code,
                code: error_code,
                message,
            } => (
                status(code),
                Json(json!({"code": error_code, "message": message})),
            )
                .into_response(),
            TransferError::Detail {
                status: code,
                detail,
            } => (status(code), Json(json!({ "detail": detail }))).into_response(),
            TransferError::ReadOnly => (
                StatusCode::CONFLICT,
                Json(
                    json!({"code": "PROJECT_NOT_WRITABLE", "detail": "The project is read-only."}),
                ),
            )
                .into_response(),
            TransferError::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            TransferError::Internal(detail) => {
                tracing::error!(%detail, "transfer request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}

type Reply = Result<Response, Fail>;

fn invalid() -> Fail {
    Fail(TransferError::InvalidRequest)
}

// ----- request prelude: auth -> project guard -> CSRF -> validation -> authority -----

type Params = Path<HashMap<String, String>>;

fn param(params: &HashMap<String, String>, key: &str) -> Option<Uuid> {
    params.get(key).and_then(|v| Uuid::parse_str(v).ok())
}

fn csrf_valid(state: &AppState, auth: &HumanSession, headers: &HeaderMap) -> bool {
    let Some(secret) = state.config.csrf_secret.as_deref() else {
        return false;
    };
    let Some(candidate) = headers
        .get("x-openvietgis-csrf")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
    else {
        return false;
    };
    auth.csrf_matches(candidate, secret)
}

/// FastAPI order: authentication (extractor), `project_access_guard`, `require_csrf`.
async fn prelude<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    headers: &HeaderMap,
    params: &HashMap<String, String>,
    post: bool,
) -> Result<TransferScope<'a>, Fail> {
    let scope = match (
        param(params, "organization_id"),
        param(params, "project_id"),
    ) {
        (Some(org), Some(project)) => {
            let pool = state
                .db_pool
                .as_ref()
                .ok_or_else(|| TransferError::detail(503, "database unavailable"))?;
            Some(
                TransferScope::guard(pool, org, project, auth.actor_id(), auth.session_id(), post)
                    .await?,
            )
        }
        _ => None,
    };
    if post && !csrf_valid(state, auth, headers) {
        return Err(TransferError::detail(403, "CSRF validation failed").into());
    }
    scope.ok_or_else(invalid)
}

async fn reopen<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    ids: Ids,
    write: bool,
) -> Result<TransferScope<'a>, TransferError> {
    let pool = state
        .db_pool
        .as_ref()
        .ok_or_else(|| TransferError::detail(503, "database unavailable"))?;
    TransferScope::open(
        pool,
        ids.0,
        ids.1,
        auth.actor_id(),
        auth.session_id(),
        write,
    )
    .await
}

type Ids = (Uuid, Uuid);

fn body<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, Fail> {
    serde_json::from_slice(bytes).map_err(|_| invalid())
}

fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn chars_between(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.chars().count())
}

/// Python `str.strip()` whitespace.
fn py_strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

async fn inspect_payload(
    payload: &[u8],
    format: &str,
    opts: &InspectOptions<'_>,
) -> Result<Inspection, TransferError> {
    if format == "geojson" {
        return build_geojson_inspection(payload, opts);
    }
    let decoded = crate::connectors::transfer_worker::run_import(payload,json!({"action":"inspect_import","format":format,"member":opts.selected_member,"encoding":opts.declared_encoding})).await
        .map_err(|e| TransferError::coded(e.status.as_u16(),e.code.unwrap_or("VECTOR_CODEC_FAILED"), if e.status.is_server_error() {"Vector decoder unavailable or resource budget exceeded.".into()} else {e.message}))?;
    build_native_inspection(decoded, format, opts)
}

fn too_large() -> TransferError {
    TransferError::coded(
        413,
        "UPLOAD_SIZE_EXCEEDED",
        format!("Uploaded file exceeds maximum allowed size of {MAX_UPLOAD_BYTES} bytes."),
    )
}

// ----- imports -----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportInspectBody {
    source_asset_id: Uuid,
    selected_member: Option<String>,
    declared_source_crs: Option<i64>,
    target_crs: Option<i64>,
    declared_encoding: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportConfirmBody {
    operation_id: Uuid,
    source_asset_id: Uuid,
    selected_member: String,
    source_name: String,
    layer_name: String,
    declared_source_crs: Option<i64>,
    target_crs: Option<i64>,
    declared_encoding: Option<String>,
    #[serde(default)]
    approved_conversions: Vec<String>,
    inspection_fingerprint: String,
}

async fn upload_import_asset(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
    request_body: Body,
) -> Reply {
    let mut scope = prelude(&state, &auth, &headers, &params, true).await?;
    let bytes = axum::body::to_bytes(request_body, MAX_UPLOAD_BYTES + MULTIPART_SLACK)
        .await
        .map_err(|_| too_large())?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let (filename, payload) = store::multipart_file(content_type, &bytes).ok_or_else(invalid)?;
    if payload.len() > MAX_UPLOAD_BYTES {
        return Err(too_large().into());
    }
    scope.authorize(true).await?;
    if payload.is_empty() {
        return Err(TransferError::validation("EMPTY_UPLOAD", "Uploaded file is empty.").into());
    }
    let format = detect_format(payload)?;
    let raw_name = filename.filter(|n| !n.is_empty()).unwrap_or("upload.bin");
    let asset_id = Uuid::new_v4();
    let storage_key = format!(
        "assets/{}/{}/{asset_id}.bin",
        scope.organization_id(),
        scope.project_id()
    );
    let store = Store::new(&state);
    let (sha256, byte_size) = store.put(&storage_key, payload).await?;
    let asset = Asset {
        id: asset_id,
        filename: sanitize_filename(raw_name, &format!("upload.{format}")),
        detected_format: format.to_string(),
        byte_size,
        sha256,
        storage_key: storage_key.clone(),
        uploaded_by_display: scope.display_name().to_string(),
        created_at: now_micros(),
    };
    let inspection = match inspect_payload(
        payload,
        format,
        &inspect_options(&asset, None, None, None, None),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            store.delete(&storage_key).await;
            return Err(e.into());
        }
    };
    let wire = scope.asset_wire(&asset);
    let persisted = match scope.create_source_asset(&asset).await {
        Ok(()) => scope.commit().await,
        Err(error) => Err(error),
    };
    if let Err(error) = persisted {
        store.delete(&storage_key).await;
        return Err(error.into());
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({"asset": wire, "inspection": inspection.wire})),
    )
        .into_response())
}

fn inspect_options<'a>(
    asset: &'a Asset,
    selected_member: Option<&'a str>,
    declared_source_crs: Option<i64>,
    target_crs: Option<i64>,
    declared_encoding: Option<&'a str>,
) -> InspectOptions<'a> {
    InspectOptions {
        asset_id: asset.id,
        asset_sha256: &asset.sha256,
        selected_member,
        declared_source_crs,
        target_crs,
        declared_encoding,
    }
}

async fn load_asset(
    state: &AppState,
    scope: &mut TransferScope<'_>,
    id: Uuid,
    missing: &str,
) -> Result<(Asset, Vec<u8>), TransferError> {
    let asset = scope
        .get_source_asset(id)
        .await?
        .ok_or_else(|| TransferError::not_found(missing))?;
    let payload = Store::new(state)
        .read_verified(&asset.storage_key, &asset.sha256)
        .await?;
    Ok((asset, payload))
}

async fn inspect_import_asset(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
    raw: Bytes,
) -> Reply {
    let mut scope = prelude(&state, &auth, &headers, &params, true).await?;
    let request: ImportInspectBody = body(&raw)?;
    scope.authorize(true).await?;
    let (asset, payload) = load_asset(
        &state,
        &mut scope,
        request.source_asset_id,
        "Source asset not found.",
    )
    .await?;
    let inspection = inspect_payload(
        &payload,
        &asset.detected_format,
        &inspect_options(
            &asset,
            request.selected_member.as_deref(),
            request.declared_source_crs,
            request.target_crs,
            request.declared_encoding.as_deref(),
        ),
    )
    .await?;
    Ok(
        Json(json!({"asset": scope.asset_wire(&asset), "inspection": inspection.wire}))
            .into_response(),
    )
}

async fn confirm_import(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
    raw: Bytes,
) -> Reply {
    let mut scope = prelude(&state, &auth, &headers, &params, true).await?;
    let request: ImportConfirmBody = body(&raw)?;
    if !chars_between(&request.selected_member, 1, 200)
        || !chars_between(&request.source_name, 1, 200)
        || !chars_between(&request.layer_name, 1, 200)
        || !hex64(&request.inspection_fingerprint)
    {
        return Err(invalid());
    }
    scope.authorize(true).await?;
    let ids = (scope.organization_id(), scope.project_id());
    let (asset, payload) = load_asset(
        &state,
        &mut scope,
        request.source_asset_id,
        "Source asset not found.",
    )
    .await?;
    let inspection = inspect_payload(
        &payload,
        &asset.detected_format,
        &inspect_options(
            &asset,
            Some(&request.selected_member),
            request.declared_source_crs,
            request.target_crs,
            request.declared_encoding.as_deref(),
        ),
    )
    .await?;
    if inspection.fingerprint != request.inspection_fingerprint {
        return Err(TransferError::conflict(
            "STALE_INSPECTION",
            "Import inspection fingerprint does not match current asset or options.",
        )
        .into());
    }
    let source_name = py_strip(&request.source_name).to_string();
    let layer_name = py_strip(&request.layer_name).to_string();
    let approved = sorted_unique(&request.approved_conversions);
    let request_fp = fingerprint(&json!({
        "inspection_fingerprint": inspection.fingerprint,
        "source_name": source_name,
        "layer_name": layer_name,
        "approved_conversions": approved,
    }));
    if let Some(existing) = scope.get_job_by_operation(request.operation_id).await? {
        if existing.request_fingerprint != request_fp {
            return Err(TransferError::conflict(
                "OPERATION_INTENT_CONFLICT",
                "This import operation_id was already used with a different request.",
            )
            .into());
        }
        return Ok(Json(existing.wire).into_response());
    }
    if !inspection.confirmable {
        let (code, message) = inspection
            .first_error
            .clone()
            .unwrap_or(("IMPORT_NOT_CONFIRMABLE", "Import validation failed.".into()));
        return Err(TransferError::validation(code, message).into());
    }
    let missing: Vec<&str> = inspection
        .conversions
        .iter()
        .filter(|c| c.requires_approval && !approved.iter().any(|a| a == c.code))
        .map(|c| c.code)
        .collect();
    if !missing.is_empty() {
        let mut missing = missing;
        missing.sort_unstable();
        missing.dedup();
        return Err(TransferError::validation(
            "UNAPPROVED_CONVERSION",
            format!(
                "Explicit approval required for conversions: {}.",
                missing.join(", ")
            ),
        )
        .into());
    }
    let job_id = Uuid::new_v4();
    let now = now_micros();
    scope
        .create_job(&NewJob {
            id: job_id,
            operation_id: request.operation_id,
            kind: "import",
            request_fingerprint: request_fp,
            source_asset_id: Some(asset.id),
            target_source_id: None,
            target_layer_id: None,
            source_content_revision: None,
            source_schema_revision: None,
            options: json!({
                "selected_member": request.selected_member,
                "source_name": source_name,
                "layer_name": layer_name,
                "declared_source_crs": request.declared_source_crs,
                "target_crs": inspection.target_crs,
                "declared_encoding": inspection.resolved_encoding,
                "approved_conversions": approved,
            }),
            inspection_report: inspection.wire.clone(),
            created_at: now,
        })
        .await?;
    scope.record_attempt(job_id, 1, now).await?;
    scope.commit().await?;
    let job = execute_import(
        &state,
        &auth,
        ids,
        job_id,
        &asset,
        inspection,
        &source_name,
        &layer_name,
    )
    .await?;
    Ok(Json(job).into_response())
}

/// Record the failed attempt like `_execute_*_attempt` and return the original error.
async fn fail_job(
    state: &AppState,
    auth: &HumanSession,
    ids: Ids,
    job_id: Uuid,
    error: TransferError,
    interrupted: (&'static str, &'static str),
) -> Fail {
    let update = match &error {
        TransferError::Coded { code, message, .. } => JobUpdate {
            status: "failed",
            error_code: Some(*code),
            error_message: Some(message.chars().take(500).collect()),
            completed_at: Some(now_micros()),
            ..JobUpdate::default()
        },
        _ => JobUpdate {
            status: "recoverable_failed",
            error_code: Some(interrupted.0),
            error_message: Some(interrupted.1.to_string()),
            ..JobUpdate::default()
        },
    };
    let recorded = async {
        let mut scope = reopen(state, auth, ids, false).await?;
        scope.update_job(job_id, update).await?;
        scope.commit().await
    }
    .await;
    match recorded {
        Ok(()) => Fail(error),
        Err(update_error) => {
            tracing::error!(?error, "transfer failure could not be recorded");
            Fail(update_error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_import(
    state: &AppState,
    auth: &HumanSession,
    ids: Ids,
    job_id: Uuid,
    asset: &Asset,
    inspection: Inspection,
    source_name: &str,
    layer_name: &str,
) -> Result<Value, Fail> {
    match run_import(
        state,
        auth,
        ids,
        job_id,
        asset,
        inspection,
        source_name,
        layer_name,
    )
    .await
    {
        Ok(job) => Ok(job),
        Err(error) => Err(fail_job(
            state,
            auth,
            ids,
            job_id,
            error,
            (
                "IMPORT_EXECUTION_INTERRUPTED",
                "Import execution did not complete; retry with fresh authorization.",
            ),
        )
        .await),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_import(
    state: &AppState,
    auth: &HumanSession,
    ids: Ids,
    job_id: Uuid,
    asset: &Asset,
    inspection: Inspection,
    source_name: &str,
    layer_name: &str,
) -> Result<Value, TransferError> {
    let spatial = || TransferError::internal("imported value failed spatial domain validation");
    let native = inspection.wire["resolved_source_crs"]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| TransferError::internal("confirmed import has no source CRS"))?;
    let target = inspection
        .target_crs
        .and_then(|t| i32::try_from(t).ok())
        .ok_or_else(|| TransferError::internal("confirmed import has no target CRS"))?;
    let member = inspection.selected_member.clone().unwrap_or_default();
    let parsed = inspection
        .parsed
        .as_ref()
        .ok_or_else(|| TransferError::internal("confirmed import has no parsed member"))?;
    if let Some(finding) = parsed.findings.first() {
        return Err(TransferError::validation(
            finding.code,
            finding.message.clone(),
        ));
    }
    if parsed.records.len() > MAX_TRANSFER_FEATURES {
        return Err(TransferError::coded(
            413,
            "FEATURE_LIMIT_EXCEEDED",
            format!("Feature count exceeds limit of {MAX_TRANSFER_FEATURES}."),
        ));
    }
    let family = parsed.family;
    let mut scope = reopen(state, auth, ids, true).await?;
    for (index, record) in parsed.records.iter().enumerate() {
        if !is_ready(&record.geom, Some(family), native, false) {
            return Err(TransferError::validation(
                "INVALID_GEOMETRY",
                format!("Feature at index {index} failed geometry validation."),
            ));
        }
    }
    let transform = Transform::resolve(native, target).map_err(|_| {
        TransferError::validation(
            "UNSUPPORTED_TRANSFORMATION",
            format!("No approved transformation from EPSG:{native} to EPSG:{target}."),
        )
    })?;
    let steps = transform.as_ref().map_or(&[][..], |t| t.steps.as_slice());
    let mut geometries = Vec::with_capacity(parsed.records.len());
    for (batch, chunk) in parsed.records.chunks(2000).enumerate() {
        let wkts: Vec<String> = chunk.iter().map(|r| r.geom.wkt()).collect();
        let rows = scope.check_and_transform(&wkts, native, steps).await?;
        if rows.len() != chunk.len() {
            return Err(TransferError::internal("geometry batch size mismatch"));
        }
        for (offset, ((empty, row), record)) in rows.iter().zip(chunk).enumerate() {
            if *empty || !row.valid {
                return Err(TransferError::validation(
                    "INVALID_GEOMETRY",
                    format!(
                        "Feature at index {} failed PostGIS ST_IsValid.",
                        batch * 2000 + offset
                    ),
                ));
            }
            geometries.push(match &transform {
                None => record.geom.clone(),
                Some(t) => t.apply(&record.geom, family, &row.wkb, &row.step_valid)?,
            });
        }
    }

    let fields: Vec<Field> = parsed
        .fields
        .iter()
        .map(|f| {
            let name = py_strip(&f.name).to_string();
            if !chars_between(&name, 1, 200) || name.contains('\0') {
                return Err(spatial());
            }
            Ok(Field { name, ..f.clone() })
        })
        .collect::<Result<_, _>>()?;
    let mut features = Vec::with_capacity(geometries.len());
    let mut bounds: Option<[f64; 4]> = None;
    for (record, geom) in parsed.records.iter().zip(geometries) {
        if !is_ready(&geom, Some(family), target, true) {
            return Err(spatial());
        }
        let mut attrs = Map::new();
        for (field, value) in fields.iter().zip(&record.attrs) {
            attrs.insert(
                field.field_id.clone(),
                canonical_attribute(field, value).ok_or_else(spatial)?,
            );
        }
        for [x, y] in geom.positions() {
            bounds = Some(match bounds {
                None => [x, y, x, y],
                Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
            });
        }
        features.push((record.feature_id.clone(), attrs, geom));
    }
    let schema_revision = schema_digest(&fields, family);
    let content_revision = content_digest(&fields, &features, native, target, family);
    let source_id = Uuid::new_v4();
    let layer_id = Uuid::new_v4();
    let short = |id: Uuid| id.simple().to_string()[..12].to_string();
    let result_report = json!({
        "asset_id": asset.id.to_string(),
        "asset_sha256": asset.sha256,
        "format": asset.detected_format,
        "selected_member": member,
        "source_id": source_id.to_string(),
        "layer_id": layer_id.to_string(),
        "content_revision": content_revision,
        "schema_revision": schema_revision,
        "feature_count": features.len(),
        "source_crs": native,
        "stored_crs": target,
        "transformation_operation_id": transform.as_ref().map(|t| t.operation_id),
        "encoding": inspection.resolved_encoding,
    });
    let field_rows = Value::Array(
        fields
            .iter()
            .map(|f| {
                json!({
                    "field_id": f.field_id,
                    "name": f.name,
                    "ordinal": f.ordinal,
                    "type": f.field_type,
                    "nullable": f.nullable,
                })
            })
            .collect(),
    );
    let feature_rows: Vec<Value> = features
        .iter()
        .map(|(id, attrs, geom)| {
            json!({
                "feature_id": id,
                "attributes": attrs,
                "wkt": geom.wkt(),
                "geometry_state": "present",
            })
        })
        .collect();
    let source = NewImportedSource {
        id: source_id,
        name: source_name.to_string(),
        reference_key: format!("imp_{}_{}", short(asset.id), short(source_id)),
        fixture_version: format!("{}:{member}", asset.detected_format)
            .chars()
            .take(100)
            .collect(),
        manifest_digest: asset.sha256.clone(),
        source_crs: native,
        stored_crs: target,
        geometry_family: family,
        feature_count: i64::try_from(features.len()).unwrap_or(i64::MAX),
        extent: bounds.map(|b| extent_value(b, target)),
        content_revision,
        schema_revision,
        created_at: now_micros(),
    };
    if source_name.is_empty() || layer_name.is_empty() {
        return Err(spatial());
    }
    let job = scope
        .publish_imported_source(
            job_id,
            asset.id,
            &source,
            &field_rows,
            &feature_rows,
            layer_id,
            layer_name,
            result_report,
        )
        .await?;
    scope.commit().await?;
    Ok(job.wire)
}

// ----- exports -----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportPreviewBody {
    layer_id: Uuid,
    content_revision: String,
    schema_revision: String,
    output_format: String,
    output_crs: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportConfirmBody {
    operation_id: Uuid,
    layer_id: Uuid,
    content_revision: String,
    schema_revision: String,
    output_format: String,
    output_crs: i64,
    #[serde(default)]
    approved_disclosures: Vec<String>,
    preview_fingerprint: String,
}

fn valid_export(content: &str, schema: &str, format: &str) -> bool {
    hex64(content) && hex64(schema) && matches!(format, "geopackage" | "geojson" | "shapefile_zip")
}

struct Preview {
    wire: Value,
    fingerprint: String,
    source_id: Uuid,
    required_approvals: Vec<&'static str>,
    mappings: Vec<Mapping>,
    operation_id: Option<&'static str>,
}

async fn build_preview(
    scope: &mut TransferScope<'_>,
    layer_id: Uuid,
    content: &str,
    schema: &str,
    format: &str,
    output_crs: i64,
) -> Result<Preview, TransferError> {
    let source = scope
        .load_export_source(layer_id, content, schema, false)
        .await?;
    let stored = source.stored_crs.ok_or_else(|| {
        TransferError::validation("MISSING_CRS", "Source has no verified stored CRS.")
    })?;
    if format != "geojson" && source.feature_count > MAX_TRANSFER_FEATURES as i64 {
        return Err(TransferError::coded(
            413,
            "FEATURE_LIMIT_EXCEEDED",
            "Binary export supports up to 50000 features.",
        ));
    }
    let fields: Vec<SchemaField<'_>> = source
        .fields
        .iter()
        .map(|f| (f.field_id.as_str(), f.name.as_str(), f.field_type.as_str()))
        .collect();
    let (mappings, disclosures, operation_id) = plan_export_mapping(
        &fields,
        format,
        stored,
        i32::try_from(output_crs).unwrap_or(0),
        source.has_long_text,
    )
    .map_err(|message| TransferError::validation("UNSUPPORTED_EXPORT_PROFILE", message))?;
    let codes: Vec<&str> = disclosures.iter().map(|d| d.code).collect();
    let fp = fingerprint(&json!({
        "kind": "export",
        "layer_id": layer_id.to_string(),
        "source_id": source.source_id.to_string(),
        "content_revision": content,
        "schema_revision": schema,
        "output_format": format,
        "output_crs": output_crs,
        "disclosures": codes,
    }));
    let wire = json!({
        "layer_id": layer_id,
        "source_id": source.source_id,
        "content_revision": content,
        "schema_revision": schema,
        "output_format": format,
        "source_crs": stored,
        "output_crs": output_crs,
        "transformation_operation_id": operation_id,
        "feature_count": source.feature_count,
        "geometry_family": source.geometry_family,
        "field_mappings": mappings.iter().map(Mapping::wire).collect::<Vec<_>>(),
        "disclosures": disclosures.iter().map(codec::Conversion::wire).collect::<Vec<_>>(),
        "confirmable": true,
        "preview_fingerprint": fp,
    });
    Ok(Preview {
        wire,
        fingerprint: fp,
        source_id: source.source_id,
        required_approvals: disclosures
            .iter()
            .filter(|d| d.requires_approval)
            .map(|d| d.code)
            .collect(),
        mappings,
        operation_id,
    })
}

async fn preview_export(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
    raw: Bytes,
) -> Reply {
    let mut scope = prelude(&state, &auth, &headers, &params, true).await?;
    let request: ExportPreviewBody = body(&raw)?;
    if !valid_export(
        &request.content_revision,
        &request.schema_revision,
        &request.output_format,
    ) {
        return Err(invalid());
    }
    scope.authorize(false).await?;
    let preview = build_preview(
        &mut scope,
        request.layer_id,
        &request.content_revision,
        &request.schema_revision,
        &request.output_format,
        request.output_crs,
    )
    .await?;
    Ok(Json(preview.wire).into_response())
}

async fn confirm_export(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
    raw: Bytes,
) -> Reply {
    let mut scope = prelude(&state, &auth, &headers, &params, true).await?;
    let request: ExportConfirmBody = body(&raw)?;
    if !valid_export(
        &request.content_revision,
        &request.schema_revision,
        &request.output_format,
    ) || !hex64(&request.preview_fingerprint)
    {
        return Err(invalid());
    }
    scope.authorize(false).await?;
    let ids = (scope.organization_id(), scope.project_id());
    let preview = build_preview(
        &mut scope,
        request.layer_id,
        &request.content_revision,
        &request.schema_revision,
        &request.output_format,
        request.output_crs,
    )
    .await?;
    if preview.fingerprint != request.preview_fingerprint {
        return Err(TransferError::conflict(
            "STALE_PREVIEW",
            "Export preview fingerprint does not match current revision or options.",
        )
        .into());
    }
    let approved = sorted_unique(&request.approved_disclosures);
    let request_fp = fingerprint(&json!({
        "preview_fingerprint": preview.fingerprint,
        "approved_disclosures": approved,
    }));
    if let Some(existing) = scope.get_job_by_operation(request.operation_id).await? {
        if existing.request_fingerprint != request_fp {
            return Err(TransferError::conflict(
                "OPERATION_INTENT_CONFLICT",
                "This export operation_id was already used with a different request.",
            )
            .into());
        }
        return Ok(Json(existing.wire).into_response());
    }
    let mut missing: Vec<&str> = preview
        .required_approvals
        .iter()
        .copied()
        .filter(|code| !approved.iter().any(|a| a.as_str() == *code))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        return Err(TransferError::validation(
            "UNAPPROVED_LOSS",
            format!(
                "Explicit approval required for export differences: {}.",
                missing.join(", ")
            ),
        )
        .into());
    }
    let job_id = Uuid::new_v4();
    let now = now_micros();
    scope
        .create_job(&NewJob {
            id: job_id,
            operation_id: request.operation_id,
            kind: "export",
            request_fingerprint: request_fp,
            source_asset_id: None,
            target_source_id: Some(preview.source_id),
            target_layer_id: Some(request.layer_id),
            source_content_revision: Some(request.content_revision.clone()),
            source_schema_revision: Some(request.schema_revision.clone()),
            options: json!({
                "output_format": request.output_format,
                "output_crs": request.output_crs,
                "approved_disclosures": approved,
            }),
            inspection_report: preview.wire.clone(),
            created_at: now,
        })
        .await?;
    scope.record_attempt(job_id, 1, now).await?;
    scope.commit().await?;
    match run_export(&state, &auth, ids, job_id, &request, &preview, &approved).await {
        Ok(job) => Ok(Json(job).into_response()),
        Err(error) => Err(fail_job(
            &state,
            &auth,
            ids,
            job_id,
            error,
            (
                "EXPORT_EXECUTION_INTERRUPTED",
                "Export did not complete; retry with fresh authorization.",
            ),
        )
        .await),
    }
}

async fn run_export(
    state: &AppState,
    auth: &HumanSession,
    ids: Ids,
    job_id: Uuid,
    request: &ExportConfirmBody,
    preview: &Preview,
    approved: &[String],
) -> Result<Value, TransferError> {
    let mut scope = reopen(state, auth, ids, false).await?;
    let (content, schema) = (&request.content_revision, &request.schema_revision);
    let source = scope
        .load_export_source(request.layer_id, content, schema, true)
        .await?;
    let stored = source
        .stored_crs
        .ok_or_else(|| TransferError::internal("export source lost its CRS"))?;
    let output_crs = i32::try_from(request.output_crs)
        .map_err(|_| TransferError::internal("export CRS outside range"))?;
    let transform = Transform::resolve(stored, output_crs).map_err(|_| {
        TransferError::validation(
            "UNSUPPORTED_TRANSFORMATION",
            format!("No approved transformation from EPSG:{stored} to EPSG:{output_crs}."),
        )
    })?;
    let artifact_id = Uuid::new_v4();
    let (format, ext, media) = match request.output_format.as_str() {
        "geojson" => ("geojson", "geojson", "application/geo+json"),
        "geopackage" => ("geopackage", "gpkg", "application/geopackage+sqlite3"),
        "shapefile_zip" => ("shapefile_zip", "zip", "application/zip"),
        _ => {
            return Err(TransferError::validation(
                "UNSUPPORTED_FORMAT",
                "Unsupported export format.",
            ))
        }
    };
    let storage_key = format!("artifacts/{}/{}/{artifact_id}.{ext}", ids.0, ids.1);
    let store = Store::new(state);
    let (sha256, byte_size, null_counts) = if format == "geojson" {
        let mut staged = store.stage(&storage_key).await?;
        let counts = match stream_geojson(
            &mut scope,
            &source,
            content,
            schema,
            transform.as_ref(),
            &preview.mappings,
            &mut staged,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                staged.abort().await;
                return Err(e);
            }
        };
        let (sha, size) = staged.finish().await?;
        (sha, size, counts)
    } else {
        let (features, counts) =
            collect_binary_features(&mut scope, &source, content, schema, transform.as_ref())
                .await?;
        let mappings=preview.mappings.iter().map(|m|json!({"field_id":m.field_id,"source_name":m.source_name,"output_name":m.output_name,"field_type":m.field_type})).collect::<Vec<_>>();
        let artifact=crate::connectors::transfer_worker::run_export(json!({"action":"export","format":format,"crs_code":output_crs,"geometry_family":source.geometry_family,"layer_name":source.layer_name,"field_mappings":mappings,"features":features,"bounded_transfer":true})).await
            .map_err(|e|TransferError::coded(e.status.as_u16(),"VECTOR_CODEC_FAILED",if e.status.is_server_error(){"Vector export worker failed.".into()}else{e.message}))?;
        if artifact.ext != ext
            || artifact.media_type != media
            || artifact.bytes.is_empty()
            || artifact.bytes.len() > 64 * 1024 * 1024
            || detect_format(&artifact.bytes)? != format
        {
            return Err(TransferError::internal("Unexpected binary artifact"));
        }
        let (sha, size) = store.put(&storage_key, &artifact.bytes).await?;
        if sha != artifact.sha256 {
            store.delete(&storage_key).await;
            return Err(TransferError::internal("Artifact digest mismatch"));
        }
        (sha, size, counts)
    };
    let published = async {
        let done_at = now_micros();
        let count = source.feature_count;
        let comparison_report = json!({
            "source_id": source.source_id.to_string(),
            "layer_id": request.layer_id.to_string(),
            "source_content_revision": content,
            "source_schema_revision": schema,
            "output_format": format,
            "source_crs": stored,
            "output_crs": output_crs,
            "transformation_operation_id": preview.operation_id,
            "expected_feature_count": count,
            "exported_feature_count": count,
            "geometry_family": source.geometry_family,
            "field_mappings": preview.mappings.iter().map(Mapping::wire).collect::<Vec<_>>(),
            "approved_disclosures": approved,
            "lossless_profile": false,
            "null_counts_by_field": null_counts,
            "artifact_sha256": sha256,
            "artifact_byte_size": byte_size,
            "independent_reader_verified": false,
            "note": "Exact revision exported; run independent GIS reader check for external verification.",
        });
        let artifact = NewArtifact {
            id: artifact_id,
            job_id,
            source_id: source.source_id,
            layer_id: request.layer_id,
            content_revision: content.clone(),
            schema_revision: schema.clone(),
            output_format: format,
            output_crs,
            filename: format!(
                "{}-{}.{ext}",
                sanitize_filename(&source.layer_name, "export"),
                &content[..8]
            ),
            content_type: media,
            byte_size,
            sha256: sha256.clone(),
            storage_key: storage_key.clone(),
            comparison_report: comparison_report.clone(),
            created_at: done_at,
            expires_at: done_at + Duration::hours(24),
        };
        let job = scope.publish_export_artifact(&artifact, comparison_report).await?;
        scope.commit().await?;
        Ok::<Value, TransferError>(job.wire)
    }
    .await;
    if published.is_err() {
        store.delete(&storage_key).await;
    }
    published
}

/// Bound decoded binary export request before invoking the file codec.
async fn collect_binary_features(
    scope: &mut TransferScope<'_>,
    source: &ovg_storage::transfers::ExportSource,
    content: &str,
    schema: &str,
    transform: Option<&Transform>,
) -> Result<(Vec<Value>, Map<String, Value>), TransferError> {
    let stored = source.stored_crs.unwrap_or_default();
    let family = source.geometry_family.as_str();
    let steps = transform.map_or(&[][..], |t| t.steps.as_slice());
    let mut features = Vec::new();
    let mut bytes = 0usize;
    let mut nulls = vec![0_i64; source.fields.len()];
    let mut after = None;
    if source.feature_count > MAX_TRANSFER_FEATURES as i64 {
        return Err(TransferError::coded(
            413,
            "FEATURE_LIMIT_EXCEEDED",
            "Binary export exceeds feature limit.",
        ));
    }
    loop {
        let rows = scope
            .fetch_export_rows(
                source.source_id,
                content,
                schema,
                steps,
                after.as_deref(),
                PAGE_ROWS,
            )
            .await?;
        for row in &rows {
            let native = row
                .geometry
                .wkb
                .first()
                .and_then(|b| b.as_ref())
                .and_then(|b| codec::decode_wkb(b))
                .filter(|g| {
                    row.present
                        && row.geometry.valid
                        && row.native_srid == Some(stored)
                        && is_ready(g, Some(family), stored, false)
                })
                .ok_or_else(|| {
                    TransferError::validation(
                        "INVALID_GEOMETRY",
                        "Binary export encountered invalid geometry.",
                    )
                })?;
            let geom = match transform {
                None => native,
                Some(t) => t.apply(&native, family, &row.geometry.wkb, &row.geometry.step_valid)?,
            };
            let mut attrs = Map::new();
            for (count, f) in nulls.iter_mut().zip(&source.fields) {
                let v = row
                    .attributes
                    .get(&f.field_id)
                    .cloned()
                    .unwrap_or(Value::Null);
                if v.is_null() {
                    *count += 1;
                }
                attrs.insert(f.field_id.clone(), v);
            }
            let value = json!({"feature_id":row.feature_id,"attributes":attrs,"geometry":{"type":family,"coordinates":geom.coordinates_value()}});
            bytes = bytes.saturating_add(
                serde_json::to_vec(&value)
                    .map_err(|_| TransferError::internal("Serialize export row"))?
                    .len()
                    + 1,
            );
            if bytes > 32 * 1024 * 1024 || features.len() >= MAX_TRANSFER_FEATURES {
                return Err(TransferError::coded(
                    413,
                    "EXPORT_BUDGET_EXCEEDED",
                    "Binary export decoded data exceeds bounded worker budget.",
                ));
            }
            features.push(value);
        }
        if rows.len() < PAGE_ROWS as usize {
            break;
        }
        after = rows.last().map(|r| r.feature_id.clone());
    }
    if features.len() as i64 != source.feature_count {
        return Err(TransferError::internal(
            "Pinned feature count changed during export",
        ));
    }
    Ok((
        features,
        source
            .fields
            .iter()
            .zip(nulls)
            .map(|(f, n)| (f.field_id.clone(), Value::from(n)))
            .collect(),
    ))
}

/// Write the RFC 7946 document page by page; returns null counts per source field.
async fn stream_geojson(
    scope: &mut TransferScope<'_>,
    source: &ovg_storage::transfers::ExportSource,
    content: &str,
    schema: &str,
    transform: Option<&Transform>,
    mappings: &[Mapping],
    staged: &mut Staged,
) -> Result<Map<String, Value>, TransferError> {
    let stored = source.stored_crs.unwrap_or_default();
    let family = source.geometry_family.as_str();
    let steps = transform.map_or(&[][..], |t| t.steps.as_slice());
    let mut nulls = vec![0_i64; source.fields.len()];
    let mut buffer = String::from(GEOJSON_HEAD);
    let mut after: Option<String> = None;
    let mut index: i64 = 0;
    loop {
        let rows = scope
            .fetch_export_rows(
                source.source_id,
                content,
                schema,
                steps,
                after.as_deref(),
                PAGE_ROWS,
            )
            .await?;
        for row in &rows {
            let native = match (&row.geometry.wkb[0], row.native_srid) {
                (Some(bytes), Some(_)) if row.present && row.geometry.valid => {
                    codec::decode_wkb(bytes)
                }
                _ => None,
            }
            .ok_or_else(|| TransferError::internal("ready feature has invalid native geometry"))?;
            if row.native_srid != Some(stored) || !is_ready(&native, Some(family), stored, false) {
                return Err(TransferError::validation(
                    "INVALID_GEOMETRY",
                    format!("Feature at index {index} failed geometry validation."),
                ));
            }
            let geom = match transform {
                None => native,
                Some(t) => t.apply(&native, family, &row.geometry.wkb, &row.geometry.step_valid)?,
            };
            if index > 0 {
                buffer.push_str(", ");
            }
            push_geojson_feature(
                &mut buffer,
                &row.feature_id,
                &geom,
                mappings,
                &row.attributes,
            )?;
            for (count, field) in nulls.iter_mut().zip(&source.fields) {
                if row
                    .attributes
                    .get(&field.field_id)
                    .is_none_or(Value::is_null)
                {
                    *count += 1;
                }
            }
            index += 1;
            if buffer.len() >= FLUSH_BYTES {
                staged.write(buffer.as_bytes()).await?;
                buffer.clear();
            }
        }
        if rows.len() < usize::try_from(PAGE_ROWS).unwrap_or(usize::MAX) {
            break;
        }
        after = rows.last().map(|r| r.feature_id.clone());
    }
    buffer.push_str(GEOJSON_TAIL);
    staged.write(buffer.as_bytes()).await?;
    if index != source.feature_count {
        return Err(TransferError::internal(
            "pinned feature count changed during export",
        ));
    }
    Ok(source
        .fields
        .iter()
        .zip(nulls)
        .map(|(f, n)| (f.field_id.clone(), Value::from(n)))
        .collect())
}

// ----- jobs and artifacts -----

async fn list_transfer_jobs(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
    Query(query): Query<HashMap<String, String>>,
) -> Reply {
    let mut scope = prelude(&state, &auth, &headers, &params, false).await?;
    let limit = match query.get("limit") {
        None => 50,
        Some(raw) => raw
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|l| (1..=100).contains(l))
            .ok_or_else(invalid)?,
    };
    scope.authorize(false).await?;
    let items = scope.list_jobs(limit).await?;
    Ok(Json(json!({ "items": items })).into_response())
}

async fn job_prelude<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    headers: &HeaderMap,
    params: &HashMap<String, String>,
    post: bool,
) -> Result<(TransferScope<'a>, ovg_storage::transfers::Job), Fail> {
    let mut scope = prelude(state, auth, headers, params, post).await?;
    let job_id = param(params, "job_id").ok_or_else(invalid)?;
    scope.authorize(false).await?;
    let job = scope
        .get_job(job_id, post)
        .await?
        .ok_or_else(|| TransferError::not_found("Transfer job not found."))?;
    if post && job.kind == "import" {
        scope.authorize(true).await?;
    }
    Ok((scope, job))
}

async fn get_transfer_job(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
) -> Reply {
    let (_scope, job) = job_prelude(&state, &auth, &headers, &params, false).await?;
    Ok(Json(job.wire).into_response())
}

async fn cancel_transfer_job(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
) -> Reply {
    let (mut scope, job) = job_prelude(&state, &auth, &headers, &params, true).await?;
    if matches!(
        job.status.as_str(),
        "succeeded" | "failed" | "denied" | "cancelled"
    ) {
        return Ok(Json(job.wire).into_response());
    }
    let updated = scope
        .update_job(
            job.id,
            JobUpdate {
                status: "cancelled",
                error_code: Some("CANCELLED_BY_ACTOR"),
                error_message: Some("Cancelled before publication.".into()),
                completed_at: Some(now_micros()),
                ..JobUpdate::default()
            },
        )
        .await?;
    scope.commit().await?;
    Ok(Json(updated.wire).into_response())
}

async fn retry_transfer_job(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
) -> Reply {
    let (mut scope, job) = job_prelude(&state, &auth, &headers, &params, true).await?;
    if job.status != "recoverable_failed" {
        return Err(TransferError::conflict(
            "JOB_NOT_RETRYABLE",
            "Only recoverable failed jobs may be retried.",
        )
        .into());
    }
    let ids = (scope.organization_id(), scope.project_id());
    let next = job.attempt_count + 1;
    scope
        .update_job(
            job.id,
            JobUpdate {
                status: "active",
                attempt_count: Some(next),
                ..JobUpdate::default()
            },
        )
        .await?;
    scope.record_attempt(job.id, next, now_micros()).await?;
    scope.commit().await?;
    if job.kind != "import" {
        return Err(TransferError::conflict(
            "EXPORT_RETRY_UNSUPPORTED",
            "Re-issue an authorized export preview and confirm for this revision.",
        )
        .into());
    }
    let mut scope = reopen(&state, &auth, ids, false).await?;
    let asset_id = job
        .source_asset_id
        .ok_or_else(|| TransferError::internal("import job without asset"))?;
    let (asset, payload) = load_asset(
        &state,
        &mut scope,
        asset_id,
        "Source asset unavailable for retry.",
    )
    .await?;
    drop(scope);
    let opts = &job.options;
    let text = |key: &str| opts.get(key).and_then(Value::as_str);
    let inspection = inspect_payload(
        &payload,
        &asset.detected_format,
        &inspect_options(
            &asset,
            text("selected_member"),
            opts.get("declared_source_crs").and_then(Value::as_i64),
            opts.get("target_crs").and_then(Value::as_i64),
            text("declared_encoding"),
        ),
    )
    .await?;
    let source_name = text("source_name").unwrap_or_default().to_string();
    let layer_name = text("layer_name").unwrap_or_default().to_string();
    let job = execute_import(
        &state,
        &auth,
        ids,
        job.id,
        &asset,
        inspection,
        &source_name,
        &layer_name,
    )
    .await?;
    Ok(Json(job).into_response())
}

async fn artifact_prelude<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    headers: &HeaderMap,
    params: &HashMap<String, String>,
) -> Result<(TransferScope<'a>, Uuid, ovg_storage::transfers::Artifact), Fail> {
    let mut scope = prelude(state, auth, headers, params, false).await?;
    let artifact_id = param(params, "artifact_id").ok_or_else(invalid)?;
    scope.authorize(false).await?;
    let artifact = scope
        .get_artifact(artifact_id)
        .await?
        .ok_or_else(|| TransferError::not_found("Derived artifact not found."))?;
    Ok((scope, artifact_id, artifact))
}

async fn get_derived_artifact(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
) -> Reply {
    let (_scope, _, artifact) = artifact_prelude(&state, &auth, &headers, &params).await?;
    Ok(Json(artifact.wire).into_response())
}

async fn download_derived_artifact(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Params,
) -> Reply {
    let (mut scope, artifact_id, artifact) =
        artifact_prelude(&state, &auth, &headers, &params).await?;
    if artifact.state == "expired" || chrono::Utc::now() >= artifact.expires_at {
        if artifact.state != "expired" {
            scope.mark_artifact_state(artifact_id, "expired").await?;
            scope.commit().await?;
        }
        return Err(TransferError::coded(
            410,
            "ARTIFACT_EXPIRED",
            "Export artifact has expired; request a new authorized export.",
        )
        .into());
    }
    if artifact.state != "ready" {
        return Err(TransferError::coded(
            410,
            "ARTIFACT_UNAVAILABLE",
            "Export artifact is unavailable; request a new authorized export.",
        )
        .into());
    }
    let store = Store::new(&state);
    let (file, length) = match store
        .open_verified(&artifact.storage_key, &artifact.sha256)
        .await
    {
        Ok(opened) => opened,
        Err(error @ TransferError::Coded { status: 410, .. }) => {
            scope
                .mark_artifact_state(artifact_id, "unavailable")
                .await?;
            scope.commit().await?;
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };
    // Release the project locks before the (possibly slow) client download.
    scope.commit().await?;
    let header_value = |value: &str| {
        HeaderValue::from_str(value).map_err(|_| TransferError::internal("unsafe artifact header"))
    };
    let mut response = Response::new(Body::from_stream(store::file_stream(file)));
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, header_value(&artifact.content_type)?);
    response_headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        header_value(&format!("attachment; filename=\"{}\"", artifact.filename))?,
    );
    response_headers.insert("x-content-sha256", header_value(&artifact.sha256)?);
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}
