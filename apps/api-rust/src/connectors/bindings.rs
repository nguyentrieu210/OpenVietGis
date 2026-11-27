//! E12 integration-hub export-scopes, QGIS connector bindings, pairing tokens,
//! sync conflicts ledger, and real-time spatial_change SSE stream.

use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::collaboration::ChangeListener;
use ovg_storage::editing::{EditError, Guard, Permission};
use ovg_storage::integration_hub::{
    self, is_valid_binding_field_id, is_valid_dataset_ref, ConflictItemInput, CreateBindingInput,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::Instant;
use uuid::Uuid;

use crate::edit_auth::{
    self, authenticate_actor, authenticate_json, require_csrf, Actor, Failure,
};
use crate::AppState;

const HUB_BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integration-hub";
const BIND_BASE: &str =
    "/organizations/{organization_id}/projects/{project_id}/integration-hub/bindings";

static INTEGRATION_LISTENERS: Semaphore = Semaphore::const_new(40);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(15);
const READY: &str = "retry: 3000\nevent: ready\ndata: {}\n\n";
const HEARTBEAT: &str = ": heartbeat\n\n";
const SPATIAL_CHANGE: &str = "event: spatial_change\ndata: {}\n\n";
const UNAVAILABLE: &str = "event: unavailable\ndata: {}\n\n";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{HUB_BASE}/export-scopes"), get(get_export_scopes))
        .route(&format!("{HUB_BASE}/events"), get(stream_integration_events))
        .route(&format!("{BIND_BASE}/candidates"), get(list_candidates))
        .route(BIND_BASE, get(list_bindings).post(create_binding))
        .route(&format!("{BIND_BASE}/{{binding_id}}/activate"), post(activate_binding))
        .route(
            &format!("{BIND_BASE}/{{binding_id}}/lifecycle"),
            post(change_binding_lifecycle),
        )
        .route(
            &format!("{BIND_BASE}/{{binding_id}}/pairing-tokens"),
            get(list_pairing_tokens).post(issue_pairing_token),
        )
        .route(
            &format!("{BIND_BASE}/{{binding_id}}/pairing-tokens/{{token_id}}/revoke"),
            post(revoke_pairing_token),
        )
        .route(
            &format!("{BIND_BASE}/{{binding_id}}/conflicts"),
            get(list_binding_conflicts).post(record_binding_conflicts),
        )
        .route(
            &format!("{BIND_BASE}/{{binding_id}}/conflicts/{{conflict_id}}/resolve"),
            post(resolve_binding_conflict),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBindingBody {
    layer_id: Uuid,
    #[serde(default = "default_qgis")]
    adapter_key: String,
    external_dataset_ref: String,
    #[serde(default = "default_direction")]
    direction: String,
    source_crs_code: i32,
    target_crs_code: i32,
    #[serde(default = "default_pk")]
    external_primary_key: String,
    allowed_field_ids: Vec<String>,
}

fn default_qgis() -> String {
    "qgis".to_owned()
}

fn default_direction() -> String {
    "bidirectional_manual".to_owned()
}

fn default_pk() -> String {
    "ovg_feature_id".to_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivateBindingBody {
    expected_revision: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeLifecycleBody {
    action: String,
    expected_revision: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IssuePairingTokenBody {
    #[serde(default = "default_label")]
    label: String,
    #[serde(default = "default_expiry_days")]
    expires_in_days: i64,
}

fn default_label() -> String {
    "QGIS Desktop".to_owned()
}

fn default_expiry_days() -> i64 {
    30
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordConflictItemBody {
    feature_id: String,
    #[serde(default = "default_conflict_kind")]
    conflict_kind: String,
    #[serde(default)]
    field_name: Option<String>,
    #[serde(default)]
    baseline_value: Value,
    #[serde(default)]
    web_value: Value,
    #[serde(default)]
    qgis_value: Value,
}

fn default_conflict_kind() -> String {
    "attribute".to_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordConflictsBody {
    items: Vec<RecordConflictItemBody>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveConflictBody {
    decision: String,
    #[serde(default)]
    merged_value: Option<Value>,
    #[serde(default)]
    note: Option<String>,
}

fn no_store_json(status: StatusCode, body: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

fn require_human(actor: &Actor) -> Result<(), Failure> {
    if !matches!(actor, Actor::Human(_)) {
        return Err(EditError::detail(403, "Project manager session required.").into());
    }
    Ok(())
}

async fn get_export_scopes(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::ReadScopedAllowed).await?;
    scope.authorize(Permission::Read).await?;
    let body = integration_hub::export_scopes(&mut scope).await?;
    scope.commit().await?;
    Ok(no_store_json(StatusCode::OK, body))
}

async fn list_candidates(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Read).await?;
    scope.authorize(Permission::Manage).await?;
    let body = integration_hub::list_binding_candidates(&mut scope).await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn list_bindings(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Read).await?;
    scope.authorize(Permission::Manage).await?;
    let body = integration_hub::list_bindings(&mut scope).await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn create_binding(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, raw) = authenticate_json(&state, request, &org).await?;
    let parsed: CreateBindingBody = edit_auth::decode(raw)?;
    let dataset = parsed.external_dataset_ref.trim().to_owned();
    if parsed.adapter_key != "qgis"
        || parsed.direction != "bidirectional_manual"
        || parsed.external_primary_key != "ovg_feature_id"
        || !is_valid_dataset_ref(&dataset)
        || !(1..=999_999).contains(&parsed.source_crs_code)
        || !(1..=999_999).contains(&parsed.target_crs_code)
        || parsed.allowed_field_ids.is_empty()
        || parsed.allowed_field_ids.len() > 100
    {
        return Err(EditError::InvalidRequest.into());
    }
    let mut uniq = HashSet::with_capacity(parsed.allowed_field_ids.len());
    for fid in &parsed.allowed_field_ids {
        if !uniq.insert(fid.clone()) || !is_valid_binding_field_id(fid) {
            return Err(EditError::InvalidRequest.into());
        }
    }
    let mut allowed: Vec<String> = uniq.into_iter().collect();
    allowed.sort();

    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Manage).await?;
    require_csrf(&state, &actor, &headers)?;

    let body = integration_hub::create_binding(
        &mut scope,
        CreateBindingInput {
            layer_id: parsed.layer_id,
            external_dataset_ref: dataset,
            direction: parsed.direction,
            source_crs_code: parsed.source_crs_code,
            target_crs_code: parsed.target_crs_code,
            external_primary_key: parsed.external_primary_key,
            allowed_field_ids: allowed,
        },
    )
    .await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

async fn activate_binding(
    State(state): State<AppState>,
    Path((org, proj, binding_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, raw) = authenticate_json(&state, request, &org).await?;
    let parsed: ActivateBindingBody = edit_auth::decode(raw)?;
    if parsed.expected_revision < 1 {
        return Err(EditError::InvalidRequest.into());
    }
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Manage).await?;
    require_csrf(&state, &actor, &headers)?;
    let body =
        integration_hub::activate_binding(&mut scope, binding_id, parsed.expected_revision).await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn change_binding_lifecycle(
    State(state): State<AppState>,
    Path((org, proj, binding_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, raw) = authenticate_json(&state, request, &org).await?;
    let parsed: ChangeLifecycleBody = edit_auth::decode(raw)?;
    if !matches!(parsed.action.as_str(), "pause" | "revoke") || parsed.expected_revision < 1 {
        return Err(EditError::InvalidRequest.into());
    }
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Manage).await?;
    require_csrf(&state, &actor, &headers)?;
    let body = integration_hub::change_binding_lifecycle(
        &mut scope,
        binding_id,
        &parsed.action,
        parsed.expected_revision,
    )
    .await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn list_pairing_tokens(
    State(state): State<AppState>,
    Path((org, proj, binding_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Read).await?;
    scope.authorize(Permission::Manage).await?;
    let body = integration_hub::list_pairing_tokens(&mut scope, binding_id).await?;
    scope.commit().await?;
    Ok(no_store_json(StatusCode::OK, body))
}

fn resolve_server_url(headers: &HeaderMap, fallback: &str) -> String {
    if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
        let host = host.trim();
        if !host.is_empty() {
            let proto = headers
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or("http");
            return format!("{proto}://{host}");
        }
    }
    fallback.trim_end_matches('/').to_owned()
}

async fn issue_pairing_token(
    State(state): State<AppState>,
    Path((org, proj, binding_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, raw) = authenticate_json(&state, request, &org).await?;
    let parsed: IssuePairingTokenBody = edit_auth::decode(raw)?;
    let label = parsed.label.trim().to_owned();
    let label_len = label.chars().count();
    if !(1..=120).contains(&label_len) || !(1..=365).contains(&parsed.expires_in_days) {
        return Err(EditError::InvalidRequest.into());
    }
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Manage).await?;
    require_csrf(&state, &actor, &headers)?;
    let server_url = resolve_server_url(&headers, &state.config.api_public_base_url);
    let body = integration_hub::issue_pairing_token(
        &mut scope,
        binding_id,
        &label,
        parsed.expires_in_days,
        &server_url,
    )
    .await?;
    scope.commit().await?;
    Ok(no_store_json(StatusCode::CREATED, body))
}

async fn revoke_pairing_token(
    State(state): State<AppState>,
    Path((org, proj, binding_id, token_id)): Path<(String, String, Uuid, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, _) = authenticate_json(&state, request, &org).await?;
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Manage).await?;
    require_csrf(&state, &actor, &headers)?;
    let body = integration_hub::revoke_pairing_token(&mut scope, binding_id, token_id).await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn list_binding_conflicts(
    State(state): State<AppState>,
    Path((org, proj, binding_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Read).await?;
    scope.authorize(Permission::Manage).await?;
    let body = integration_hub::list_binding_conflicts(&mut scope, binding_id).await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn record_binding_conflicts(
    State(state): State<AppState>,
    Path((org, proj, binding_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, _headers, raw) = authenticate_json(&state, request, &org).await?;
    let parsed: RecordConflictsBody = edit_auth::decode(raw)?;
    if parsed.items.is_empty() || parsed.items.len() > 100 {
        return Err(EditError::InvalidRequest.into());
    }
    let mut items = Vec::with_capacity(parsed.items.len());
    for item in parsed.items {
        let fid = item.feature_id.trim().to_owned();
        if !(1..=128).contains(&fid.chars().count())
            || !matches!(
                item.conflict_kind.as_str(),
                "attribute" | "geometry" | "delete_candidate"
            )
            || item
                .field_name
                .as_ref()
                .is_some_and(|f| f.chars().count() > 128)
        {
            return Err(EditError::InvalidRequest.into());
        }
        items.push(ConflictItemInput {
            feature_id: fid,
            conflict_kind: item.conflict_kind,
            field_name: item.field_name,
            baseline_value: item.baseline_value,
            web_value: item.web_value,
            qgis_value: item.qgis_value,
        });
    }

    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Edit).await?;
    let body = integration_hub::record_binding_conflicts(&mut scope, binding_id, items).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

async fn resolve_binding_conflict(
    State(state): State<AppState>,
    Path((org, proj, binding_id, conflict_id)): Path<(String, String, Uuid, Uuid)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, raw) = authenticate_json(&state, request, &org).await?;
    let parsed: ResolveConflictBody = edit_auth::decode(raw)?;
    if !matches!(
        parsed.decision.as_str(),
        "keep_web" | "keep_qgis" | "merge" | "dismiss"
    ) || parsed.note.as_ref().is_some_and(|n| n.chars().count() > 500)
    {
        return Err(EditError::InvalidRequest.into());
    }
    require_human(&actor)?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Write).await?;
    scope.authorize(Permission::Manage).await?;
    require_csrf(&state, &actor, &headers)?;
    let body = integration_hub::resolve_binding_conflict(
        &mut scope,
        binding_id,
        conflict_id,
        &parsed.decision,
        parsed.merged_value,
        parsed.note.as_deref(),
    )
    .await?;
    scope.commit().await?;
    Ok(Json(body).into_response())
}

async fn stream_integration_events(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let mut scope = edit_auth::open(&state, &actor, &org, &proj, Guard::Read).await?;
    let org_id = scope.organization_id();
    let proj_id = scope.project_id();
    scope.authorize(Permission::Read).await?;
    scope.commit().await?;

    let (sender, receiver) = mpsc::channel::<String>(4);
    tokio::spawn(pump_integration_events(state, sender, org_id, proj_id));
    let frames = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let frame = receiver.recv().await?;
        Some((Ok::<_, Infallible>(frame), receiver))
    });
    let headers = [
        (
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        ),
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-transform"),
        ),
        (
            header::HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        ),
        (header::CONNECTION, HeaderValue::from_static("keep-alive")),
    ];
    Ok((headers, Body::from_stream(frames)).into_response())
}

async fn pump_integration_events(
    state: AppState,
    sender: mpsc::Sender<String>,
    org_id: Uuid,
    proj_id: Uuid,
) {
    let terminal = match INTEGRATION_LISTENERS.try_acquire() {
        Ok(_slot) => listen_spatial_changes(&state, &sender, org_id, proj_id).await,
        Err(_) => Some(UNAVAILABLE),
    };
    if let Some(frame) = terminal {
        let _ = sender.send(frame.to_owned()).await;
    }
}

async fn listen_spatial_changes(
    state: &AppState,
    sender: &mpsc::Sender<String>,
    org_id: Uuid,
    proj_id: Uuid,
) -> Option<&'static str> {
    let pool = state.db_pool.as_ref()?;
    let Ok(mut listener) = ChangeListener::connect(pool, "ovg_team_collaboration").await else {
        return Some(UNAVAILABLE);
    };
    let org_str = org_id.to_string();
    let proj_str = proj_id.to_string();
    if sender.send(READY.to_owned()).await.is_err() {
        listener.close().await;
        return None;
    }
    let mut heartbeat = Instant::now() + HEARTBEAT_EVERY;
    let terminal = loop {
        let mut is_spatial = false;
        tokio::select! {
            () = sender.closed() => break None,
            notice = tokio::time::timeout_at(heartbeat, listener.next()) => {
                if let Ok(res) = notice {
                    let Ok(payload) = res else {
                        break Some(UNAVAILABLE);
                    };
                    if let Ok(val) = serde_json::from_str::<Value>(&payload) {
                        is_spatial = val.get("o").and_then(Value::as_str) == Some(org_str.as_str())
                            && val.get("p").and_then(Value::as_str) == Some(proj_str.as_str())
                            && val.get("kind").and_then(Value::as_str) == Some("spatial_change");
                    }
                }
            }
        }
        let frame = if is_spatial {
            heartbeat = Instant::now() + HEARTBEAT_EVERY;
            Some(SPATIAL_CHANGE)
        } else if Instant::now() >= heartbeat {
            heartbeat = Instant::now() + HEARTBEAT_EVERY;
            Some(HEARTBEAT)
        } else {
            None
        };
        if let Some(frame) = frame {
            if sender.send(frame.to_owned()).await.is_err() {
                break None;
            }
        }
    };
    listener.close().await;
    terminal
}
