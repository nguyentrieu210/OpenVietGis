//! E14 Rust-native project extension activation lifecycle.

use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_sdk::ModuleManifest;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::extensions as extension_store;
use ovg_storage::extensions::validation::{semver, validate_generic};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;

use super::{
    authenticate_actor, authenticate_json, decode_body, open_project_scope, parse_uuid,
    project_actor_display, require_actor_csrf, ExtensionError,
};

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/extensions";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/activations"),
            get(list_activations).post(activate_extension),
        )
        .route(
            &format!("{BASE}/{{extension_key}}/disable"),
            post(disable_extension),
        )
        .route(
            &format!("{BASE}/activations/{{activation_id}}/deactivate"),
            post(deactivate_extension),
        )
        .route(
            &format!("{BASE}/activations/{{activation_id}}/transition"),
            post(transition_extension),
        )
        .route(
            &format!("{BASE}/activations/{{activation_id}}/history"),
            get(list_activation_history),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivateRequest {
    manifest_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionRequest {
    expected_manifest_id: Uuid,
    target_manifest_id: Uuid,
}

fn validate_official_runtime(
    state: &AppState,
    manifest: &extension_store::ManifestContractRow,
) -> Result<(), ExtensionError> {
    let runtimes = state.modules.list_manifests();
    let Some(shipped) = runtimes
        .iter()
        .find(|runtime| runtime.extension_key == manifest.extension_key)
    else {
        return Ok(());
    };
    let stored = serde_json::from_value::<ModuleManifest>(manifest.contract()).ok();
    if !stored.is_some_and(|stored| shipped.runtime_contract_matches(&stored, manifest.enabled)) {
        return Err(ExtensionError::Domain(AppError::conflict(
            "OFFICIAL_MANIFEST_MISMATCH",
            "stored official manifest differs from shipped runtime",
        )));
    }
    Ok(())
}

fn validate_manifest_for_mutation(
    state: &AppState,
    manifest: &extension_store::ManifestContractRow,
) -> Result<(), ExtensionError> {
    if !validate_generic(manifest) {
        return Err(ExtensionError::Domain(AppError::conflict(
            "EXTENSION_MANIFEST_REVALIDATION_REQUIRED",
            "stored manifest no longer satisfies the metadata contract",
        )));
    }
    validate_official_runtime(state, manifest)
}

fn normalized_contract(manifest: &extension_store::ManifestContractRow) -> Value {
    json!({
        "extension_key": manifest.extension_key,
        "api_compatibility": manifest.api_compatibility,
        "permissions": manifest.permissions,
        "commands": manifest.commands,
        "capabilities": manifest.capabilities,
        "entity_schemas": manifest.entity_schemas,
    })
}

fn verify_transition(
    current: &extension_store::ManifestContractRow,
    target: &extension_store::ManifestContractRow,
) -> Result<(), ExtensionError> {
    if current.extension_key != target.extension_key
        || !target.enabled
        || current.api_compatibility != "1.x"
        || target.api_compatibility != "1.x"
    {
        return Err(ExtensionError::Domain(AppError::conflict(
            "EXTENSION_TRANSITION_INCOMPATIBLE",
            "incompatible extension transition",
        )));
    }
    let old = semver(&current.version);
    let new = semver(&target.version);
    if !old
        .zip(new)
        .is_some_and(|(old, new)| old.0 == new.0 && new > old)
    {
        return Err(ExtensionError::Domain(AppError::conflict(
            "EXTENSION_TRANSITION_INCOMPATIBLE",
            "only forward same-major versions are supported",
        )));
    }
    if normalized_contract(current) != normalized_contract(target) {
        return Err(ExtensionError::Domain(AppError::conflict(
            "EXTENSION_TRANSITION_MIGRATION_REQUIRED",
            "schema, permission or contribution changes require verified migration",
        )));
    }
    Ok(())
}

async fn activate_extension(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request<axum::body::Body>,
) -> Result<(StatusCode, Json<Value>), ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let body: ActivateRequest = decode_body(body)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let manifest = extension_store::manifest(&mut edit, body.manifest_id)
        .await
        .map_err(ExtensionError::Domain)?
        .filter(|manifest| manifest.enabled && manifest.api_compatibility == "1.x")
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::not_found("extension manifest not found"))
        })?;
    validate_manifest_for_mutation(&state, &manifest)?;
    let display = project_actor_display(&edit, &actor);
    let value = extension_store::activate(&mut edit, actor.actor_id(), &display, &manifest)
        .await
        .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn disable_extension(
    State(state): State<AppState>,
    Path((org, project, extension_key)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_actor_csrf(&state, &actor, &parts.headers)?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let display = project_actor_display(&edit, &actor);
    let value = extension_store::disable(&mut edit, actor.actor_id(), &display, &extension_key)
        .await
        .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(value))
}

async fn deactivate_extension(
    State(state): State<AppState>,
    Path((org, project, activation)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_actor_csrf(&state, &actor, &parts.headers)?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let activation_id = parse_uuid(&activation)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let display = project_actor_display(&edit, &actor);
    let value = extension_store::deactivate(&mut edit, actor.actor_id(), &display, activation_id)
        .await
        .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(value))
}

async fn transition_extension(
    State(state): State<AppState>,
    Path((org, project, activation)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let activation_id = parse_uuid(&activation)?;
    let body: TransitionRequest = decode_body(body)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;

    let current = extension_store::manifest(&mut edit, body.expected_manifest_id)
        .await
        .map_err(ExtensionError::Domain)?
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::not_found("extension manifest not found"))
        })?;
    let target = extension_store::manifest(&mut edit, body.target_manifest_id)
        .await
        .map_err(ExtensionError::Domain)?
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::not_found("extension manifest not found"))
        })?;

    validate_manifest_for_mutation(&state, &current)?;
    validate_manifest_for_mutation(&state, &target)?;
    verify_transition(&current, &target)?;
    validate_official_runtime(&state, &target)?;

    let display = project_actor_display(&edit, &actor);
    let value = extension_store::transition(
        &mut edit,
        actor.actor_id(),
        &display,
        activation_id,
        body.expected_manifest_id,
        body.target_manifest_id,
    )
    .await
    .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(value))
}

async fn list_activation_history(
    State(state): State<AppState>,
    Path((org, project, activation)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let activation_id = parse_uuid(&activation)?;
    let mut read = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = extension_store::activation_history(&mut read, activation_id)
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(result))
}

async fn list_activations(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let mut read = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::LayerAcl,
    )
    .await?;
    let rows = extension_store::list_activations(&mut read)
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(key: &str, version: &str) -> extension_store::ManifestContractRow {
        extension_store::ManifestContractRow {
            id: Uuid::new_v4(),
            extension_key: key.to_owned(),
            version: version.to_owned(),
            api_compatibility: "1.x".to_owned(),
            display_name: "Demo".to_owned(),
            description: String::new(),
            permissions: json!(["ext.demo.read"]),
            commands: json!([]),
            capabilities: json!([]),
            entity_schemas: json!({"thing":{"fields":{"name":{"type":"text"}}}}),
            ui_contributions: json!([]),
            enabled: true,
        }
    }

    #[test]
    fn transition_rules_match_reference() {
        let current = manifest("demo", "1.0.0");
        let mut target = manifest("demo", "1.1.0");
        target.permissions = current.permissions.clone();
        target.entity_schemas = current.entity_schemas.clone();
        assert!(verify_transition(&current, &target).is_ok());

        target.version = "2.0.0".to_owned();
        assert!(verify_transition(&current, &target).is_err());
        target.version = "1.1.0".to_owned();
        target.capabilities = json!(["changed"]);
        assert!(verify_transition(&current, &target).is_err());
    }
}
