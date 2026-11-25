//! E12 integration-hub HTTP catalog.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::routing::get;
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::integration_hub::{self, EffectiveProjectLevel};
use serde_json::{json, Value};
use uuid::Uuid;

use super::catalog::{serialize_adapter, PermissionLevel, ADAPTERS};
use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integration-hub";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(&format!("{BASE}/catalog"), get(read_catalog))
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

    let permission = match context.level {
        EffectiveProjectLevel::Read => PermissionLevel::Read,
        EffectiveProjectLevel::Edit => PermissionLevel::Edit,
        EffectiveProjectLevel::Manage => PermissionLevel::Manage,
    };
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
