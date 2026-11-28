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
    extract::{Path, Query, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use ovg_core::{ActorContext, ActorType, AppError, Permission, TenantScope};
use ovg_module_sdk::declarative::{authorize, ActiveGrant, CommandPolicy, Document, Invocation};
use ovg_storage::{
    catalog::{ProjectRead, ProjectReadPolicy},
    extensions::runtime_v2::{
        active_pin, dependency_candidates, grant_status, minimum_approved_epoch, receipt,
        set_grant, ActivePin, DependencyApproval, ReceiptInput,
    },
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
        .route(
            "/organizations/{organization_id}/projects/{project_id}/extensions/v2-audit",
            get(audit_receipts),
        )
        .route(&format!("{BASE}/invoke"), post(invoke))
        .route(&format!("{BASE}/grant"), post(change_grant))
        .route(
            &format!("{BASE}/processing/confirm"),
            post(confirm_processing),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQuery {
    after_id: Option<Uuid>,
    limit: Option<u32>,
}
async fn audit_receipts(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    Query(q): Query<AuditQuery>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let limit = q.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) || q.after_id.is_some_and(|id| id.is_nil()) {
        return Err(ExtensionError::InvalidRequest);
    }
    let (mut parts, _) = req.into_parts();
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
    let result =
        ovg_storage::extensions::runtime_v2::list_own_receipts(&mut read, q.after_id, limit)
            .await
            .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(result))
}
struct Ready {
    doc: Document,
    permissions: BTreeSet<String>,
    policies: BTreeMap<String, CommandPolicy>,
    digest: String,
}
/// Only reviewed host capabilities can be translated. GIS reads are restricted to metadata.
/// No material mutations are permitted.
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
    let caps: BTreeSet<String> = serde_json::from_value(pin.manifest["capabilities"].clone())
        .map_err(|_| invalid("invalid capabilities"))?;
    let policies = ovg_module_sdk::host_profile::read_only_policies(key, &caps);
    let permissions: BTreeSet<String> = serde_json::from_value(e["permissions"].clone())
        .map_err(|_| invalid("invalid permissions"))?;
    let doc = Document::parse(&release.document)?;
    ovg_module_sdk::host_profile::validate_read_only(&doc, &permissions, &policies)?;
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
struct ApprovedLock {
    pins: Value,
    digest: String,
    epoch: i64,
}
/// Resolve only signed candidates already active in the same project. The lock
/// includes activation revisions and exact manifest fingerprints, not merely semver.
async fn dependency_lock(
    read: &mut ProjectRead<'_>,
    root: &ActivePin,
    ready: &Ready,
    releases: &[VerifiedRelease],
) -> Result<ApprovedLock, AppError> {
    use ovg_module_sdk::release_v2::{canonical, resolve_lock, Pin};
    let mut catalog = Vec::new();
    for r in releases {
        let e = &r.metadata["envelope"];
        let get = |k: &str| {
            e[k].as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("invalid signed dependency identity"))
        };
        catalog.push(Pin {
            key: get("extension_key")?,
            version: get("version")?,
            digest: r.metadata["release_digest"]
                .as_str()
                .ok_or_else(|| invalid("missing digest"))?
                .to_owned(),
            dependencies: if e.get("dependencies").is_some() {
                serde_json::from_value(e["dependencies"].clone())
                    .map_err(|_| invalid("invalid signed dependency"))?
            } else {
                vec![]
            },
        });
    }
    let root_key = root.manifest["extension_key"]
        .as_str()
        .ok_or_else(|| invalid("missing root key"))?;
    let root_version = root.manifest["version"]
        .as_str()
        .ok_or_else(|| invalid("missing root version"))?;
    let keys: Vec<String> = catalog
        .iter()
        .map(|p| p.key.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let candidates = dependency_candidates(read, &keys).await?;
    let mut active = BTreeMap::new();
    for (id, key, version) in candidates {
        if active.insert((key, version), id).is_some() {
            return Err(invalid("duplicate active dependency"));
        }
    }
    catalog.retain(|p| active.contains_key(&(p.key.clone(), p.version.clone())));
    let selected = resolve_lock(root_key, root_version, &catalog)?;
    let mut pins = Vec::new();
    let mut epoch = None;
    for p in selected {
        let r = releases
            .iter()
            .find(|r| r.metadata["release_digest"].as_str() == Some(p.digest.as_str()))
            .ok_or_else(|| invalid("dependency missing"))?;
        let e = r.metadata["catalog_epoch"]
            .as_i64()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("invalid epoch"))?;
        if epoch.is_some_and(|v| v != e) {
            return Err(invalid("inconsistent catalog epoch"));
        }
        epoch = Some(e);
        let id = active[&(p.key.clone(), p.version.clone())];
        let pin = active_pin(read, id).await?;
        let qualified = qualify(&pin, releases)?;
        if qualified.digest != p.digest
            || (p.key == root_key && (id != root.manifest_id || p.digest != ready.digest))
        {
            return Err(invalid("active dependency does not match signed release"));
        }
        pins.push(json!({"key":p.key,"version":p.version,"release_digest":p.digest,"manifest_id":pin.manifest_id,
            "activation_id":pin.activation_id,"activation_revision":pin.revision}));
    }
    let pins = json!(pins);
    let digest = hash(&canonical(&pins)?);
    let epoch = epoch.ok_or_else(|| invalid("missing root epoch"))?;
    if epoch < minimum_approved_epoch(read).await? {
        return Err(AppError::conflict(
            "STALE_PLUGIN_CATALOG",
            "catalog epoch rollback",
        ));
    }
    Ok(ApprovedLock {
        pins,
        digest,
        epoch,
    })
}
fn lock_matches(
    grant: &ovg_storage::extensions::runtime_v2::GrantState,
    lock: &ApprovedLock,
) -> bool {
    grant.granted
        && grant.lock_digest.as_deref() == Some(lock.digest.as_str())
        && grant.catalog_epoch.is_some_and(|e| e <= lock.epoch)
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
        state,
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
    let catalog = releases(&state).await?;
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
    let ready = qualify(&pin, &catalog).map_err(ExtensionError::Domain)?;
    let lock = dependency_lock(&mut read, &pin, &ready, &catalog)
        .await
        .map_err(ExtensionError::Domain)?;
    super::entitlement::require_graph_license(&state, &mut read, &lock.pins, &catalog)
        .await
        .map_err(ExtensionError::Domain)?;
    let grant = grant_status(&mut read, &pin, &ready.digest)
        .await
        .map_err(ExtensionError::Domain)?;
    let result = json!({"granted":lock_matches(&grant,&lock),"dependency_lock":lock.pins,"lock_digest":lock.digest,"catalog_epoch":lock.epoch,"grant_revision":grant.sequence.to_string(),"permissions":ready.permissions,"document":ready.doc,"release_digest":ready.digest,"activation_revision":pin.revision,
        "manifest_id":pin.manifest_id,"profile":"web_read_only"});
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(result))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantBody {
    release_digest: String,
    activation_revision: i64,
    enabled: bool,
    grant_revision: String,
    lock_digest: String,
}
async fn change_grant(
    State(state): State<AppState>,
    Path((org, project, manifest)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    check_enabled(&state)?;
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 4096)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let (actor, headers, body) = authenticate_json(
        &state,
        Request::from_parts(parts, axum::body::Body::from(bytes)),
        &org,
    )
    .await?;
    check_human(&actor)?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: GrantBody = decode_body(body)?;
    let catalog = if body.enabled {
        Some(releases(&state).await?)
    } else {
        None
    };
    let mut read = open_project_scope(
        &state,
        &actor,
        parse_uuid(&org)?,
        parse_uuid(&project)?,
        ProjectReadPolicy::UnrestrictedManage,
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
    // Approval requires the exact lock previewed by the manager. Revocation
    // still works when a dependency or the root has been removed from the catalog.
    let lock = if let Some(catalog) = catalog {
        let ready = qualify(&pin, &catalog).map_err(ExtensionError::Domain)?;
        if ready.digest != body.release_digest {
            return Err(ExtensionError::Domain(AppError::conflict(
                "STALE_PLUGIN_RELEASE",
                "release changed",
            )));
        }
        let lock = dependency_lock(&mut read, &pin, &ready, &catalog)
            .await
            .map_err(ExtensionError::Domain)?;
        super::entitlement::require_graph_license(&state, &mut read, &lock.pins, &catalog)
            .await
            .map_err(ExtensionError::Domain)?;
        if lock.digest != body.lock_digest {
            return Err(ExtensionError::Domain(AppError::conflict(
                "STALE_PLUGIN_LOCK",
                "dependency lock changed; reload",
            )));
        }
        Some(lock)
    } else {
        None
    };
    let expected = body
        .grant_revision
        .parse::<i64>()
        .ok()
        .filter(|v| *v >= 0)
        .ok_or(ExtensionError::InvalidRequest)?;
    let grant = set_grant(
        &mut read,
        &pin,
        &body.release_digest,
        body.enabled,
        expected,
        lock.as_ref().map(|l| DependencyApproval {
            pins: &l.pins,
            digest: &l.digest,
            epoch: l.epoch,
        }),
    )
    .await
    .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(
        json!({"granted":grant.granted,"grant_revision":grant.sequence.to_string()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvokeBody {
    request_id: Uuid,
    release_digest: String,
    activation_revision: i64,
    action_id: String,
    input: Value,
    grant_revision: String,
    lock_digest: String,
    catalog_epoch: i64,
}
/// Bind replay identity to the authorization snapshot, not just raw input.
fn invocation_digest(
    body: &InvokeBody,
    lock: &ApprovedLock,
    grant_sequence: i64,
) -> Result<String, AppError> {
    let payload = json!({"schema":"openvietgis.invocation.v2","action_id":body.action_id,"input":body.input,
        "dependency_lock_digest":lock.digest,"grant_revision":grant_sequence,"catalog_epoch":lock.epoch});
    Ok(hash(
        &marketplace_canonical(&payload).map_err(|_| invalid("invalid invocation fingerprint"))?,
    ))
}
/// Allowlisted metadata only; never return source URLs, features, coordinates, attributes or credentials.
fn layer_summary(layer: &Value) -> Value {
    json!({"id":layer["id"],"name":layer["name"],"lifecycle":layer["lifecycle"],"layer_revision":layer["layer_revision"],
        "geometry_family":layer["source"]["schema"]["geometry_family"],"feature_count":layer["source"]["feature_count"]})
}
fn authorize_invocation<'a>(
    read: &ProjectRead<'_>,
    ready: &'a Ready,
    body: &InvokeBody,
) -> Result<&'a ovg_module_sdk::declarative::Action, AppError> {
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
    authorize(&ready.doc, &scope, &grant, &request, &ready.policies)
}
async fn run(
    read: &mut ProjectRead<'_>,
    command: &str,
    body: &InvokeBody,
) -> Result<Value, AppError> {
    let output = match command {
        "host_context" => json!({"locale":"vi-VN","sdk_version":"2.0","profile":"web_read_only"}),
        "gis_layers" => {
            let cursor = body
                .input
                .get("after_id")
                .map(|v| {
                    Uuid::parse_str(v.as_str().unwrap_or_default())
                        .map_err(|_| invalid("invalid layer cursor"))
                })
                .transpose()?;
            let result = read.list_layers(cursor, 50).await?;
            let rows = result["items"]
                .as_array()
                .ok_or_else(|| invalid("invalid layer catalog"))?;
            json!({"items":rows.iter().map(layer_summary).collect::<Vec<_>>(),"next_after_id":result["next_after_id"],"limit":50})
        }
        "gis_layer" => {
            let id = Uuid::parse_str(
                body.input["layer_id"]
                    .as_str()
                    .ok_or_else(|| invalid("layer id required"))?,
            )
            .map_err(|_| invalid("invalid layer id"))?;
            layer_summary(&read.get_layer(id).await?)
        }
        "gis_processing_plan" => {
            super::processing_plan::preview(
                read,
                body.input["request"]
                    .as_str()
                    .ok_or_else(|| invalid("processing request required"))?,
            )
            .await?
        }
        "host_notice" => {
            let text = body.input["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.chars().count() <= 240)
                .ok_or_else(|| invalid("notice must contain 1–240 characters"))?;
            json!({"notice":text})
        }
        _ => return Err(invalid("command unavailable")),
    };
    Ok(output)
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
    let catalog = releases(&state).await?;
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
    let ready = qualify(&pin, &catalog).map_err(ExtensionError::Domain)?;
    let lock = dependency_lock(&mut read, &pin, &ready, &catalog)
        .await
        .map_err(ExtensionError::Domain)?;
    super::entitlement::require_graph_license(&state, &mut read, &lock.pins, &catalog)
        .await
        .map_err(ExtensionError::Domain)?;
    let grant = grant_status(&mut read, &pin, &ready.digest)
        .await
        .map_err(ExtensionError::Domain)?;
    if !lock_matches(&grant, &lock) {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "current dependency lock approval required",
        )));
    }
    if body.grant_revision.parse::<i64>().ok() != Some(grant.sequence)
        || body.lock_digest != lock.digest
        || body.catalog_epoch != lock.epoch
    {
        return Err(ExtensionError::Domain(AppError::conflict(
            "STALE_PLUGIN_AUTHORIZATION",
            "plugin approval or catalog changed; reload",
        )));
    }
    let action = authorize_invocation(&read, &ready, &body).map_err(ExtensionError::Domain)?;
    let command = action.command_id.clone();
    let deadline = std::time::Duration::from_millis(u64::from(action.timeout_ms));
    let digest = invocation_digest(&body, &lock, grant.sequence).map_err(ExtensionError::Domain)?;
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
    // Reserve the successful-invocation receipt before touching GIS data.
    // The same transaction rolls back this receipt on failure or timeout.
    let mut output = tokio::time::timeout(deadline, run(&mut read, &command, &body))
        .await
        .map_err(|_| {
            ExtensionError::Domain(AppError::service_unavailable(
                "PLUGIN_COMMAND_TIMEOUT",
                "plugin command timed out",
            ))
        })?
        .map_err(ExtensionError::Domain)?;
    if command == "gis_processing_plan" {
        let digest =
            hash(&ovg_module_sdk::release_v2::canonical(&output).map_err(ExtensionError::Domain)?);
        output = ovg_storage::extensions::processing::propose(
            &mut read,
            &pin,
            id,
            ovg_storage::extensions::processing::ProposalBinding {
                release: &ready.digest,
                lock: &lock.digest,
                grant_revision: grant.sequence,
                epoch: lock.epoch,
            },
            &output,
            &digest,
        )
        .await
        .map_err(ExtensionError::Domain)?;
    }
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(
        json!({"receipt_id":id,"command_id":command,"output":output}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmProcessing {
    proposal_id: Uuid,
    plan_digest: String,
}
/// Dedicated trusted-host confirmation. Not a command callable from the plugin
/// document; requires current manager session, CSRF, same actor and exact proposal.
async fn confirm_processing(
    State(state): State<AppState>,
    Path((org, project, manifest)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    use ovg_storage::extensions::processing::{self, ProposalBinding};
    check_enabled(&state)?;
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 4096)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let (actor, headers, body) = authenticate_json(
        &state,
        Request::from_parts(parts, axum::body::Body::from(bytes)),
        &org,
    )
    .await?;
    check_human(&actor)?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: ConfirmProcessing = decode_body(body)?;
    let _slot = INVOCATIONS
        .try_acquire()
        .map_err(|_| ExtensionError::Domain(AppError::rate_limited(Some(1))))?;
    let catalog = releases(&state).await?;
    let mut read = open_project_scope(
        &state,
        &actor,
        parse_uuid(&org)?,
        parse_uuid(&project)?,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let pin = active_pin(&mut read, parse_uuid(&manifest)?)
        .await
        .map_err(ExtensionError::Domain)?;
    let ready = qualify(&pin, &catalog).map_err(ExtensionError::Domain)?;
    let lock = dependency_lock(&mut read, &pin, &ready, &catalog)
        .await
        .map_err(ExtensionError::Domain)?;
    super::entitlement::require_graph_license(&state, &mut read, &lock.pins, &catalog)
        .await
        .map_err(ExtensionError::Domain)?;
    let grant = grant_status(&mut read, &pin, &ready.digest)
        .await
        .map_err(ExtensionError::Domain)?;
    if !lock_matches(&grant, &lock)
        || !ready.doc.actions.iter().any(|a| {
            a.command_id == "gis_processing_plan" && ready.permissions.contains(&a.permission)
        })
    {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "processing plan permission required",
        )));
    }
    let plan = processing::proposal(
        &mut read,
        body.proposal_id,
        &pin,
        ProposalBinding {
            release: &ready.digest,
            lock: &lock.digest,
            grant_revision: grant.sequence,
            epoch: lock.epoch,
        },
        &body.plan_digest,
    )
    .await
    .map_err(ExtensionError::Domain)?;
    if hash(&ovg_module_sdk::release_v2::canonical(&plan).map_err(ExtensionError::Domain)?)
        != body.plan_digest
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let job = if let Some(id) = processing::approved_job(&mut read, body.proposal_id)
        .await
        .map_err(ExtensionError::Domain)?
    {
        ovg_storage::processing_jobs::get_job(&mut read, id)
            .await
            .map_err(ExtensionError::Domain)?
    } else {
        let job = super::processing_plan::queue(&mut read, body.proposal_id, &plan)
            .await
            .map_err(ExtensionError::Domain)?;
        let id = job["id"]
            .as_str()
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or(ExtensionError::InvalidRequest)?;
        processing::approve(&mut read, body.proposal_id, id)
            .await
            .map_err(ExtensionError::Domain)?;
        job
    };
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(no_store(
        json!({"proposal_id":body.proposal_id,"job":job,"execution":"manual_core_job"}),
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
                manifest: manifest.clone(),
            },
            vec![VerifiedRelease {
                metadata: json!({"release_digest":"a".repeat(64),"envelope":{"manifest_digest":md,"extension_key":"partner_pub_demo","version":"1.0.0","assets":[],"permissions":[format!("ext.partner_pub_demo.{cap}")]}}),
                document: serde_json::to_vec(&doc).unwrap(),
                manifest: manifest.clone(),
            }],
        )
    }
    #[test]
    fn shared_host_wire_vectors_match_broker_contract() {
        let f: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/spec075/runtime-wire-vectors.json"
        ))
        .unwrap();
        for c in f["valid"].as_array().unwrap() {
            let runtime = &c["runtime"];
            let request: InvokeBody = serde_json::from_value(c["expected"].clone()).unwrap();
            assert_eq!(
                request.release_digest,
                runtime["release_digest"].as_str().unwrap()
            );
            assert_eq!(
                request.activation_revision,
                runtime["activation_revision"].as_i64().unwrap()
            );
            assert_eq!(
                request.grant_revision,
                runtime["grant_revision"].as_str().unwrap()
            );
            assert_eq!(
                request.lock_digest,
                runtime["lock_digest"].as_str().unwrap()
            );
            assert_eq!(
                request.catalog_epoch,
                runtime["catalog_epoch"].as_i64().unwrap()
            );
            let doc = Document::parse(&serde_json::to_vec(&runtime["document"]).unwrap()).unwrap();
            let permissions: std::collections::BTreeSet<String> =
                serde_json::from_value(runtime["permissions"].clone()).unwrap();
            let key = runtime["dependency_lock"][0]["key"].as_str().unwrap();
            let caps = permissions
                .iter()
                .map(|p| p.rsplit('.').next().unwrap().to_owned())
                .collect();
            let policies = ovg_module_sdk::host_profile::read_only_policies(key, &caps);
            ovg_module_sdk::host_profile::validate_read_only(&doc, &permissions, &policies)
                .unwrap();
            let action = doc
                .actions
                .iter()
                .find(|a| a.id == request.action_id)
                .unwrap();
            for (k, v) in request.input.as_object().unwrap() {
                assert!(action.input.contains_key(k) && v.is_string());
            }
            for (k, v) in &action.input {
                if v.required {
                    assert!(request.input.get(k).is_some());
                }
            }
            let mut missing = c["expected"].clone();
            missing.as_object_mut().unwrap().remove("grant_revision");
            assert!(serde_json::from_value::<InvokeBody>(missing).is_err());
        }
    }
    #[test]
    fn invocation_replay_is_bound_to_grant_and_lock() {
        let mut body = InvokeBody {
            request_id: Uuid::new_v4(),
            release_digest: "a".repeat(64),
            activation_revision: 1,
            action_id: "read".into(),
            input: json!({"x":"y"}),
            grant_revision: "1".into(),
            lock_digest: "b".repeat(64),
            catalog_epoch: 2,
        };
        let mut lock = ApprovedLock {
            pins: json!([]),
            digest: "b".repeat(64),
            epoch: 2,
        };
        let initial = invocation_digest(&body, &lock, 1).unwrap();
        assert_eq!(initial, invocation_digest(&body, &lock, 1).unwrap());
        assert_ne!(initial, invocation_digest(&body, &lock, 2).unwrap());
        lock.epoch = 3;
        assert_ne!(initial, invocation_digest(&body, &lock, 1).unwrap());
        lock.epoch = 2;
        lock.digest = "c".repeat(64);
        assert_ne!(initial, invocation_digest(&body, &lock, 1).unwrap());
        lock.digest = "b".repeat(64);
        body.input = json!({"x":"z"});
        assert_ne!(initial, invocation_digest(&body, &lock, 1).unwrap());
    }
    #[test]
    fn lock_approval_rejects_changed_dependency_and_epoch_rollback() {
        use ovg_storage::extensions::runtime_v2::GrantState;
        let lock = ApprovedLock {
            pins: json!([]),
            digest: "a".repeat(64),
            epoch: 4,
        };
        let mut grant = GrantState {
            granted: true,
            sequence: 1,
            lock_digest: Some(lock.digest.clone()),
            catalog_epoch: Some(4),
        };
        assert!(lock_matches(&grant, &lock));
        grant.catalog_epoch = Some(5);
        assert!(!lock_matches(&grant, &lock));
        grant.catalog_epoch = Some(3);
        assert!(lock_matches(&grant, &lock));
        grant.lock_digest = Some("b".repeat(64));
        assert!(!lock_matches(&grant, &lock));
        grant.lock_digest = None;
        assert!(!lock_matches(&grant, &lock));
    }
    #[test]
    fn layer_summary_excludes_sensitive_source_fields() {
        let row = json!({"id":"layer","name":"A","lifecycle":"active","layer_revision":2,
            "source":{"schema":{"geometry_family":"Polygon"},"feature_count":3,"url":"private","extent":[1,2,3,4],"fields":[{"name":"private"}]}});
        let result = layer_summary(&row);
        assert_eq!(
            result,
            json!({"id":"layer","name":"A","lifecycle":"active","layer_revision":2,"geometry_family":"Polygon","feature_count":3})
        );
    }
    #[test]
    fn runtime_policy_is_fail_closed() {
        let (p, r) = fixture("host_context", "host_context", "read");
        assert!(qualify(&p, &r).is_ok());
        let (p, r) = fixture("host_context", "host_context", "mutation");
        assert!(qualify(&p, &r).is_err());
        let (p, r) = fixture("map_view", "gis_layers", "read");
        assert!(qualify(&p, &r).is_ok());
        let (p, r) = fixture("host_context", "gis_layers", "read");
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
