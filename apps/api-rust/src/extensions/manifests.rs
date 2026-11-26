//! E14 organization extension manifests and official module registration routes.

use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_sdk::ModuleManifest;
use ovg_storage::extensions::RegisterManifestInput;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

use super::{
    authenticate_actor, authenticate_json, decode_body, open_org_scope, parse_uuid,
    require_actor_csrf, ExtensionError,
};

const ORG: &str = "/organizations/{organization_id}/extensions";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{ORG}/manifests"),
            get(list_manifests).post(register_manifest),
        )
        .route(&format!("{ORG}/official"), get(list_official_catalog))
        .route(
            &format!("{ORG}/official/{{extension_key}}/register"),
            post(register_official_manifest),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterManifestBody {
    extension_key: String,
    version: String,
    #[serde(default = "default_compat")]
    api_compatibility: String,
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "empty_array")]
    permissions: Value,
    #[serde(default = "empty_array")]
    commands: Value,
    #[serde(default = "empty_array")]
    capabilities: Value,
    entity_schemas: Value,
    #[serde(default = "empty_array")]
    ui_contributions: Value,
}

fn default_compat() -> String {
    "1.x".to_owned()
}

fn empty_array() -> Value {
    Value::Array(Vec::new())
}

async fn list_manifests(
    State(state): State<AppState>,
    Path(org): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let mut scope = open_org_scope(&state, &actor, org_id).await?;
    let items = scope
        .list_manifests()
        .await
        .map_err(ExtensionError::Domain)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(json!({
        "items": items.iter().map(|m| m.wire()).collect::<Vec<_>>()
    })))
}

async fn register_manifest(
    State(state): State<AppState>,
    Path(org): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<(StatusCode, Json<Value>), ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let org_id = parse_uuid(&org)?;
    let body: RegisterManifestBody = decode_body(body)?;
    if !body.entity_schemas.is_object()
        || !body.permissions.is_array()
        || !body.commands.is_array()
        || !body.capabilities.is_array()
        || !body.ui_contributions.is_array()
    {
        return Err(ExtensionError::InvalidRequest);
    }
    if state
        .modules
        .list_manifests()
        .iter()
        .any(|m| m.extension_key == body.extension_key)
    {
        return Err(ExtensionError::Domain(AppError::conflict(
            "OFFICIAL_MANIFEST_REQUIRED",
            "official runtime keys must use the official registration endpoint",
        )));
    }
    let mut scope = open_org_scope(&state, &actor, org_id).await?;
    let rec = scope
        .register_manifest(RegisterManifestInput {
            extension_key: &body.extension_key,
            version: &body.version,
            api_compatibility: &body.api_compatibility,
            display_name: &body.display_name,
            description: &body.description,
            permissions: &body.permissions,
            commands: &body.commands,
            capabilities: &body.capabilities,
            entity_schemas: &body.entity_schemas,
            ui_contributions: &body.ui_contributions,
        })
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;
    Ok((StatusCode::CREATED, Json(rec.wire())))
}

async fn list_official_catalog(
    State(state): State<AppState>,
    Path(org): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let mut scope = open_org_scope(&state, &actor, org_id).await?;
    let _ = scope
        .list_manifests()
        .await
        .map_err(ExtensionError::Domain)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;

    let items: Vec<Value> = state
        .modules
        .list_manifests()
        .into_iter()
        .map(|m| {
            json!({
                "extension_key": m.extension_key,
                "version": m.version,
                "display_name": m.display_name,
                "description": m.description,
                "capabilities": m.capabilities,
                "ui_contributions": m.ui_contributions,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

async fn register_official_manifest(
    State(state): State<AppState>,
    Path((org, extension_key)): Path<(String, String)>,
    request: Request<axum::body::Body>,
) -> Result<(StatusCode, Json<Value>), ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_actor_csrf(&state, &actor, &parts.headers)?;
    let org_id = parse_uuid(&org)?;

    let shipped = state
        .modules
        .list_manifests()
        .into_iter()
        .find(|m| m.extension_key == extension_key)
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::not_found("official extension not found"))
        })?;

    let mut scope = open_org_scope(&state, &actor, org_id).await?;
    let existing = scope
        .list_manifests()
        .await
        .map_err(ExtensionError::Domain)?;
    if let Some(found) = existing.into_iter().find(|m| {
        m.contract.extension_key == extension_key && m.contract.version == shipped.version
    }) {
        let stored = serde_json::from_value::<ModuleManifest>(found.contract.contract()).ok();
        if !stored.is_some_and(|s| shipped.runtime_contract_matches(&s, found.contract.enabled)) {
            return Err(ExtensionError::Domain(AppError::conflict(
                "OFFICIAL_MANIFEST_MISMATCH",
                "stored official manifest differs from shipped runtime",
            )));
        }
        scope.commit().await.map_err(ExtensionError::Domain)?;
        return Ok((StatusCode::CREATED, Json(found.wire())));
    }

    let permissions = serde_json::to_value(&shipped.permissions).unwrap_or_else(|_| json!([]));
    let commands = serde_json::to_value(&shipped.commands).unwrap_or_else(|_| json!([]));
    let capabilities = serde_json::to_value(&shipped.capabilities).unwrap_or_else(|_| json!([]));
    let ui_contributions =
        serde_json::to_value(&shipped.ui_contributions).unwrap_or_else(|_| json!([]));

    let rec = scope
        .register_manifest(RegisterManifestInput {
            extension_key: &shipped.extension_key,
            version: &shipped.version,
            api_compatibility: &shipped.api_compatibility,
            display_name: &shipped.display_name,
            description: &shipped.description,
            permissions: &permissions,
            commands: &commands,
            capabilities: &capabilities,
            entity_schemas: &shipped.entity_schemas,
            ui_contributions: &ui_contributions,
        })
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;
    Ok((StatusCode::CREATED, Json(rec.wire())))
}
