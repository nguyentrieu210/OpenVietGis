//! R029 managed realtime GIS client (`managed_sync`) for QGIS: sessions, bootstrap,
//! durable delta replay, commit receipts, the SSE wake-up stream and desktop commands.
//!
//! Order follows FastAPI: malformed JSON (422) -> credential (401) ->
//! `project_access_guard` (404/403) -> CSRF (403) -> body/query validation (422)
//! -> project scope -> managed session -> domain.

use std::convert::Infallible;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::collaboration::ChangeListener;
use ovg_storage::editing::domain::py_strip;
use ovg_storage::editing::managed::NewSession;
use ovg_storage::editing::{CommitRequest, EditError, EditOperation, EditScope, Guard, Permission};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::mpsc;
use tokio::time::Instant;
use uuid::Uuid;

use crate::editing::{
    authenticate_actor, authenticate_json, decode, open, require_csrf, Actor, Failure,
};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integration-hub/managed";
const RETRY: &str = "retry: 3000\n\n";
const SESSION_REVOKED: &str = "event: session_revoked\ndata: {\"revoked\":true}\n\n";
const HEARTBEAT: &str = "event: heartbeat\ndata: {\"ok\":true}\n\n";

pub(crate) fn router() -> Router<AppState> {
    let commands = format!("{BASE}/desktop-sessions/{{session_id}}/commands");
    Router::new()
        .route(
            &format!("{BASE}/sessions"),
            get(list_sessions).post(create_session),
        )
        .route(
            &format!("{BASE}/sessions/{{session_id}}"),
            get(get_session).delete(revoke_session),
        )
        .route(&format!("{BASE}/bootstrap"), get(bootstrap))
        .route(&format!("{BASE}/delta"), get(delta))
        .route(&format!("{BASE}/commits"), post(commit))
        .route(&format!("{BASE}/commits/{{operation_id}}"), get(receipt))
        .route(&format!("{BASE}/events"), get(events))
        .route(&commands, get(list_commands).post(enqueue_command))
        .route(
            &format!("{commands}/{{command_id}}/respond"),
            post(respond_command),
        )
}

type Ids = Path<(String, String)>;
type SessionIds = Path<(String, String, String)>;
pub(crate) type Pairs = Vec<(String, String)>;

pub(crate) fn invalid() -> Failure {
    EditError::InvalidRequest.into()
}

pub(crate) fn query_pairs(uri: &Uri) -> Result<Pairs, Failure> {
    Query::<Pairs>::try_from_uri(uri)
        .map(|Query(pairs)| pairs)
        .map_err(|_| invalid())
}

/// The last repeated value wins, like Starlette's `QueryParams`.
pub(crate) fn param<'a>(pairs: &'a Pairs, key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

pub(crate) fn uuid_param(pairs: &Pairs, key: &str) -> Result<Uuid, Failure> {
    param(pairs, key)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(invalid)
}

pub(crate) fn int_param(
    pairs: &Pairs,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> Result<i64, Failure> {
    match param(pairs, key) {
        None => Ok(default),
        Some(value) => value
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| (min..=max).contains(value))
            .ok_or_else(invalid),
    }
}

/// Pydantic lax boolean from a query string.
pub(crate) fn bool_param(pairs: &Pairs, key: &str) -> Result<bool, Failure> {
    let Some(value) = param(pairs, key) else {
        return Ok(false);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "on" | "t" | "true" | "y" | "yes" => Ok(true),
        "0" | "off" | "f" | "false" | "n" | "no" => Ok(false),
        _ => Err(invalid()),
    }
}

/// `StringConstraints(strip_whitespace=True, min_length=1, max_length=max)`.
pub(crate) fn stripped(
    value: Option<String>,
    default: &str,
    max: usize,
) -> Result<String, Failure> {
    let value = value.unwrap_or_else(|| default.to_owned());
    let value = py_strip(&value);
    let length = value.chars().count();
    if length == 0 || length > max {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

pub(crate) fn uuid(value: &str) -> Result<Uuid, Failure> {
    Uuid::parse_str(value).map_err(|_| invalid())
}

/// GET: credential, then the guard (scoped accounts allowed on managed reads).
async fn read_scope<'a>(
    state: &'a AppState,
    ids: &(String, String),
    request: Request,
) -> Result<(Option<EditScope<'a>>, Pairs), Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(state, &mut parts, &ids.0).await?;
    let scope = open(state, &actor, &ids.0, &ids.1, Guard::ReadScopedAllowed).await?;
    Ok((scope, query_pairs(&parts.uri)?))
}

/// POST/DELETE: body, credential, guard (`SAFE_POST`/`SAFE_DELETE`), CSRF.
async fn write_scope<'a>(
    state: &'a AppState,
    ids: &(String, String),
    request: Request,
) -> Result<(Option<EditScope<'a>>, Option<Value>, Pairs), Failure> {
    let uri = request.uri().clone();
    let (actor, headers, body) = authenticate_json(state, request, &ids.0).await?;
    let scope = open(state, &actor, &ids.0, &ids.1, Guard::ScopedAllowed).await?;
    require_csrf(state, &actor, &headers)?;
    Ok((scope, body, query_pairs(&uri)?))
}

pub(crate) async fn finish(
    scope: EditScope<'_>,
    body: Value,
    status: StatusCode,
) -> Result<Response, Failure> {
    scope.commit().await?;
    Ok((status, Json(body)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSessionBody {
    #[serde(default)]
    binding_id: Option<Uuid>,
    #[serde(default)]
    device_label: Option<String>,
    #[serde(default)]
    client_version: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    expires_in_hours: Option<i64>,
}

async fn create_session(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, body, _) = write_scope(&state, &ids, request).await?;
    let body: CreateSessionBody = decode(body)?;
    let mode = body.mode.unwrap_or_else(|| "managed".to_owned());
    let hours = body.expires_in_hours.unwrap_or(24);
    let new = NewSession {
        binding_id: body.binding_id,
        device_label: stripped(body.device_label, "QGIS Desktop", 120)?,
        client_version: stripped(body.client_version, "0.9.0", 40)?,
        mode,
        expires_in_hours: hours,
    };
    if !matches!(new.mode.as_str(), "managed" | "legacy_snapshot") || !(1..=720).contains(&hours) {
        return Err(invalid());
    }
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    let result = scope.create_managed_session(new).await?;
    finish(scope, result, StatusCode::CREATED).await
}

async fn list_sessions(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, _) = read_scope(&state, &ids, request).await?;
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    let result = scope.list_managed_sessions().await?;
    finish(scope, result, StatusCode::OK).await
}

async fn get_session(
    State(state): State<AppState>,
    Path((organization, project, session)): SessionIds,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, _) = read_scope(&state, &(organization, project), request).await?;
    let (Some(mut scope), Ok(session_id)) = (scope, uuid(&session)) else {
        return Err(invalid());
    };
    let result = scope.get_managed_session(session_id).await?;
    finish(scope, result, StatusCode::OK).await
}

async fn revoke_session(
    State(state): State<AppState>,
    Path((organization, project, session)): SessionIds,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, _, pairs) = write_scope(&state, &(organization, project), request).await?;
    let bump_epoch = bool_param(&pairs, "bump_epoch")?;
    let (Some(mut scope), Ok(session_id)) = (scope, uuid(&session)) else {
        return Err(invalid());
    };
    let result = scope.revoke_managed_session(session_id, bump_epoch).await?;
    finish(scope, result, StatusCode::OK).await
}

async fn bootstrap(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, pairs) = read_scope(&state, &ids, request).await?;
    let layer_id = uuid_param(&pairs, "layer_id")?;
    let session_id = uuid_param(&pairs, "session_id")?;
    let limit = int_param(&pairs, "limit", 1000, 1, 5000)?;
    let offset = int_param(&pairs, "offset", 0, 0, i64::MAX)?;
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    let result = scope
        .managed_bootstrap(layer_id, session_id, limit, offset)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

async fn delta(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, pairs) = read_scope(&state, &ids, request).await?;
    let session_id = uuid_param(&pairs, "session_id")?;
    let cursor = int_param(&pairs, "cursor", 0, 0, i64::MAX)?;
    let limit = int_param(&pairs, "limit", 200, 1, 500)?;
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    let result = scope.managed_delta(session_id, cursor, limit).await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitBody {
    session_id: Uuid,
    source_id: Uuid,
    base_content_revision: String,
    #[serde(default)]
    base_schema_revision: Option<String>,
    operation_id: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    allow_deletes: bool,
    operations: Vec<EditOperation>,
}

async fn commit(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, body, _) = write_scope(&state, &ids, request).await?;
    let body: CommitBody = decode(body)?;
    let revision_ok = |value: &str| value.chars().count() == 64;
    if !revision_ok(&body.base_content_revision)
        || !body.base_schema_revision.as_deref().is_none_or(revision_ok)
        || !(1..=1000).contains(&body.operations.len())
    {
        return Err(invalid());
    }
    let operation_id = stripped(Some(body.operation_id), "", 128)?;
    let summary = stripped(body.summary, "QGIS Managed Realtime Edit", 500)?;
    let mut operations = Vec::with_capacity(body.operations.len());
    for mut operation in body.operations {
        operation.feature_id = stripped(Some(operation.feature_id), "", 128)?;
        operations.push(operation);
    }
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    let request = CommitRequest {
        operation_id,
        base_content_revision: body.base_content_revision,
        base_schema_revision: body.base_schema_revision,
        summary,
        operations,
    };
    let result = scope
        .managed_commit(body.session_id, body.source_id, body.allow_deletes, request)
        .await?;
    finish(scope, result, StatusCode::CREATED).await
}

async fn receipt(
    State(state): State<AppState>,
    Path((organization, project, operation_id)): SessionIds,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, _) = read_scope(&state, &(organization, project), request).await?;
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    let result = scope.managed_receipt(&operation_id).await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandBody {
    tool_name: String,
    #[serde(default)]
    arguments: Option<Map<String, Value>>,
    #[serde(default)]
    ttl_seconds: Option<i64>,
}

async fn enqueue_command(
    State(state): State<AppState>,
    Path((organization, project, session)): SessionIds,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, body, _) = write_scope(&state, &(organization, project), request).await?;
    let body: CommandBody = decode(body)?;
    let tool_name = stripped(Some(body.tool_name), "", 64)?;
    let ttl = body.ttl_seconds.unwrap_or(120);
    if !(10..=600).contains(&ttl) {
        return Err(invalid());
    }
    let (Some(mut scope), Ok(session_id)) = (scope, uuid(&session)) else {
        return Err(invalid());
    };
    let arguments = Value::Object(body.arguments.unwrap_or_default());
    let result = scope
        .enqueue_desktop_command(session_id, &tool_name, arguments, ttl)
        .await?;
    finish(scope, result, StatusCode::CREATED).await
}

async fn list_commands(
    State(state): State<AppState>,
    Path((organization, project, session)): SessionIds,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, pairs) = read_scope(&state, &(organization, project), request).await?;
    let (Some(mut scope), Ok(session_id)) = (scope, uuid(&session)) else {
        return Err(invalid());
    };
    let status = param(&pairs, "status").filter(|status| !status.is_empty());
    let result = scope.list_desktop_commands(session_id, status).await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RespondBody {
    status: String,
    #[serde(default)]
    result: Option<Map<String, Value>>,
}

async fn respond_command(
    State(state): State<AppState>,
    Path((organization, project, session, command)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (scope, body, _) = write_scope(&state, &(organization, project), request).await?;
    let body: RespondBody = decode(body)?;
    if !matches!(
        body.status.as_str(),
        "approved" | "rejected" | "completed" | "failed"
    ) {
        return Err(invalid());
    }
    let ids = uuid(&session).ok().zip(uuid(&command).ok());
    let (Some(mut scope), Some((session_id, command_id))) = (scope, ids) else {
        return Err(invalid());
    };
    let result = body.result.map(Value::Object);
    let result = scope
        .respond_desktop_command(session_id, command_id, &body.status, result)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

/// SSE wake-up stream: 5 s polls, a revocation recheck on each project notice and
/// a heartbeat (with recheck) every 15 s.
async fn events(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &ids.0).await?;
    let scope = open(&state, &actor, &ids.0, &ids.1, Guard::ReadScopedAllowed).await?;
    let pairs = query_pairs(&parts.uri)?;
    let session_id = uuid_param(&pairs, "session_id")?;
    let Some(mut scope) = scope else {
        return Err(invalid());
    };
    scope.authorize(Permission::Read).await?;
    scope.active_session(session_id, false, true).await?;
    scope.commit().await?;
    let (organization, project) = (uuid(&ids.0)?, uuid(&ids.1)?);

    let (sender, receiver) = mpsc::channel(4);
    let feed = ManagedFeed {
        actor,
        ids,
        session_id,
        organization: organization.to_string(),
        project: project.to_string(),
    };
    tokio::spawn(pump(state.clone(), sender, feed));
    let frames = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let frame = receiver.recv().await?;
        Some((Ok::<_, Infallible>(frame), receiver))
    });
    let headers = [
        (header::CONTENT_TYPE, "text/event-stream; charset=utf-8"),
        (header::CACHE_CONTROL, "no-cache"),
        (header::HeaderName::from_static("x-accel-buffering"), "no"),
    ];
    Ok((headers, Body::from_stream(frames)).into_response())
}

struct ManagedFeed {
    actor: Actor,
    ids: (String, String),
    session_id: Uuid,
    organization: String,
    project: String,
}

impl ManagedFeed {
    /// `_session_still_valid`: credential, project read and the active session.
    async fn still_valid(&self, state: &AppState) -> bool {
        let guard = Guard::ReadScopedAllowed;
        let opened = open(state, &self.actor, &self.ids.0, &self.ids.1, guard);
        let Ok(Some(mut scope)) = opened.await else {
            return false;
        };
        scope.authorize(Permission::Read).await.is_ok()
            && scope
                .active_session(self.session_id, false, true)
                .await
                .is_ok()
            && scope.commit().await.is_ok()
    }
}

/// `Some(frame)` for a notice about this project; the inner `None` sends no frame.
fn managed_notice(payload: &str, organization: &str, project: &str) -> Option<Option<String>> {
    let Ok(Value::Object(event)) = serde_json::from_str::<Value>(payload) else {
        return None;
    };
    let text = |key: &str| event.get(key).and_then(Value::as_str);
    if text("o") != Some(organization) || text("p") != Some(project) {
        return None;
    }
    // Python `str(p.get("kind") or "spatial_change")`: falsy values default.
    let falsy = match event.get("kind") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(value)) => !value,
        Some(Value::Number(value)) => value.as_f64() == Some(0.0),
        Some(Value::String(value)) => value.is_empty(),
        Some(Value::Array(value)) => value.is_empty(),
        Some(Value::Object(value)) => value.is_empty(),
    };
    let kind = if falsy {
        "spatial_change"
    } else {
        event.get("kind").and_then(Value::as_str).unwrap_or("")
    };
    let wakes = matches!(kind, "spatial_change" | "desktop_command");
    Some(wakes.then(|| format!("event: {kind}\ndata: {{\"kind\": \"{kind}\"}}\n\n")))
}

async fn pump(state: AppState, sender: mpsc::Sender<String>, feed: ManagedFeed) {
    if sender.send(RETRY.to_owned()).await.is_err() {
        return;
    }
    let Some(pool) = state.db_pool.as_ref() else {
        return;
    };
    let Ok(mut listener) = ChangeListener::connect(pool, "ovg_team_collaboration").await else {
        return;
    };
    wake(&state, &sender, &feed, &mut listener).await;
    listener.close().await;
}

async fn wake(
    state: &AppState,
    sender: &mpsc::Sender<String>,
    feed: &ManagedFeed,
    listener: &mut ChangeListener,
) {
    let mut last_heartbeat = Instant::now();
    loop {
        let mut notice = None;
        tokio::select! {
            () = sender.closed() => return,
            received = tokio::time::timeout(Duration::from_secs(5), listener.next()) => {
                match received {
                    Ok(Ok(payload)) => {
                        notice = managed_notice(&payload, &feed.organization, &feed.project);
                    }
                    Ok(Err(_)) => return,
                    Err(_) => {}
                }
            }
        }
        if let Some(frame) = notice {
            if !feed.still_valid(state).await {
                let _ = sender.send(SESSION_REVOKED.to_owned()).await;
                return;
            }
            // A valid session ignores another session's revoke notice.
            if let Some(frame) = frame {
                if sender.send(frame).await.is_err() {
                    return;
                }
            }
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(15) {
            if !feed.still_valid(state).await {
                let _ = sender.send(SESSION_REVOKED.to_owned()).await;
                return;
            }
            if sender.send(HEARTBEAT.to_owned()).await.is_err() {
                return;
            }
            last_heartbeat = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_notice_follows_the_python_filter() {
        let notice = |payload: &str| managed_notice(payload, "o", "p");
        let frame = |kind: &str| format!("event: {kind}\ndata: {{\"kind\": \"{kind}\"}}\n\n");
        let spatial = Some(Some(frame("spatial_change")));
        assert_eq!(notice(r#"{"o":"o","p":"p"}"#), spatial);
        assert_eq!(notice(r#"{"o":"o","p":"p","kind":0}"#), spatial);
        assert_eq!(notice(r#"{"o":"o","p":"p","kind":""}"#), spatial);
        let desktop = Some(Some(frame("desktop_command")));
        assert_eq!(
            notice(r#"{"o":"o","p":"p","kind":"desktop_command"}"#),
            desktop
        );
        assert_eq!(
            notice(r#"{"o":"o","p":"p","kind":"session_revoked"}"#),
            Some(None)
        );
        assert_eq!(notice(r#"{"o":"o","p":"p","kind":5}"#), Some(None));
        assert_eq!(notice(r#"{"o":"o","p":"x"}"#), None);
        assert_eq!(notice("[]"), None);
        assert_eq!(notice("nope"), None);
    }
}
