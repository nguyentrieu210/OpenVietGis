//! E13 Rust-native online control plane for offline field packages and outbox sync.

use axum::body::{to_bytes, Body};
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{SubsecRound, Utc};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::editing::domain::{validate_operation_id, EditOperation, OpType};
use ovg_storage::editing::{CommitRequest, EditError, Guard, Permission};
use ovg_storage::field_packages::{self, InsertPackageArgs, InsertSyncBatchArgs};
use ovg_storage::transfers::TransferError;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::edit_auth::{self, authenticate_json, require_csrf as actor_csrf, Actor, Failure};
use crate::field_worker::{self, WorkerError};
use crate::identity::{require_csrf, HumanSession};
use crate::transfers::store::{file_stream, Store};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/field/packages";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, get(list_packages).post(create_package))
        .route(
            &format!("{BASE}/{{package_id}}/download"),
            get(download_package),
        )
        .route(
            &format!("{BASE}/{{package_id}}/snapshot"),
            get(download_snapshot),
        )
        .route(
            &format!("{BASE}/{{package_id}}/revoke"),
            post(revoke_package),
        )
        .route(&format!("{BASE}/{{package_id}}/sync"), post(sync_outbox))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateOfflinePackageRequest {
    name: String,
    layer_id: Uuid,
    aoi_bbox: BTreeMap<String, f64>,
    #[serde(default = "default_ttl_hours")]
    ttl_hours: i64,
    #[serde(default)]
    local_storage_confirmed: bool,
}

const fn default_ttl_hours() -> i64 {
    72
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SyncOutboxRequest {
    operation_id: String,
    operations: Vec<Value>,
}

async fn create_package(
    State(state): State<AppState>,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), FieldError> {
    let (mut parts, body) = request.into_parts();
    let raw = to_bytes(body, 256 * 1024)
        .await
        .map_err(|_| FieldError::Access(CatalogError::InvalidRequest))?;
    let req: CreateOfflinePackageRequest =
        serde_json::from_slice(&raw).map_err(|_| FieldError::Access(CatalogError::InvalidRequest))?;
    let auth = HumanSession::from_request_parts(&mut parts, &state)
        .await
        .map_err(|err| FieldError::Access(CatalogError::Identity(err)))?;
    require_csrf(&state, &auth, &parts.headers)
        .map_err(|error| FieldError::Access(CatalogError::Identity(error)))?;

    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(FieldError::Access)?;
    if !req.local_storage_confirmed {
        return Err(FieldError::Domain(AppError::forbidden(
            "human operator must confirm data is eligible for local storage",
        )));
    }

    let prepared =
        field_packages::prepare_package_data(&mut edit, req.layer_id, &req.aoi_bbox, req.ttl_hours)
            .await
            .map_err(FieldError::Domain)?;
    let built = field_worker::build_package(
        prepared.srid,
        &prepared.geometry_family,
        &prepared.mappings,
        &prepared.snapshot,
    )
    .await
    .map_err(FieldError::Worker)?;

    let storage_key = format!(
        "field/{}/{}/{}.gpkg",
        organization_id, project_id, prepared.package_id
    );
    let store = Store::new(&state);
    let (sha256, byte_size) = store
        .put(&storage_key, &built.gpkg_bytes)
        .await
        .map_err(FieldError::Artifact)?;

    let created = match field_packages::insert_package(
        &mut edit,
        InsertPackageArgs {
            prepared: &prepared,
            name: &req.name,
            aoi_bbox: &req.aoi_bbox,
            snapshot_sha256: &built.snapshot_sha256,
            storage_key: &storage_key,
            sha256: &sha256,
            byte_size,
        },
    )
    .await
    {
        Ok(row) => row,
        Err(err) => {
            store.delete(&storage_key).await;
            return Err(FieldError::Domain(err));
        }
    };
    edit.commit().await.map_err(FieldError::Domain)?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn list_packages(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, FieldError> {
    // Python list_packages intentionally requires project.edit because the only
    // issued package profile is writable and this endpoint revalidates it.
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(FieldError::Access)?;
    let result = field_packages::list_packages(&mut edit, auth.actor_id())
        .await
        .map_err(FieldError::Domain)?;
    edit.commit().await.map_err(FieldError::Domain)?;
    Ok(Json(result))
}

async fn download_package(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, package_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, FieldError> {
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(FieldError::Access)?;
    let info = field_packages::download_info(&mut edit, auth.actor_id(), package_id)
        .await
        .map_err(FieldError::Domain)?;
    edit.commit().await.map_err(FieldError::Domain)?;

    let store = Store::new(&state);
    let (file, length) = store
        .open_verified(&info.storage_key, &info.sha256)
        .await
        .map_err(FieldError::Artifact)?;
    let mut response = Response::new(Body::from_stream(file_stream(file)));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/geopackage+sqlite3"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).map_err(|_| {
            FieldError::Domain(AppError::internal("invalid offline artifact length"))
        })?,
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"offline-{}.gpkg\"",
            info.id
        ))
        .map_err(|_| FieldError::Domain(AppError::internal("invalid offline filename")))?,
    );
    headers.insert(
        "x-content-sha256",
        HeaderValue::from_str(&info.sha256)
            .map_err(|_| FieldError::Domain(AppError::internal("invalid offline sha256")))?,
    );
    Ok(response)
}

const MAX_PACKAGE_BYTES: u64 = 75 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 25 * 1024 * 1024;
static SNAPSHOT_SLOTS: Semaphore = Semaphore::const_new(2);

fn invalid_snapshot() -> FieldError {
    FieldError::Domain(AppError::validation(
        "PACKAGE_INVALID",
        "offline snapshot missing or invalid; create a fresh package",
    ))
}

// Private scratch directory, never derived from caller-controlled paths.
struct SnapshotScratch(PathBuf);

impl SnapshotScratch {
    fn create() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("ovg-snapshot-{}", Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self(path))
    }
}

impl Drop for SnapshotScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// SQLite is a separately maintained file-format engine. Invoke a fixed query
/// against a private, checksummed copy; no shell, user SQL, or outbound access.
async fn extract_snapshot(path: &FsPath, expected_sha256: &str) -> Result<Vec<u8>, FieldError> {
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid_snapshot());
    }
    let query = format!(
        "PRAGMA trusted_schema=OFF; SELECT hex(payload) FROM ovg_offline_snapshot \
         WHERE typeof(payload)='blob' AND length(payload)<={} LIMIT 1;",
        MAX_SNAPSHOT_BYTES,
    );
    let mut command = tokio::process::Command::new("/usr/bin/sqlite3");
    command
        .arg("-safe")
        .arg("-readonly")
        .arg("-batch")
        .arg("-noheader")
        .arg("-init")
        .arg("/dev/null")
        .arg(path)
        .arg(query)
        .env_clear()
        .env("LANG", "C")
        .env("HOME", "/nonexistent")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| {
        FieldError::Domain(AppError::service_unavailable(
            "SNAPSHOT_ENGINE_UNAVAILABLE",
            "offline snapshot engine unavailable",
        ))
    })?;
    let stdout = child.stdout.take().ok_or_else(invalid_snapshot)?;
    let output = tokio::time::timeout(Duration::from_secs(15), async {
        let mut data = Vec::new();
        stdout
            .take((MAX_SNAPSHOT_BYTES as u64) * 2 + 2)
            .read_to_end(&mut data)
            .await?;
        if data.len() > MAX_SNAPSHOT_BYTES * 2 + 1 {
            let _ = child.kill().await;
            return Err(std::io::Error::other("snapshot too large"));
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(std::io::Error::other("snapshot query failed"));
        }
        Ok::<Vec<u8>, std::io::Error>(data)
    })
    .await
    .map_err(|_| invalid_snapshot())?
    .map_err(|_| invalid_snapshot())?;
    let encoded = output.strip_suffix(b"\n").ok_or_else(invalid_snapshot)?;
    let bytes = hex::decode(encoded).map_err(|_| invalid_snapshot())?;
    if bytes.is_empty()
        || bytes.len() > MAX_SNAPSHOT_BYTES
        || hex::encode(Sha256::digest(&bytes)) != expected_sha256
    {
        return Err(invalid_snapshot());
    }
    Ok(bytes)
}

async fn read_snapshot_json(
    state: &AppState,
    storage_key: &str,
    sha256: &str,
    snapshot_digest: &str,
) -> Result<Value, FieldError> {
    let _permit = SNAPSHOT_SLOTS
        .acquire()
        .await
        .map_err(|_| invalid_snapshot())?;
    let store = Store::new(state);
    let (file, length) = store
        .open_verified(storage_key, sha256)
        .await
        .map_err(FieldError::Artifact)?;
    if length > MAX_PACKAGE_BYTES {
        return Err(invalid_snapshot());
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(MAX_PACKAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| invalid_snapshot())?;
    if bytes.len() as u64 != length || hex::encode(Sha256::digest(&bytes)) != sha256 {
        return Err(invalid_snapshot());
    }
    let scratch = SnapshotScratch::create().map_err(|_| invalid_snapshot())?;
    let path = scratch.0.join("package.gpkg");
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|_| invalid_snapshot())?;
    let payload = extract_snapshot(&path, snapshot_digest).await?;
    serde_json::from_slice(&payload).map_err(|_| invalid_snapshot())
}

async fn download_snapshot(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, package_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, FieldError> {
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(FieldError::Access)?;
    let info = field_packages::download_info(&mut edit, auth.actor_id(), package_id)
        .await
        .map_err(FieldError::Domain)?;
    let digest = info.snapshot_sha256.as_deref().ok_or_else(|| {
        FieldError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "legacy package cannot be opened offline; create a fresh package",
        ))
    })?;
    let _permit = SNAPSHOT_SLOTS
        .try_acquire()
        .map_err(|_| FieldError::Domain(AppError::rate_limited(Some(1))))?;
    let store = Store::new(&state);
    let (file, length) = store
        .open_verified(&info.storage_key, &info.sha256)
        .await
        .map_err(FieldError::Artifact)?;
    if length > MAX_PACKAGE_BYTES {
        return Err(invalid_snapshot());
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(MAX_PACKAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| invalid_snapshot())?;
    if bytes.len() as u64 != length || hex::encode(Sha256::digest(&bytes)) != info.sha256 {
        return Err(invalid_snapshot());
    }
    let scratch = SnapshotScratch::create().map_err(|_| invalid_snapshot())?;
    let path = scratch.0.join("package.gpkg");
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|_| invalid_snapshot())?;
    let payload = extract_snapshot(&path, digest).await?;
    edit.commit().await.map_err(FieldError::Domain)?;
    let mut response = Response::new(Body::from(payload));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-sha256",
        HeaderValue::from_str(digest).map_err(|_| invalid_snapshot())?,
    );
    Ok(response)
}

async fn revoke_package(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, package_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, FieldError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| FieldError::Access(CatalogError::Identity(error)))?;
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(FieldError::Access)?;
    let result = field_packages::revoke_package(&mut edit, package_id)
        .await
        .map_err(FieldError::Domain)?;
    edit.commit().await.map_err(FieldError::Domain)?;
    Ok(Json(result))
}

async fn sync_outbox(
    State(state): State<AppState>,
    Path((org, proj, package_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), FieldError> {
    let (actor, headers, raw) = authenticate_json(&state, request, &org)
        .await
        .map_err(FieldError::EditFailure)?;
    let req: SyncOutboxRequest = edit_auth::decode(raw).map_err(FieldError::EditFailure)?;
    if !matches!(actor, Actor::Human(_)) {
        return Err(FieldError::EditFailure(Failure::Edit(EditError::detail(
            403,
            "human actor required",
        ))));
    }
    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied)
        .await
        .map_err(FieldError::EditFailure)?
    else {
        return Err(FieldError::EditFailure(Failure::Edit(
            EditError::InvalidRequest,
        )));
    };
    scope
        .authorize(Permission::Edit)
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
    actor_csrf(&state, &actor, &headers).map_err(FieldError::EditFailure)?;

    let clean_op = validate_operation_id(&req.operation_id)
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
    if req.operations.iter().any(|item| !item.is_object()) {
        return Err(FieldError::EditFailure(Failure::Edit(
            EditError::InvalidRequest,
        )));
    }
    let submitted_count = i32::try_from(req.operations.len()).map_err(|_| {
        FieldError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "offline group requires 1..2000 operations",
        ))
    })?;

    let org_id = scope.organization_id();
    let proj_id = scope.project_id();
    let actor_id = scope.actor_id();
    let pkg = field_packages::load_sync_package(&mut scope, package_id)
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
    let digest = pkg.snapshot_sha256.as_deref().ok_or_else(|| {
        FieldError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "legacy offline package; issue a fresh package",
        ))
    })?;
    let snapshot = read_snapshot_json(&state, &pkg.storage_key, &pkg.sha256, digest).await?;

    let org_s = org_id.to_string();
    let proj_s = proj_id.to_string();
    let actor_s = actor_id.to_string();
    let pkg_s = pkg.id.to_string();
    let hashes = field_worker::validate_ops(
        &snapshot,
        &req.operations,
        &pkg.base_rev,
        [&org_s, &proj_s, &actor_s, &pkg_s, &clean_op],
    )
    .await
    .map_err(FieldError::Worker)?;

    let mut feature_ids = Vec::with_capacity(req.operations.len());
    let mut seen_core = HashSet::with_capacity(req.operations.len());
    for op in &req.operations {
        let local_id = op
            .get("feature_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut core_id = local_id.to_owned();
        if op.get("op_type").and_then(Value::as_str) == Some("create") {
            if let Some(raw_client_op) = op.get("client_operation_id").and_then(Value::as_str) {
                let client_op = Uuid::parse_str(raw_client_op).map_err(|_| {
                    FieldError::Domain(AppError::validation(
                        "INVALID_OFFLINE_OPERATION_ID",
                        "client_operation_id must be a unique canonical non-nil UUID",
                    ))
                })?;
                core_id = field_packages::offline_core_feature_id(
                    actor_id,
                    org_id,
                    proj_id,
                    pkg.id,
                    pkg.source_id,
                    client_op,
                );
                if core_id == local_id {
                    return Err(FieldError::Domain(AppError::validation(
                        "VALIDATION_ERROR",
                        "local feature identity must differ from Core identity",
                    )));
                }
            }
        }
        if !seen_core.insert(core_id.clone()) {
            return Err(FieldError::Domain(AppError::validation(
                "VALIDATION_ERROR",
                "offline operations target the same Core feature",
            )));
        }
        feature_ids.push(core_id);
    }

    field_packages::ensure_packaged_layer_active(&mut scope, pkg.layer_id, pkg.source_id)
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;

    let bound_op = format!("{}:{}", hashes.scope_hash, hashes.payload_hash);
    if let Some(existing) =
        field_packages::find_sync_batch_by_prefix(&mut scope, &hashes.scope_hash)
            .await
            .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?
    {
        if existing.operation_id != bound_op {
            return Err(FieldError::EditFailure(Failure::Edit(EditError::conflict(
                "OPERATION_ID_CONFLICT",
                "operation identity reused with different content",
            ))));
        }
        let body = field_packages::sync_wire(&existing, &clean_op, &req.operations, &feature_ids);
        scope
            .commit()
            .await
            .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
        return Ok((StatusCode::CREATED, Json(body)));
    }

    let core_op = format!("field-sync-{}", hashes.scope_hash);
    let core_summary = format!("Offline package {}; intent {}", pkg.id, hashes.payload_hash);
    field_packages::check_existing_core_changeset(
        &mut scope,
        pkg.source_id,
        &core_op,
        &core_summary,
    )
    .await
    .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;

    let mut edit_ops = Vec::with_capacity(req.operations.len());
    for (op, feature_id) in req.operations.iter().zip(feature_ids.iter()) {
        let op_type = match op.get("op_type").and_then(Value::as_str) {
            Some("create") => OpType::Create,
            Some("update") => OpType::Update,
            Some("delete") => OpType::Delete,
            _ => {
                return Err(FieldError::Domain(AppError::validation(
                    "OUTSIDE_PACKAGE_SCOPE",
                    "unsupported offline operation",
                )))
            }
        };
        let geometry = op.get("geometry").and_then(Value::as_object).cloned();
        let properties = op.get("properties").and_then(Value::as_object).cloned();
        edit_ops.push(EditOperation {
            op_type,
            feature_id: feature_id.clone(),
            geometry,
            properties,
        });
    }

    let mut applied_ids = Vec::new();
    let mut conflicts = Vec::new();
    let status = match scope
        .commit_changeset(
            pkg.source_id,
            CommitRequest {
                operation_id: core_op,
                base_content_revision: pkg.base_rev,
                base_schema_revision: None,
                summary: core_summary,
                operations: edit_ops,
            },
        )
        .await
    {
        Ok(cs) => {
            if let Some(cid) = cs.get("id").and_then(Value::as_str) {
                applied_ids.push(cid.to_owned());
            }
            scope
                .commit()
                .await
                .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
            "applied"
        }
        Err(EditError::Platform {
            status: 409,
            code,
            message,
            details,
        }) => {
            scope
                .rollback()
                .await
                .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
            conflicts.push(json!({
                "code": code,
                "message": message,
                "details": details.unwrap_or_else(|| json!({})),
                "resolution": "explicit_conflict_review_required",
            }));
            "conflict"
        }
        Err(other) => return Err(FieldError::EditFailure(Failure::Edit(other))),
    };

    let Some(mut scope2) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied)
        .await
        .map_err(FieldError::EditFailure)?
    else {
        return Err(FieldError::EditFailure(Failure::Edit(
            EditError::InvalidRequest,
        )));
    };
    scope2
        .authorize(Permission::Edit)
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
    let now = Utc::now().trunc_subsecs(6);
    field_packages::verify_package_lease(&mut scope2, package_id, now)
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;

    if let Some(raced) = field_packages::find_sync_batch_by_prefix(&mut scope2, &hashes.scope_hash)
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?
    {
        if raced.operation_id != bound_op {
            return Err(FieldError::EditFailure(Failure::Edit(EditError::conflict(
                "OPERATION_ID_CONFLICT",
                "operation identity reused with different content",
            ))));
        }
        let body = field_packages::sync_wire(&raced, &clean_op, &req.operations, &feature_ids);
        scope2
            .commit()
            .await
            .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
        return Ok((StatusCode::CREATED, Json(body)));
    }

    let inserted = field_packages::insert_sync_batch(
        &mut scope2,
        InsertSyncBatchArgs {
            package_id: pkg.id,
            bound_op: &bound_op,
            status,
            submitted_ops_count: submitted_count,
            applied_ids: &applied_ids,
            conflicts: &conflicts,
            now,
        },
    )
    .await
    .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
    let body = field_packages::sync_wire(&inserted, &clean_op, &req.operations, &feature_ids);
    scope2
        .commit()
        .await
        .map_err(|e| FieldError::EditFailure(Failure::Edit(e)))?;
    Ok((StatusCode::CREATED, Json(body)))
}

enum FieldError {
    Access(CatalogError),
    Domain(AppError),
    Artifact(TransferError),
    Worker(WorkerError),
    EditFailure(Failure),
}

impl IntoResponse for FieldError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::EditFailure(failure) => failure.into_response(),
            Self::Worker(err) => {
                let mut body = json!({ "code": err.code, "message": err.message });
                let details = err
                    .details
                    .filter(|d| d.as_object().is_none_or(|m| !m.is_empty()));
                if let Some(details) = details {
                    body["details"] = details;
                }
                (err.status, Json(body)).into_response()
            }
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "offline field request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
            Self::Artifact(TransferError::Coded {
                status,
                code,
                message,
            }) => (
                StatusCode::from_u16(status).unwrap_or(StatusCode::GONE),
                Json(serde_json::json!({"code": code, "message": message})),
            )
                .into_response(),
            Self::Artifact(TransferError::Detail { status, detail }) => (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(serde_json::json!({"detail": detail})),
            )
                .into_response(),
            Self::Artifact(TransferError::InvalidRequest) => {
                CatalogError::InvalidRequest.into_response()
            }
            Self::Artifact(TransferError::ReadOnly) => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "code": "PROJECT_NOT_WRITABLE",
                    "detail": "Project is read-only."
                })),
            )
                .into_response(),
            Self::Artifact(TransferError::Internal(detail)) => {
                tracing::error!(%detail, "offline artifact read failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_snapshot_requires_exact_digest() {
        let scratch = SnapshotScratch::create().expect("private scratch");
        let path = scratch.0.join("fixture.gpkg");
        let payload = br#"{"features":[]}"#;
        let query = format!(
            "CREATE TABLE ovg_offline_snapshot(payload BLOB NOT NULL); \
             INSERT INTO ovg_offline_snapshot VALUES(X'{}');",
            hex::encode(payload),
        );
        let status = tokio::process::Command::new("/usr/bin/sqlite3")
            .arg(&path)
            .arg(query)
            .status()
            .await
            .expect("SQLite fixture engine is required");
        assert!(status.success());
        let digest = hex::encode(Sha256::digest(payload));
        let result = extract_snapshot(&path, &digest).await;
        assert_eq!(result.ok().as_deref(), Some(payload.as_slice()));
        assert!(extract_snapshot(&path, &"0".repeat(64)).await.is_err());
    }
}
