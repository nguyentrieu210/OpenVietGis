//! E14 Rust-native project extension activation lifecycle.

use std::collections::HashSet;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_sdk::ModuleManifest;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::extensions as extension_store;
use ovg_storage::integration_hub::{self, EffectiveProjectLevel};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/extensions";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/activations"),
            get(list_activations).post(activate_extension),
        )
        .route(
            &format!("{BASE}/{{extension_key}}/disable"),
            axum::routing::post(disable_extension),
        )
        .route(
            &format!("{BASE}/activations/{{activation_id}}/deactivate"),
            axum::routing::post(deactivate_extension),
        )
        .route(
            &format!("{BASE}/activations/{{activation_id}}/transition"),
            axum::routing::post(transition_extension),
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

async fn manage_scope<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<ProjectRead<'a>, ExtensionWriteError> {
    // Open unrestricted read first so an editor/viewer reaches the exact
    // project.manage decision instead of being rejected as project.edit.
    let mut scope = project_read(
        state,
        auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(ExtensionWriteError::Access)?;
    let level = integration_hub::catalog_context(&mut scope, auth.actor_id())
        .await
        .map_err(ExtensionWriteError::Domain)?
        .level;
    if level != EffectiveProjectLevel::Manage {
        return Err(ExtensionWriteError::Permission(
            "missing permission: project.manage",
        ));
    }
    Ok(scope)
}

async fn activate_extension(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    body: Result<Json<ActivateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ExtensionWriteError> {
    let Json(body) = body.map_err(|_| ExtensionWriteError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers)
        .map_err(|error| ExtensionWriteError::Access(CatalogError::Identity(error)))?;
    let mut edit = manage_scope(&state, &auth, organization_id, project_id).await?;
    let manifest = extension_store::manifest(&mut edit, body.manifest_id)
        .await
        .map_err(ExtensionWriteError::Domain)?
        .filter(|manifest| manifest.enabled && manifest.api_compatibility == "1.x")
        .ok_or_else(|| {
            ExtensionWriteError::Domain(AppError::not_found("extension manifest not found"))
        })?;
    validate_manifest_for_mutation(&state, &manifest)?;
    let value =
        extension_store::activate(&mut edit, auth.actor_id(), auth.display_name(), &manifest)
            .await
            .map_err(ExtensionWriteError::Domain)?;
    edit.commit().await.map_err(ExtensionWriteError::Domain)?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn disable_extension(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, extension_key)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<Value>, ExtensionWriteError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| ExtensionWriteError::Access(CatalogError::Identity(error)))?;
    let mut edit = manage_scope(&state, &auth, organization_id, project_id).await?;
    let value = extension_store::disable(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        &extension_key,
    )
    .await
    .map_err(ExtensionWriteError::Domain)?;
    edit.commit().await.map_err(ExtensionWriteError::Domain)?;
    Ok(Json(value))
}

async fn deactivate_extension(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, activation_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ExtensionWriteError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| ExtensionWriteError::Access(CatalogError::Identity(error)))?;
    let mut edit = manage_scope(&state, &auth, organization_id, project_id).await?;
    let value = extension_store::deactivate(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        activation_id,
    )
    .await
    .map_err(ExtensionWriteError::Domain)?;
    edit.commit().await.map_err(ExtensionWriteError::Domain)?;
    Ok(Json(value))
}

async fn transition_extension(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, activation_id)): Path<(Uuid, Uuid, Uuid)>,
    body: Result<Json<TransitionRequest>, JsonRejection>,
) -> Result<Json<Value>, ExtensionWriteError> {
    let Json(body) = body.map_err(|_| ExtensionWriteError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers)
        .map_err(|error| ExtensionWriteError::Access(CatalogError::Identity(error)))?;
    let mut edit = manage_scope(&state, &auth, organization_id, project_id).await?;

    let current = extension_store::manifest(&mut edit, body.expected_manifest_id)
        .await
        .map_err(ExtensionWriteError::Domain)?
        .ok_or_else(|| {
            ExtensionWriteError::Domain(AppError::not_found("extension manifest not found"))
        })?;
    let target = extension_store::manifest(&mut edit, body.target_manifest_id)
        .await
        .map_err(ExtensionWriteError::Domain)?
        .ok_or_else(|| {
            ExtensionWriteError::Domain(AppError::not_found("extension manifest not found"))
        })?;

    validate_manifest_for_mutation(&state, &current)?;
    validate_manifest_for_mutation(&state, &target)?;
    verify_transition(&current, &target)?;
    validate_official_runtime(&state, &target)?;

    let value = extension_store::transition(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        activation_id,
        body.expected_manifest_id,
        body.target_manifest_id,
    )
    .await
    .map_err(ExtensionWriteError::Domain)?;
    edit.commit().await.map_err(ExtensionWriteError::Domain)?;
    Ok(Json(value))
}

fn valid_extension_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    (2..=100).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| matches!(*byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

fn valid_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn semver(value: &str) -> Option<(u64, u64, u64)> {
    if value.len() > 32 {
        return None;
    }
    let mut parts = value.split('.');
    let parse = |part: &str| {
        (!part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part.len() == 1 || !part.starts_with('0')))
        .then(|| part.parse::<u64>().ok())
        .flatten()
    };
    let major = parse(parts.next()?)?;
    let minor = parse(parts.next()?)?;
    let patch = parse(parts.next()?)?;
    parts.next().is_none().then_some((major, minor, patch))
}

fn string_array(value: &Value) -> Option<Vec<&str>> {
    value
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()
}

fn portable(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => true,
        Value::Number(number) => {
            const LIMIT: i128 = 9_007_199_254_740_991;
            if let Some(value) = number.as_i64() {
                i128::from(value).abs() <= LIMIT
            } else if let Some(value) = number.as_u64() {
                i128::from(value) <= LIMIT
            } else {
                number
                    .as_f64()
                    .is_some_and(|value| value.is_finite() && value.abs() <= LIMIT as f64)
            }
        }
        Value::Array(items) => items.iter().all(portable),
        Value::Object(object) => object.values().all(portable),
    }
}

fn validate_partner(manifest: &extension_store::ManifestContractRow) -> bool {
    const CAPS: [&str; 7] = [
        "host_context",
        "host_notice",
        "map_view",
        "map_navigate",
        "map_selection",
        "map_overlay",
        "tool_register",
    ];
    let permissions = string_array(&manifest.permissions);
    let commands = manifest.commands.as_array();
    let schemas = manifest.entity_schemas.as_object();
    let capabilities = string_array(&manifest.capabilities);
    let ui = manifest.ui_contributions.as_array();
    let Some(capabilities) = capabilities else {
        return false;
    };
    let unique: HashSet<_> = capabilities.iter().copied().collect();
    if !manifest.extension_key.starts_with("partner_")
        || manifest.extension_key.len() <= "partner_".len()
        || permissions.is_none_or(|values| !values.is_empty())
        || commands.is_none_or(|values| !values.is_empty())
        || schemas.is_none_or(|values| !values.is_empty())
        || capabilities.is_empty()
        || unique.len() != capabilities.len()
        || capabilities.iter().any(|cap| !CAPS.contains(cap))
        || ui.is_none_or(|values| values.len() != 1)
    {
        return false;
    }
    let Some(object) = ui.and_then(|values| values[0].as_object()) else {
        return false;
    };
    let expected = [
        "id",
        "slot",
        "label",
        "route_suffix",
        "runtime",
        "url",
        "sdk_version",
    ];
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return false;
    }
    let label_ok = object
        .get("label")
        .and_then(Value::as_str)
        .is_some_and(|label| !label.trim().is_empty() && label.len() <= 200);
    let fixed = object.get("id").and_then(Value::as_str) == Some("partner_panel")
        && object.get("slot").and_then(Value::as_str) == Some("project_panel")
        && object.get("runtime").and_then(Value::as_str) == Some("sandboxed_iframe")
        && object.get("sdk_version").and_then(Value::as_str) == Some("1.0")
        && object.get("route_suffix").and_then(Value::as_str) == Some("/partner");
    fixed
        && label_ok
        && object
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(valid_partner_url)
}

fn valid_partner_url(url: &str) -> bool {
    if url.len() > 2048
        || url
            .bytes()
            .any(|byte| byte <= 0x20 || byte >= 0x7f || byte == 0x5c)
        || url.contains('?')
        || url.contains('#')
    {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    if host.is_empty()
        || host.contains(':')
        || host.contains('@')
        || host == "openvietgis.cloud"
        || host.ends_with(".openvietgis.cloud")
        || host.ends_with(".local")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".test")
        || !path.is_empty() && !url.contains('/')
    {
        return false;
    }
    let labels = host.split('.').collect::<Vec<_>>();
    labels.len() >= 2
        && labels.last().is_some_and(|label| {
            (2..=63).contains(&label.len()) && label.bytes().all(|byte| byte.is_ascii_lowercase())
        })
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn validate_generic(manifest: &extension_store::ManifestContractRow) -> bool {
    if !valid_extension_key(&manifest.extension_key)
        || semver(&manifest.version).is_none()
        || manifest.api_compatibility != "1.x"
    {
        return false;
    }
    if manifest.extension_key.starts_with("partner_") {
        return validate_partner(manifest);
    }
    let Some(permissions) = string_array(&manifest.permissions) else {
        return false;
    };
    let prefix = format!("ext.{}.", manifest.extension_key);
    let permission_set: HashSet<_> = permissions.iter().copied().collect();
    if permissions.len() > 64
        || permission_set.len() != permissions.len()
        || permissions.iter().any(|permission| {
            !permission
                .strip_prefix(&prefix)
                .is_some_and(valid_identifier)
        })
    {
        return false;
    }
    let Some(schemas) = manifest.entity_schemas.as_object() else {
        return false;
    };
    if schemas.is_empty() || schemas.len() > 100 {
        return false;
    }
    for (entity, schema) in schemas {
        let Some(schema) = schema.as_object() else {
            return false;
        };
        if !valid_identifier(entity) || schema.len() != 1 || !schema.contains_key("fields") {
            return false;
        }
        let Some(fields) = schema["fields"].as_object() else {
            return false;
        };
        if fields.is_empty() || fields.len() > 100 {
            return false;
        }
        for (name, spec) in fields {
            let Some(spec) = spec.as_object() else {
                return false;
            };
            if !valid_identifier(name)
                || spec.keys().any(|key| key != "type" && key != "required")
                || !spec
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| matches!(kind, "text" | "integer" | "number" | "boolean"))
                || spec
                    .get("required")
                    .is_some_and(|required| !required.is_boolean())
            {
                return false;
            }
        }
    }
    let Some(commands) = manifest.commands.as_array() else {
        return false;
    };
    if commands.len() > 64 {
        return false;
    }
    let mut command_ids = HashSet::new();
    for command in commands {
        let Some(command) = command.as_object() else {
            return false;
        };
        let id = command.get("id").and_then(Value::as_str);
        let label = command.get("label").and_then(Value::as_str);
        let required = command.get("required_permission").and_then(Value::as_str);
        if command.len() != 3
            || !command.contains_key("id")
            || !command.contains_key("label")
            || !command.contains_key("required_permission")
            || id.is_none_or(|id| !valid_identifier(id) || !command_ids.insert(id))
            || label.is_none_or(|label| label.trim().is_empty() || label.len() > 200)
            || required.is_none_or(|required| !permission_set.contains(required))
        {
            return false;
        }
    }
    let Some(capabilities) = string_array(&manifest.capabilities) else {
        return false;
    };
    let capability_set: HashSet<_> = capabilities.iter().copied().collect();
    if capabilities.len() > 64
        || capability_set.len() != capabilities.len()
        || capabilities
            .iter()
            .any(|capability| !valid_identifier(capability))
    {
        return false;
    }
    let Some(ui) = manifest.ui_contributions.as_array() else {
        return false;
    };
    if ui.len() > 64 {
        return false;
    }
    let mut ui_ids = HashSet::new();
    for contribution in ui {
        let Some(contribution) = contribution.as_object() else {
            return false;
        };
        if contribution.contains_key("runtime") {
            return false;
        }
        let id = contribution.get("id").and_then(Value::as_str);
        let label = contribution.get("label").and_then(Value::as_str);
        let route = contribution.get("route_suffix").and_then(Value::as_str);
        let slot = contribution.get("slot").and_then(Value::as_str);
        if id.is_none_or(|id| !valid_identifier(id) || !ui_ids.insert(id))
            || !slot.is_some_and(|slot| {
                matches!(slot, "project_tool" | "project_panel" | "project_route")
            })
            || label.is_none_or(|label| label.trim().is_empty())
            || route.is_none_or(|route| !route.starts_with('/') || route.contains(".."))
        {
            return false;
        }
        if contribution
            .get("required_permission")
            .is_some_and(|required| {
                !required
                    .as_str()
                    .is_some_and(|required| permission_set.contains(required))
            })
        {
            return false;
        }
    }
    let bounded = json!({
        "permissions": manifest.permissions,
        "commands": manifest.commands,
        "capabilities": manifest.capabilities,
        "schemas": manifest.entity_schemas,
        "ui_contributions": manifest.ui_contributions,
    });
    portable(&bounded)
        && serde_json::to_vec(&bounded).is_ok_and(|encoded| encoded.len() <= 128 * 1024)
}

fn validate_official_runtime(
    state: &AppState,
    manifest: &extension_store::ManifestContractRow,
) -> Result<(), ExtensionWriteError> {
    let runtimes = state.modules.list_manifests();
    let Some(shipped) = runtimes
        .iter()
        .find(|runtime| runtime.extension_key == manifest.extension_key)
    else {
        return Ok(());
    };
    let stored = serde_json::from_value::<ModuleManifest>(manifest.contract()).ok();
    if !stored.is_some_and(|stored| shipped.runtime_contract_matches(&stored, manifest.enabled)) {
        return Err(ExtensionWriteError::Domain(AppError::conflict(
            "OFFICIAL_MANIFEST_MISMATCH",
            "stored official manifest differs from shipped runtime",
        )));
    }
    Ok(())
}

fn validate_manifest_for_mutation(
    state: &AppState,
    manifest: &extension_store::ManifestContractRow,
) -> Result<(), ExtensionWriteError> {
    if !validate_generic(manifest) {
        return Err(ExtensionWriteError::Domain(AppError::conflict(
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
) -> Result<(), ExtensionWriteError> {
    if current.extension_key != target.extension_key
        || !target.enabled
        || current.api_compatibility != "1.x"
        || target.api_compatibility != "1.x"
    {
        return Err(ExtensionWriteError::Domain(AppError::conflict(
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
        return Err(ExtensionWriteError::Domain(AppError::conflict(
            "EXTENSION_TRANSITION_INCOMPATIBLE",
            "only forward same-major versions are supported",
        )));
    }
    if normalized_contract(current) != normalized_contract(target) {
        return Err(ExtensionWriteError::Domain(AppError::conflict(
            "EXTENSION_TRANSITION_MIGRATION_REQUIRED",
            "schema, permission or contribution changes require verified migration",
        )));
    }
    Ok(())
}

async fn list_activation_history(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, activation_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ExtensionHistoryError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(ExtensionHistoryError::Access)?;
    let result = extension_store::activation_history(&mut read, activation_id)
        .await
        .map_err(ExtensionHistoryError::Domain)?;
    read.commit().await.map_err(ExtensionHistoryError::Domain)?;
    Ok(Json(result))
}

enum ExtensionHistoryError {
    Access(CatalogError),
    Domain(AppError),
}

impl IntoResponse for ExtensionHistoryError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "extension history request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
        }
    }
}

async fn list_activations(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
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

enum ExtensionWriteError {
    InvalidRequest,
    Permission(&'static str),
    Access(CatalogError),
    Domain(AppError),
}

impl IntoResponse for ExtensionWriteError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            Self::Permission(detail) => {
                (StatusCode::FORBIDDEN, Json(json!({"detail": detail}))).into_response()
            }
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "extension mutation failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
        }
    }
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
    fn generic_manifest_and_transition_rules_match_reference() {
        let current = manifest("demo", "1.0.0");
        let mut target = manifest("demo", "1.1.0");
        target.permissions = current.permissions.clone();
        target.entity_schemas = current.entity_schemas.clone();
        assert!(validate_generic(&current));
        assert!(verify_transition(&current, &target).is_ok());

        target.version = "2.0.0".to_owned();
        assert!(verify_transition(&current, &target).is_err());
        target.version = "1.1.0".to_owned();
        target.capabilities = json!(["changed"]);
        assert!(verify_transition(&current, &target).is_err());
    }

    #[test]
    fn partner_contract_is_isolated() {
        let mut partner = manifest("partner_demo", "1.0.0");
        partner.permissions = json!([]);
        partner.entity_schemas = json!({});
        partner.capabilities = json!(["host_context", "map_view"]);
        partner.ui_contributions = json!([{
            "id":"partner_panel",
            "slot":"project_panel",
            "label":"Partner",
            "route_suffix":"/partner",
            "runtime":"sandboxed_iframe",
            "url":"https://partner.example.com/app",
            "sdk_version":"1.0"
        }]);
        assert!(validate_generic(&partner));
        partner.ui_contributions[0]["url"] = json!("https://openvietgis.cloud/app");
        assert!(!validate_generic(&partner));
    }
}
