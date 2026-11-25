//! E14 Rust-native project extension activation read control plane.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ovg_module_sdk::ModuleManifest;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::extensions as extension_store;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/extensions";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(&format!("{BASE}/activations"), get(list_activations))
}

async fn list_activations(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    // Python's PROJECT_READ scope permits GIS-scoped actors; extension rows
    // remain project-RLS protected and contain no layer data.
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let rows = extension_store::list_activations(&mut read)
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;

    let runtimes = state.modules.list_manifests();
    let items = rows
        .into_iter()
        .map(|row| {
            let mut wire = row.wire;
            let object = wire
                .as_object_mut()
                .expect("activation wire is always a JSON object");
            object.insert("manifest_version".to_string(), json!(row.manifest_version));
            if let Some(shipped) = runtimes
                .iter()
                .find(|manifest| manifest.extension_key == row.extension_key)
            {
                let compatible = serde_json::from_value::<ModuleManifest>(row.manifest_contract)
                    .ok()
                    .is_some_and(|stored| {
                        shipped.runtime_contract_matches(&stored, row.manifest_enabled)
                    });
                object.insert("runtime_compatible".to_string(), json!(compatible));
            }
            wire
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "items": items })))
}
