//! E12 authenticated external-source metadata and cache lifecycle.

use axum::extract::{rejection::JsonRejection, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::external_sources::{self, AllowlistInput, ConnectionInput};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::tenancy::{self, TenancyError};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/external-sources";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/organizations/{organization_id}/external-sources/allowlist",
            get(list_allowlist).post(register_allowlist),
        )
        .route(BASE, get(list_connections).post(create_connection))
        .route(
            &format!("{BASE}/{{connection_id}}/invalidate-cache"),
            post(invalidate_cache),
        )
        .route(&format!("{BASE}/{{connection_id}}/revoke"), post(revoke))
        .route(&format!("{BASE}/{{connection_id}}/preview"), post(preview))
        .route(
            &format!("{BASE}/{{connection_id}}/viewport"),
            post(viewport),
        )
}

enum ExternalError {
    Access(CatalogError),
    Domain(AppError),
    Provider(StatusCode, Value),
}

impl From<CatalogError> for ExternalError {
    fn from(error: CatalogError) -> Self {
        Self::Access(error)
    }
}

impl IntoResponse for ExternalError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Provider(status, value) => (status, Json(value)).into_response(),
            Self::Domain(error) => {
                if matches!(&error, AppError::Internal { .. })
                    || matches!(&error, AppError::Unauthorized { .. })
                    || matches!(
                        &error,
                        AppError::Forbidden {
                            code: "FORBIDDEN",
                            ..
                        }
                    )
                    || matches!(&error, AppError::NotFound { message, .. } if message == "organization not found")
                {
                    return CatalogError::Application(error).into_response();
                }
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
        }
    }
}

async fn register_allowlist(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    input: Result<Json<AllowlistInput>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ExternalError> {
    let Json(input) = input.map_err(|_| CatalogError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers).map_err(CatalogError::Identity)?;
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        CatalogError::Application(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })?;
    let mut tx = ovg_storage::tenancy::TenantTx::begin(
        pool,
        auth.actor_id(),
        auth.session_id(),
        auth.display_name(),
    )
    .await
    .map_err(ExternalError::Domain)?;
    let result =
        external_sources::register_allowlist(&mut tx, organization_id, auth.actor_id(), input)
            .await
            .map_err(ExternalError::Domain)?;
    tx.commit().await.map_err(ExternalError::Domain)?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn create_connection(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    input: Result<Json<ConnectionInput>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ExternalError> {
    let Json(input) = input.map_err(|_| CatalogError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers).map_err(CatalogError::Identity)?;
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let result = external_sources::create_connection(&mut edit, auth.actor_id(), input)
        .await
        .map_err(ExternalError::Domain)?;
    edit.commit().await.map_err(ExternalError::Domain)?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_allowlist(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(organization_id): Path<Uuid>,
) -> Result<Json<Value>, TenancyError> {
    let mut tx = tenancy::begin(&state, &auth).await?;
    let result = external_sources::list_allowlisted_hosts(&mut tx, organization_id).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn list_connections(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ExternalError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = external_sources::list_connections(&mut read)
        .await
        .map_err(ExternalError::Domain)?;
    read.commit().await.map_err(ExternalError::Domain)?;
    Ok(Json(result))
}

async fn invalidate_cache(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(scope): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ExternalError> {
    update(&state, &auth, &headers, scope, false).await
}

async fn revoke(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(scope): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ExternalError> {
    update(&state, &auth, &headers, scope, true).await
}

async fn update(
    state: &AppState,
    auth: &HumanSession,
    headers: &HeaderMap,
    (organization_id, project_id, connection_id): (Uuid, Uuid, Uuid),
    revoke: bool,
) -> Result<Json<Value>, ExternalError> {
    require_csrf(state, auth, headers).map_err(CatalogError::Identity)?;
    let mut edit = project_read(
        state,
        auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let result = external_sources::update_lifecycle(&mut edit, connection_id, revoke)
        .await
        .map_err(ExternalError::Domain)?;
    edit.commit().await.map_err(ExternalError::Domain)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Viewport {
    crs: i32,
    bbox: [f64; 4],
    width: u16,
    height: u16,
}

async fn preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(scope): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, ExternalError> {
    prove(&state, &auth, &headers, scope, None).await
}

async fn viewport(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(scope): Path<(Uuid, Uuid, Uuid)>,
    input: Result<Json<Viewport>, JsonRejection>,
) -> Result<Response, ExternalError> {
    let Json(view) = input.map_err(|_| CatalogError::InvalidRequest)?;
    if !(1..=512).contains(&view.width)
        || !(1..=512).contains(&view.height)
        || !view.bbox.iter().all(|v| v.is_finite())
    {
        return Err(CatalogError::InvalidRequest.into());
    }
    let value = json!({
        "crs": view.crs, "bbox": view.bbox,
        "width": view.width, "height": view.height
    });
    prove(&state, &auth, &headers, scope, Some(value)).await
}

async fn prove(
    state: &AppState,
    auth: &HumanSession,
    headers: &HeaderMap,
    (org, project, id): (Uuid, Uuid, Uuid),
    viewport: Option<Value>,
) -> Result<Response, ExternalError> {
    require_csrf(state, auth, headers).map_err(CatalogError::Identity)?;
    let mut read = project_read(state, auth, org, project, ProjectReadPolicy::Unrestricted).await?;
    let original = external_sources::provider_context(&mut read, id)
        .await
        .map_err(ExternalError::Domain)?;
    let mut result = crate::provider_worker::prove(&original, viewport)
        .await
        .map_err(|(status, body)| ExternalError::Provider(status, body))?;
    read.commit().await.map_err(ExternalError::Domain)?;
    // Fresh authorization and comparison before returning any external data.
    let mut read = project_read(state, auth, org, project, ProjectReadPolicy::Unrestricted).await?;
    let current = external_sources::provider_context(&mut read, id)
        .await
        .map_err(ExternalError::Domain)?;
    if original != current {
        return Err(ExternalError::Provider(
            StatusCode::FORBIDDEN,
            json!({"code":"PROVIDER_DISABLED","message":"provider profile changed"}),
        ));
    }
    read.commit().await.map_err(ExternalError::Domain)?;
    result["connection_id"] = json!(id);
    result["cache_generation"] = original["connection"]["cache_generation"].clone();
    let mut response = Json(result).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}
