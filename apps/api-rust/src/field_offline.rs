//! E13 Rust-native online control plane for offline field packages.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::field_packages;
use ovg_storage::transfers::TransferError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::transfers::store::{file_stream, Store};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/field/packages";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, get(list_packages))
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
async fn extract_snapshot(
    path: &FsPath,
    expected_sha256: &str,
) -> Result<Vec<u8>, FieldError> {
    if expected_sha256.len() != 64
        || !expected_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
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
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-sha256",
        HeaderValue::from_str(digest).map_err(|_| invalid_snapshot())?,
    );
    Ok(response)
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

enum FieldError {
    Access(CatalogError),
    Domain(AppError),
    Artifact(TransferError),
}

impl IntoResponse for FieldError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
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
