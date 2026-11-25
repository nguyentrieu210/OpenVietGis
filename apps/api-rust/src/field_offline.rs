//! E13 Rust-native online control plane for offline field packages.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::field_packages;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/field/packages";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(BASE, get(list_packages)).route(
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
        }
    }
}
