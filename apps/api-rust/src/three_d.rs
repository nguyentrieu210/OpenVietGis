//! E10 3D model vault and saved-scene read control plane.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::three_d;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const MODELS: &str = "/organizations/{organization_id}/projects/{project_id}/3d/models";
const SCENES: &str = "/organizations/{organization_id}/projects/{project_id}/3d/scenes";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(MODELS, get(list_models))
        .route(
            &format!("{MODELS}/{{model_id}}/placements"),
            get(model_history),
        )
        .route(SCENES, get(list_scenes))
        .route(
            &format!("{SCENES}/{{scene_id}}/versions"),
            get(scene_history),
        )
}
fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h
}
async fn list_models(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p)): Path<(Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::list_models(&mut r)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    Ok((headers(), Json(v)))
}
async fn model_history(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::model_history(&mut r, id)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    v.map(|v| (headers(), Json(v)))
        .ok_or(ThreeDError::NotFound("Model not found"))
}
async fn list_scenes(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p)): Path<(Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::list_scenes(&mut r)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    Ok((headers(), Json(v)))
}
async fn scene_history(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::scene_history(&mut r, id)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    v.map(|v| (headers(), Json(v)))
        .ok_or(ThreeDError::NotFound("Scene not found"))
}
enum ThreeDError {
    Access(CatalogError),
    Domain(AppError),
    NotFound(&'static str),
}
impl IntoResponse for ThreeDError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(e) => e.into_response(),
            Self::NotFound(d) => {
                (StatusCode::NOT_FOUND, Json(serde_json::json!({"detail":d}))).into_response()
            }
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail=%internal_detail,"3D request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(e) => {
                let s = StatusCode::from_u16(e.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (s, Json(e.to_envelope(None))).into_response()
            }
        }
    }
}
