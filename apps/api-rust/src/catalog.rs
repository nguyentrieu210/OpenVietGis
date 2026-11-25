//! Scoped spatial catalog routes matching the existing FastAPI wire.

use axum::body::Bytes;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::crs::reference_registry_wire;
use ovg_core::AppError;
use ovg_storage::catalog::{FeatureRequest, MapSaveIntent, ProjectRead, ProjectReadPolicy};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::{require_csrf, HumanSession, IdentityError};
use crate::AppState;

pub fn router() -> Router<AppState> {
    const PROJECT: &str = "/organizations/{organization_id}/projects/{project_id}";
    Router::new()
        .route(&format!("{PROJECT}/map"), get(get_map).put(save_map))
        .route(&format!("{PROJECT}/map/history"), get(map_history))
        .route(
            &format!("{PROJECT}/map/operations/{{operation_id}}"),
            get(map_operation),
        )
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
        .route(
            &format!("{PROJECT}/layers/{{layer_id}}/features"),
            get(page_features),
        )
        .route(
            &format!("{PROJECT}/layers/{{layer_id}}/features/{{feature_id}}"),
            get(get_feature),
        )
}

/// Raw feature-read query; validated like the FastAPI parameter declarations.
#[derive(Deserialize)]
struct FeatureQuery {
    content_revision: Option<String>,
    schema_revision: Option<String>,
    include_geometry: Option<String>,
    response_crs: Option<String>,
    after_feature_id: Option<String>,
    limit: Option<String>,
}

impl FeatureQuery {
    fn request(&self, layer_id: Uuid) -> Result<FeatureRequest<'_>, CatalogError> {
        let include_geometry = match self
            .include_geometry
            .as_deref()
            .map(str::to_ascii_lowercase)
        {
            None => true,
            Some(value) => match value.as_str() {
                "1" | "true" | "t" | "yes" | "y" | "on" => true,
                "0" | "false" | "f" | "no" | "n" | "off" => false,
                _ => return Err(CatalogError::InvalidRequest),
            },
        };
        let response_crs = self
            .response_crs
            .as_deref()
            .map(|value| {
                value
                    .parse::<u32>()
                    .ok()
                    .filter(|code| (1..=2_147_483_647).contains(code))
                    .ok_or(CatalogError::InvalidRequest)
            })
            .transpose()?;
        Ok(FeatureRequest {
            layer_id,
            content_revision: pin(&self.content_revision)?,
            schema_revision: pin(&self.schema_revision)?,
            include_geometry,
            response_crs,
        })
    }

    fn page(&self) -> Result<(Option<&str>, usize), CatalogError> {
        let after = self
            .after_feature_id
            .as_deref()
            .map(|value| feature_id(value).ok_or(CatalogError::InvalidRequest))
            .transpose()?;
        let limit = match self.limit.as_deref() {
            None => 100,
            Some(value) => value
                .parse::<usize>()
                .ok()
                .filter(|limit| (1..=1000).contains(limit))
                .ok_or(CatalogError::InvalidRequest)?,
        };
        Ok((after, limit))
    }
}

/// Read pins are lowercase SHA-256 hex digests.
fn pin(value: &Option<String>) -> Result<&str, CatalogError> {
    value
        .as_deref()
        .filter(|pin| {
            pin.len() == 64 && pin.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
        .ok_or(CatalogError::InvalidRequest)
}

/// Stable feature IDs: 1..=128 characters of `[A-Za-z0-9_-]`.
fn feature_id(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then_some(value)
}

async fn page_features(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
    query: Result<Query<FeatureQuery>, QueryRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id, layer_id)) =
        path.map_err(|_| CatalogError::InvalidRequest)?;
    // Membership is resolved before parameter validation, like the FastAPI guard.
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let Query(query) = query.map_err(|_| CatalogError::InvalidRequest)?;
    let request = query.request(layer_id)?;
    let (after, limit) = query.page()?;
    let result = read
        .feature_page(&request, after, limit)
        .await
        .map_err(CatalogError::spatial)?;
    read.commit().await?;
    Ok(Json(result))
}

async fn get_feature(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid, Uuid, String)>, PathRejection>,
    query: Result<Query<FeatureQuery>, QueryRejection>,
) -> Result<Json<Value>, CatalogError> {
    let Path((organization_id, project_id, layer_id, feature)) =
        path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let Query(query) = query.map_err(|_| CatalogError::InvalidRequest)?;
    let feature = feature_id(&feature).ok_or(CatalogError::InvalidRequest)?;
    let request = query.request(layer_id)?;
    let result = read
        .feature_detail(&request, feature)
        .await
        .map_err(CatalogError::spatial)?;
    read.commit().await?;
    Ok(Json(result))
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

async fn get_map(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, CatalogError> {
    let Path((org, project)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(&state, &auth, org, project, ProjectReadPolicy::LayerAcl).await?;
    let value = read.get_map().await?;
    read.commit().await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response())
}

async fn save_map(
    auth: HumanSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    body: Bytes,
) -> Result<Response, CatalogError> {
    require_csrf(&state, &auth, &headers).map_err(CatalogError::Identity)?;
    let Path((org, project)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        org,
        project,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let intent = MapSaveIntent::parse(&body).map_err(CatalogError::spatial)?;
    let value = read
        .save_map(&intent, auth.display_name())
        .await
        .map_err(CatalogError::spatial)?;
    read.commit().await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response())
}

#[derive(Deserialize)]
struct MapHistoryQuery {
    after_revision: Option<i64>,
    limit: Option<usize>,
}

async fn map_history(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<MapHistoryQuery>, QueryRejection>,
) -> Result<Response, CatalogError> {
    let Path((org, project)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read =
        project_read(&state, &auth, org, project, ProjectReadPolicy::Unrestricted).await?;
    let Query(query) = query.map_err(|_| CatalogError::InvalidRequest)?;
    let value = read
        .map_history(query.after_revision, query.limit.unwrap_or(50))
        .await?;
    read.commit().await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response())
}

async fn map_operation(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, CatalogError> {
    let Path((org, project, operation)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        org,
        project,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let value = read.map_operation(operation).await?;
    read.commit().await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response())
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
    .map_err(CatalogError::spatial)
}

enum CatalogError {
    InvalidRequest,
    Identity(IdentityError),
    /// Fixed spatial descriptions rendered as `{"code", "detail"}`.
    Spatial(StatusCode, &'static str, String),
    Application(AppError),
}

/// Spatial error codes with a fixed public description in the reference API.
const SPATIAL_CODES: [&str; 13] = [
    "STALE_MAP_REVISION",
    "OPERATION_INTENT_CONFLICT",
    "NO_CHANGES",
    "MAP_REFERENCE_UNAVAILABLE",
    "INVALID_MAP_REQUEST",
    "PROJECT_NOT_WRITABLE",
    "SOURCE_NOT_READY",
    "STALE_READ_PIN",
    "INVALID_CURSOR",
    "UNSUPPORTED_CRS",
    "UNSUPPORTED_TRANSFORMATION",
    "TRANSFORMATION_OUT_OF_AREA",
    "TRANSFORMATION_UNAVAILABLE",
];

impl CatalogError {
    fn spatial(error: AppError) -> Self {
        match error {
            AppError::Conflict { code, message, .. } if SPATIAL_CODES.contains(&code) => {
                Self::Spatial(StatusCode::CONFLICT, code, message)
            }
            AppError::Validation { code, message, .. } if SPATIAL_CODES.contains(&code) => {
                Self::Spatial(StatusCode::UNPROCESSABLE_ENTITY, code, message)
            }
            error => error.into(),
        }
    }
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
            Self::Identity(error) => error.into_response(),
            Self::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            Self::Spatial(status, code, detail) => {
                (status, Json(json!({"code": code, "detail": detail}))).into_response()
            }
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
