//! E12 authenticated external-source metadata and cache lifecycle.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::external_sources;
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
            get(list_allowlist),
        )
        .route(BASE, get(list_connections))
        .route(
            &format!("{BASE}/{{connection_id}}/invalidate-cache"),
            post(invalidate_cache),
        )
        .route(&format!("{BASE}/{{connection_id}}/revoke"), post(revoke))
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
