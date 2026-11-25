//! E08 Rust-native processing control plane.
//!
//! The geometry engines remain replaceable OSS workers. This module owns the
//! authenticated OpenVietGIS API contract and algorithm registry.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/processing";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(&format!("{BASE}/algorithms"), get(list_algorithms))
}

fn algorithm_registry() -> Value {
    json!({
        "items": [
            {
                "id": "buffer",
                "name": "Buffer",
                "description": "Creates projected-distance buffers (Web Mercator for geographic inputs) and publishes a new Polygon layer.",
                "required_inputs": ["input_layer_id"],
                "parameters_schema": {
                    "distance_meters": {"type": "number", "min": 0.1, "max": 500000.0},
                    "quad_segs": {"type": "integer", "min": 2, "max": 16, "default": 8}
                }
            },
            {
                "id": "clip",
                "name": "Clip",
                "description": "Clips input layer features to an authorized overlay polygon layer.",
                "required_inputs": ["input_layer_id", "overlay_layer_id"],
                "parameters_schema": {}
            },
            {
                "id": "dissolve",
                "name": "Dissolve",
                "description": "Unions features globally or grouped by an attribute field into a new layer.",
                "required_inputs": ["input_layer_id"],
                "parameters_schema": {
                    "group_by_field": {"type": "string", "nullable": true}
                }
            }
        ]
    })
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

    Ok(Json(algorithm_registry()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_frozen_python_contract() {
        let body = algorithm_registry();
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
