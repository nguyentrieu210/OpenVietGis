//! E08 Rust-native processing control plane.
//!
//! The geometry engines remain replaceable OSS workers. This module owns the
//! authenticated OpenVietGIS API contract and algorithm registry.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/processing";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(&format!("{BASE}/algorithms"), get(list_algorithms))
}

fn algorithm_registry_wire() -> Value {
    serde_json::json!({ "items": ovg_processing::registry::algorithm_registry() })
}

async fn list_algorithms(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    // Match FastAPI context_for(...): this is an unrestricted project.read
    // contract, so GIS-scoped actors fail closed.
    let _scope = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;

    Ok(Json(algorithm_registry_wire()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_frozen_python_contract() {
        let body = algorithm_registry_wire();
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["id"], "buffer");
        assert_eq!(items[0]["parameters_schema"]["quad_segs"]["default"], 8);
        assert_eq!(items[1]["required_inputs"][1], "overlay_layer_id");
        assert_eq!(
            items[2]["parameters_schema"]["group_by_field"]["nullable"],
            true
        );
    }
}
