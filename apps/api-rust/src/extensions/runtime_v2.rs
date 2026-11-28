//! Opt-in read-only declarative host. No arbitrary code, files, SQL, GIS writes,
//! or network access. Signed catalog and canonical activation checked on every call.
use super::{
    authenticate_actor, authenticate_json, decode_body,
    marketplace_v2::{load_verified, Profile, VerifiedRelease},
    open_project_scope, parse_uuid, require_actor_csrf,
    validation::marketplace_canonical,
    ExtensionError,
};
use crate::AppState;
use axum::{
    extract::{Path, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use ovg_core::{ActorContext, ActorType, AppError, Permission, TenantScope};
use ovg_module_sdk::declarative::{
    authorize, ActiveGrant, CommandPolicy, Consent, Document, FieldType, Invocation,
};
use ovg_storage::{
    catalog::{ProjectRead, ProjectReadPolicy},
    extensions::runtime_v2::{active_pin, receipt, ActivePin, ReceiptInput},
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const BASE: &str =
    "/organizations/{organization_id}/projects/{project_id}/extensions/{manifest_id}/v2-runtime";
static INVOCATIONS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
fn invalid(msg: &str) -> AppError {
    AppError::validation("INVALID_PLUGIN_RUNTIME", msg.to_owned())
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, get(document))
        .route(&format!("{BASE}/invoke"), post(invoke))
}
struct Ready {
    doc: Document,
    permissions: BTreeSet<String>,
    policies: BTreeMap<String, CommandPolicy>,
    digest: String,
}
/// Only reviewed v1 host_context/host_notice capabilities can be translated.
/// These commands cannot read resource contents or perform material mutations.
fn qualify(pin: &ActivePin, releases: &[VerifiedRelease]) -> Result<Ready, AppError> {
    let md =
        hash(&marketplace_canonical(&pin.manifest).map_err(|_| invalid("manifest serialization"))?);
    let release = releases
        .iter()
        .find(|r| r.metadata["envelope"]["manifest_digest"].as_str() == Some(md.as_str()))
        .ok_or_else(|| AppError::not_found("reviewed compatible release not found"))?;
    let e = &release.metadata["envelope"];
    let key = pin.manifest["extension_key"]
        .as_str()
        .ok_or_else(|| invalid("invalid extension"))?;
    if !key.starts_with("partner_")
        || e["extension_key"].as_str() != Some(key)
        || e["version"] != pin.manifest["version"]
        || e["assets"].as_array().is_none_or(|a| !a.is_empty())
    {
        return Err(invalid("unsupported runtime profile"));
    }
    let caps = pin.manifest["capabilities"]
        .as_array()
        .ok_or_else(|| invalid("invalid capabilities"))?;
    let mut policies = BTreeMap::new();
    for command in ["host_context", "host_notice"] {
        if caps.iter().any(|v| v.as_str() == Some(command)) {
            policies.insert(
                command.to_owned(),
                CommandPolicy {
                    permission: format!("ext.{key}.{command}"),
                    consent: Consent::Read,
                },
            );
        }
    }
    let permissions: BTreeSet<String> = serde_json::from_value(e["permissions"].clone())
        .map_err(|_| invalid("invalid permissions"))?;
    if permissions
        .iter()
        .any(|p| !policies.values().any(|policy| policy.permission == *p))
    {
        return Err(invalid("unsupported runtime permission"));
    }
    let doc = Document::parse(&release.document)?;
    doc.validate(&permissions, &policies)?;
    for action in &doc.actions {
        let valid = match action.command_id.as_str() {
            "host_context" => action.input.is_empty(),
            "host_notice" => {
                action.input.len() == 1
                    && action
                        .input
                        .get("text")
                        .is_some_and(|f| f.kind == FieldType::Text && f.required)
            }
            _ => false,
        };
        if !valid {
            return Err(invalid("host command input contract mismatch"));
        }
    }
    Ok(Ready {
        doc,
        permissions,
        policies,
        digest: release.metadata["release_digest"]
            .as_str()
            .ok_or_else(|| invalid("missing release digest"))?
            .to_owned(),
    })
}
fn check_enabled(state: &AppState) -> Result<(), ExtensionError> {
    if !state.config.marketplace_v2_runtime_enabled {
        return Err(ExtensionError::Domain(AppError::not_found(
            "declarative runtime unavailable",
        )));
    }
    Ok(())
}
async fn releases(state: &AppState) -> Result<Vec<VerifiedRelease>, ExtensionError> {
    load_verified(
        state.config.clone(),
        Profile {
            host: "web".into(),
            api: "1.0.0".into(),
        },
    )
    .await
}
fn check_human(actor: &super::Actor) -> Result<(), ExtensionError> {
    if !actor.is_human() {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "human project session required",
        )));
    }
    Ok(())
}
fn no_store(value: Value) -> Response {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(value)).into_response()
}
async fn document(
    State(state): State<AppState>,
    Path((org, project, manifest)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    check_enabled(&state)?;
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    check_human(&actor)?;
    let mut read = open_project_scope(
        &state,
        &actor,
        parse_uuid(&org)?,
        parse_uuid(&project)?,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let pin = active_pin(&mut read, parse_uuid(&manifest)?)
        .await
        .map_err(ExtensionError::Domain)?;
    let ready = qualify(&pin, &releases(&state).await?).map_err(ExtensionError::Domain)?;
    let result = json!({"document":ready.doc,"release_digest":ready.digest,"activation_revision":pin.revision,
        "manifest_id":pin.manifest_id,"profile":"web_read_only"});
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(result))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvokeBody {
    request_id: Uuid,
    release_digest: String,
    activation_revision: i64,
    action_id: String,
    input: Value,
}
fn run(
    read: &ProjectRead<'_>,
    ready: &Ready,
    body: &InvokeBody,
) -> Result<(String, Value), AppError> {
    let scope = TenantScope {
        organization_id: read.organization_id(),
        workspace_id: None,
        project_id: Some(read.project_id()),
        actor: ActorContext {
            actor_type: ActorType::Human,
            actor_id: read.actor_id(),
            credential_id: None,
            email: None,
        },
        permissions: vec![Permission::ProjectRead],
    };
    let grant = ActiveGrant {
        organization_id: read.organization_id(),
        project_id: read.project_id(),
        actor_id: read.actor_id(),
        release_digest: &ready.digest,
        active: true,
        permissions: &ready.permissions,
        release_permissions: &ready.permissions,
    };
    let request = Invocation {
        release_digest: &body.release_digest,
        action_id: &body.action_id,
        input: &body.input,
        human_confirmed: false,
    };
    let a = authorize(&ready.doc, &scope, &grant, &request, &ready.policies)?;
    let output = match a.command_id.as_str() {
        "host_context" => json!({"locale":"vi-VN","sdk_version":"2.0","profile":"web_read_only"}),
        "host_notice" => {
            let text = body.input["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.chars().count() <= 240)
                .ok_or_else(|| invalid("notice must contain 1–240 characters"))?;
            json!({"notice":text})
        }
        _ => return Err(invalid("command unavailable")),
    };
    Ok((a.command_id.clone(), output))
}
async fn invoke(
    State(state): State<AppState>,
    Path((org, project, manifest)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    check_enabled(&state)?;
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 256 * 1024)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let request = Request::from_parts(parts, axum::body::Body::from(bytes));
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    check_human(&actor)?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: InvokeBody = decode_body(body)?;
    if body.request_id.is_nil()
        || body.release_digest.len() != 64
        || body.action_id.len() > 64
        || body.activation_revision <= 0
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let _slot = INVOCATIONS
        .try_acquire()
        .map_err(|_| ExtensionError::Domain(AppError::rate_limited(Some(1))))?;
    let mut read = open_project_scope(
        &state,
        &actor,
        parse_uuid(&org)?,
        parse_uuid(&project)?,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let pin = active_pin(&mut read, parse_uuid(&manifest)?)
        .await
        .map_err(ExtensionError::Domain)?;
    if pin.revision != body.activation_revision {
        return Err(ExtensionError::Domain(AppError::conflict(
            "STALE_PLUGIN_ACTIVATION",
            "activation changed",
        )));
    }
    let ready = qualify(&pin, &releases(&state).await?).map_err(ExtensionError::Domain)?;
    let (command, output) = run(&read, &ready, &body).map_err(ExtensionError::Domain)?;
    let digest = hash(
        &marketplace_canonical(&json!({"action_id":body.action_id,"input":body.input}))
            .map_err(|_| ExtensionError::InvalidRequest)?,
    );
    let id = receipt(
        &mut read,
        &pin,
        ReceiptInput {
            request_id: body.request_id,
            release_digest: &ready.digest,
            request_digest: &digest,
            command_id: &command,
        },
    )
    .await
    .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(
        json!({"receipt_id":id,"command_id":command,"output":output}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(cap: &str, command: &str, consent: &str) -> (ActivePin, Vec<VerifiedRelease>) {
        let manifest =
            json!({"extension_key":"partner_pub_demo","version":"1.0.0","capabilities":[cap]});
        let md = hash(&marketplace_canonical(&manifest).unwrap());
        let doc = json!({"slot":"workspace","widgets":[{"kind":"button","id":"b","label":"Run","action":"run"}],
            "actions":[{"id":"run","command_id":command,"permission":format!("ext.partner_pub_demo.{cap}"),"consent":consent,"timeout_ms":1000,"input":{}}]});
        (
            ActivePin {
                activation_id: Uuid::new_v4(),
                manifest_id: Uuid::new_v4(),
                revision: 1,
                manifest,
            },
            vec![VerifiedRelease {
                metadata: json!({"release_digest":"a".repeat(64),"envelope":{"manifest_digest":md,"extension_key":"partner_pub_demo","version":"1.0.0","assets":[],"permissions":[format!("ext.partner_pub_demo.{cap}")]}}),
                document: serde_json::to_vec(&doc).unwrap(),
            }],
        )
    }
    #[test]
    fn runtime_policy_is_fail_closed() {
        let (p, r) = fixture("host_context", "host_context", "read");
        assert!(qualify(&p, &r).is_ok());
        let (p, r) = fixture("host_context", "host_context", "mutation");
        assert!(qualify(&p, &r).is_err());
        let (p, r) = fixture("map_view", "map_view", "read");
        assert!(qualify(&p, &r).is_err());
        let (p, r) = fixture("host_notice", "host_notice", "read");
        assert!(qualify(&p, &r).is_err());
        let (mut p, r) = fixture("host_context", "host_context", "read");
        p.manifest["version"] = json!("2.0.0");
        assert!(qualify(&p, &r).is_err());
    }
}
