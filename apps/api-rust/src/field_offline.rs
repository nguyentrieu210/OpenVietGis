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
