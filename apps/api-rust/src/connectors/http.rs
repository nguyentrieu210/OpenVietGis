//! E12 integration-hub HTTP catalog.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::routing::get;
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::integration_hub::{self, EffectiveProjectLevel};
use serde_json::{json, Value};
use uuid::Uuid;

use super::catalog::{get_adapter, serialize_adapter, PermissionLevel, ADAPTERS};
use super::profile::{connection_profile, execution_status, serialize_layer};
use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integration-hub";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/catalog"), get(read_catalog))
        .route(
            &format!("{BASE}/adapters/{{adapter_key}}"),
            get(read_adapter_profile),
        )
        .merge(super::bindings::router())
}

fn permission(level: EffectiveProjectLevel) -> PermissionLevel {
    match level {
        EffectiveProjectLevel::Read => PermissionLevel::Read,
        EffectiveProjectLevel::Edit => PermissionLevel::Edit,
        EffectiveProjectLevel::Manage => PermissionLevel::Manage,
    }
}

async fn read_adapter_profile(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, adapter_key)): Path<(Uuid, Uuid, String)>,
) -> Result<(HeaderMap, Json<Value>), CatalogError> {
    let adapter = get_adapter(&adapter_key).ok_or_else(|| {
        CatalogError::Application(ovg_core::AppError::not_found(
            "Integration adapter not found.",
        ))
    })?;
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let context = integration_hub::catalog_context(&mut read, auth.actor_id())
        .await
        .map_err(CatalogError::Application)?;
    let connection = connection_profile(
        adapter.key,
        &organization_id.to_string(),
        &project_id.to_string(),
    );
    let layers = if connection.is_some() {
        integration_hub::accessible_layers(&mut read, context.scoped_access)
            .await
            .map_err(CatalogError::Application)?
            .iter()
            .map(serialize_layer)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    read.commit().await.map_err(CatalogError::Application)?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok((
        headers,
        Json(json!({
            "schema_version": 1,
            "organization_id": organization_id,
            "project_id": project_id,
            "scoped_access": context.scoped_access,
            "default_write_policy": "deny",
            "adapter": serialize_adapter(adapter, permission(context.level)),
            "execution_status": execution_status(adapter),
            "connection_profile": connection,
            "layers": layers,
        })),
    ))
}

async fn read_catalog(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), CatalogError> {
    // Unlike raster/CAD, the catalog is deliberately visible to GIS-scoped
    // actors and reports that fact to the client.
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let context = integration_hub::catalog_context(&mut read, auth.actor_id())
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;

    let permission = permission(context.level);
    let adapters = ADAPTERS
        .iter()
        .map(|adapter| serialize_adapter(adapter, permission))
        .collect::<Vec<_>>();

    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok((
        headers,
        Json(json!({
            "schema_version": 1,
            "organization_id": organization_id,
            "project_id": project_id,
            "mode": "catalog_read_only",
            "scoped_access": context.scoped_access,
            "default_write_policy": "deny",
            "adapters": adapters,
        })),
    ))
}
