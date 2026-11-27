//! E12 authenticated external-source metadata and cache lifecycle.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::external_sources::{self, AllowlistInput, ConnectionInput};
use ovg_core::AppError;
use serde_json::Value;
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
}

fn domain_error(error: AppError) -> CatalogError {
    match error {
        AppError::Validation { code, message, .. } =>
            CatalogError::Spatial(StatusCode::UNPROCESSABLE_ENTITY, code, message),
        AppError::Forbidden { code, message } =>
            CatalogError::Spatial(StatusCode::FORBIDDEN, code, message),
        AppError::Conflict { code, message, .. } =>
            CatalogError::Spatial(StatusCode::CONFLICT, code, message),
        error => CatalogError::Application(error),
    }
}

async fn register_allowlist(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<Uuid>,
    Json(input): Json<AllowlistInput>,
) -> Result<(StatusCode, Json<Value>), CatalogError> {
    require_csrf(&state, &auth, &headers).map_err(CatalogError::Identity)?;
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        CatalogError::Application(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE", "database unavailable",
        ))
    })?;
    let mut tx = ovg_storage::tenancy::TenantTx::begin(
        pool, auth.actor_id(), auth.session_id(), auth.display_name(),
    ).await.map_err(domain_error)?;
    let result = external_sources::register_allowlist(
        &mut tx, organization_id, auth.actor_id(), input,
    ).await.map_err(domain_error)?;
    tx.commit().await.map_err(domain_error)?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn create_connection(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<ConnectionInput>,
) -> Result<(StatusCode, Json<Value>), CatalogError> {
    require_csrf(&state, &auth, &headers).map_err(CatalogError::Identity)?;
    let mut edit = project_read(
        &state, &auth, organization_id, project_id, ProjectReadPolicy::UnrestrictedEdit,
    ).await?;
    let result = external_sources::create_connection(&mut edit, auth.actor_id(), input)
        .await.map_err(domain_error)?;
    edit.commit().await.map_err(domain_error)?;
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
) -> Result<Json<Value>, CatalogError> {
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
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}

async fn invalidate_cache(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(scope): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    update(&state, &auth, &headers, scope, false).await
}

async fn revoke(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(scope): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    update(&state, &auth, &headers, scope, true).await
}

async fn update(
    state: &AppState,
    auth: &HumanSession,
    headers: &HeaderMap,
    (organization_id, project_id, connection_id): (Uuid, Uuid, Uuid),
    revoke: bool,
) -> Result<Json<Value>, CatalogError> {
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
        .map_err(CatalogError::Application)?;
    edit.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}
