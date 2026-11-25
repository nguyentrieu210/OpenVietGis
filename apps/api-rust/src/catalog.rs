//! Scoped spatial catalog routes matching the existing FastAPI wire.

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::crs::reference_registry_wire;
use ovg_core::AppError;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::HumanSession;
use crate::AppState;

pub fn router() -> Router<AppState> {
    const PROJECT: &str = "/organizations/{organization_id}/projects/{project_id}";
    Router::new()
        .route(&format!("{PROJECT}/layers"), get(list_layers))
        .route(&format!("{PROJECT}/layers/{{layer_id}}"), get(get_layer))
        .route(&format!("{PROJECT}/spatial/sources"), get(list_sources))
        .route(
            &format!("{PROJECT}/spatial/sources/{{source_id}}"),
            get(get_source),
        )
        .route(
            &format!("{PROJECT}/spatial/crs-profiles"),
            get(list_crs_profiles),
        )
}

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    after_id: Option<Uuid>,
}

const fn default_limit() -> usize {
    100
}

async fn list_layers(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    page: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let Query(page) = page.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let result = read.list_layers(page.after_id, page.limit).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn get_layer(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id, layer_id)) =
        path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let result = read.get_layer(layer_id).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn list_sources(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    page: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let Query(page) = page.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = read.list_sources(page.after_id, page.limit).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn get_source(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id, source_id)) =
        path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = read.get_source(source_id).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn list_crs_profiles(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = reference_registry_wire(organization_id, project_id);
    read.commit().await?;
    Ok(Json(result))
}

async fn project_read<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
    policy: ProjectReadPolicy,
) -> Result<ProjectRead<'a>, CatalogError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        AppError::service_unavailable("DATABASE_UNAVAILABLE", "database unavailable")
    })?;
    ProjectRead::begin(
        pool,
        organization_id,
        project_id,
        auth.actor_id(),
        auth.session_id(),
        policy,
    )
    .await
    .map_err(Into::into)
}

enum CatalogError {
    InvalidRequest,
    Application(AppError),
}

impl From<AppError> for CatalogError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Validation { .. } => Self::InvalidRequest,
            error => Self::Application(error),
        }
    }
}

impl IntoResponse for CatalogError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            Self::Application(error) => {
                if let AppError::Internal { internal_detail } = &error {
                    tracing::error!(detail = %internal_detail, "catalog request failed");
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
                        .into_response();
                }
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(json!({"detail": error.to_string()}))).into_response()
            }
        }
    }
}
