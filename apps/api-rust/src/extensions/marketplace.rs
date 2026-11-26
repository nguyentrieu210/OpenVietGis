//! E14 public reviewed marketplace catalog and tenant-admin release installation.

use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::extensions::RegisterManifestInput;
use serde_json::{json, Value};

use crate::AppState;

use super::validation::load_catalog;
use super::{authenticate_actor, open_org_scope, parse_uuid, require_actor_csrf, ExtensionError};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/marketplace/catalog", get(marketplace_catalog))
        .route(
            "/organizations/{organization_id}/marketplace/install/{publisher_id}/{extension_key}/{version}",
            post(install_marketplace_release),
        )
}

async fn marketplace_catalog(State(state): State<AppState>) -> Result<Response, ExtensionError> {
    let releases = load_catalog(&state.config).map_err(ExtensionError::Domain)?;
    let items: Vec<Value> = releases.iter().map(|r| r.wire()).collect();
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "items": items })),
    )
        .into_response())
}

async fn install_marketplace_release(
    State(state): State<AppState>,
    Path((org, publisher_id, extension_key, version)): Path<(String, String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<(StatusCode, Json<Value>), ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_actor_csrf(&state, &actor, &parts.headers)?;
    let org_id = parse_uuid(&org)?;

    let releases = load_catalog(&state.config).map_err(ExtensionError::Domain)?;
    let release = releases
        .into_iter()
        .find(|r| {
            r.publisher_id == publisher_id
                && r.manifest.get("extension_key").and_then(Value::as_str)
                    == Some(extension_key.as_str())
                && r.manifest.get("version").and_then(Value::as_str) == Some(version.as_str())
        })
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::not_found(
                "reviewed marketplace release not found",
            ))
        })?;

    let m = release
        .manifest
        .as_object()
        .ok_or(ExtensionError::InvalidRequest)?;
    let mut scope = open_org_scope(&state, &actor, org_id).await?;
    let rec = scope
        .register_manifest(RegisterManifestInput {
            extension_key: m["extension_key"].as_str().unwrap_or_default(),
            version: m["version"].as_str().unwrap_or_default(),
            api_compatibility: m["api_compatibility"].as_str().unwrap_or_default(),
            display_name: m["display_name"].as_str().unwrap_or_default(),
            description: m["description"].as_str().unwrap_or_default(),
            permissions: &m["permissions"],
            commands: &m["commands"],
            capabilities: &m["capabilities"],
            entity_schemas: &m["entity_schemas"],
            ui_contributions: &m["ui_contributions"],
        })
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;
    Ok((StatusCode::CREATED, Json(rec.wire())))
}
