//! E10 3D Tiles catalog + manifest control plane.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::tilesets;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/3d/tilesets";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(BASE, get(list_tilesets)).route(
        &format!("{BASE}/{{tileset_id}}/tileset.json"),
        get(get_manifest),
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
async fn list_tilesets(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p)): Path<(Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), TilesetError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(TilesetError::Access)?;
    let v = tilesets::list(&mut r).await.map_err(TilesetError::Domain)?;
    r.commit().await.map_err(TilesetError::Domain)?;
    Ok((headers(), Json(v)))
}
async fn get_manifest(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, TilesetError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(TilesetError::Access)?;
    let v = tilesets::manifest(&mut r, id)
        .await
        .map_err(TilesetError::Domain)?;
    r.commit().await.map_err(TilesetError::Domain)?;
    let Some(v) = v else {
        return Err(TilesetError::NotFound("Tileset not found"));
    };
    let body = serde_json::to_vec(&v).map_err(|_| {
        TilesetError::Domain(AppError::internal("tileset manifest serialization failed"))
    })?;
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    for (key, value) in headers() {
        if let Some(key) = key {
            response.headers_mut().insert(key, value);
        }
    }
    Ok(response)
}
enum TilesetError {
    Access(CatalogError),
    Domain(AppError),
    NotFound(&'static str),
}
impl IntoResponse for TilesetError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(e) => e.into_response(),
            Self::NotFound(d) => {
                (StatusCode::NOT_FOUND, Json(serde_json::json!({"detail":d}))).into_response()
            }
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail=%internal_detail,"tileset request failed");
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
