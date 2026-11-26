//! E11 commit-delivered SSE invalidations for project chat, tasks, locks and edits
//! (`team_realtime.events`). Activity history paging lives with editing scopes.

use std::convert::Infallible;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::collaboration::ChangeListener;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::Instant;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::editing::{authenticate_actor, Actor};
use crate::AppState;

const CHANNEL: &str = "ovg_team_collaboration";
const HEARTBEAT_EVERY: Duration = Duration::from_secs(15);
const READY: &str = "retry: 3000\nevent: ready\ndata: {}\n\n";
const REVOKED: &str = "event: revoked\ndata: {}\n\n";
const UNAVAILABLE: &str = "event: unavailable\ndata: {}\n\n";
const HEARTBEAT: &str = ": heartbeat\n\n";
const CHANGE: &str = "event: change\ndata: {}\n\n";
const SPATIAL_CHANGE: &str = "event: spatial_change\ndata: {}\n\n";
const LOCK_CHANGE: &str = "event: lock_change\ndata: {}\n\n";

/// Dedicated PostgreSQL listener connections per process (Python `MAX_LISTENERS`).
static LISTENERS: Semaphore = Semaphore::const_new(40);

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(
        "/organizations/{organization_id}/projects/{project_id}/collaboration/events",
        get(events),
    )
}

#[derive(Clone, Copy)]
struct StreamScope {
    organization: Uuid,
    project: Uuid,
    actor: Uuid,
    session: Uuid,
}

async fn events(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Response> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization)
        .await
        .map_err(IntoResponse::into_response)?;
    let ids = Uuid::parse_str(&organization).ok();
    let ids = ids.zip(Uuid::parse_str(&project).ok());
    let Some((organization, project)) = ids else {
        return Err(CatalogError::InvalidRequest.into_response());
    };
    let Actor::Human(auth) = actor else {
        let detail = json!({"detail": "Human project member required"});
        return Err((StatusCode::FORBIDDEN, Json(detail)).into_response());
    };
    let read = project_read(
        &state,
        &auth,
        organization,
        project,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(IntoResponse::into_response)?;
    read.commit()
        .await
        .map_err(|error| CatalogError::Application(error).into_response())?;
    let scope = StreamScope {
        organization,
        project,
        actor: auth.actor_id(),
        session: auth.session_id(),
    };
    let (sender, receiver) = mpsc::channel(4);
    tokio::spawn(pump(state, sender, scope));
    let frames = futures_util::stream::unfold(receiver, |mut receiver| async move {
        let frame = receiver.recv().await?;
        Some((Ok::<_, Infallible>(frame), receiver))
    });
    let headers = [
        (header::CONTENT_TYPE, "text/event-stream; charset=utf-8"),
        (header::CACHE_CONTROL, "no-store, no-transform"),
        (header::HeaderName::from_static("x-accel-buffering"), "no"),
        (header::CONNECTION, "keep-alive"),
    ];
    Ok((headers, Body::from_stream(frames)).into_response())
}

async fn pump(state: AppState, sender: mpsc::Sender<&'static str>, scope: StreamScope) {
    let last = match LISTENERS.try_acquire() {
        Ok(_slot) => listen(&state, &sender, scope).await,
        Err(_) => Some(UNAVAILABLE),
    };
    if let Some(frame) = last {
        let _ = sender.send(frame).await;
    }
}

/// Deliver frames until the stream ends; returns the terminal frame, if any.
async fn listen(
    state: &AppState,
    sender: &mpsc::Sender<&'static str>,
    scope: StreamScope,
) -> Option<&'static str> {
    let Some(pool) = state.db_pool.as_ref() else {
        return Some(UNAVAILABLE);
    };
    // Authenticate after LISTEN and before the first refresh, then on every
    // relevant commit or heartbeat. Payloads carry no user-facing content.
    let Ok(mut listener) = ChangeListener::connect(pool, CHANNEL).await else {
        return Some(UNAVAILABLE);
    };
    if !still_allowed(state, scope).await {
        return Some(REVOKED);
    }
    sender.send(READY).await.ok()?;
    let mut heartbeat = Instant::now() + HEARTBEAT_EVERY;
    loop {
        let mut relevant = None;
        tokio::select! {
            () = sender.closed() => return None,
            notice = tokio::time::timeout_at(heartbeat, listener.next()) => {
                if let Ok(payload) = notice {
                    let Ok(payload) = payload else {
                        return Some(UNAVAILABLE);
                    };
                    relevant = matches(&payload, scope);
                }
            }
        }
        if relevant.is_none() && Instant::now() < heartbeat {
            continue;
        }
        if !still_allowed(state, scope).await {
            return Some(REVOKED);
        }
        sender.send(relevant.unwrap_or(HEARTBEAT)).await.ok()?;
        heartbeat = Instant::now() + HEARTBEAT_EVERY;
    }
}

/// Re-check the session and full project read access without a long-lived transaction.
async fn still_allowed(state: &AppState, scope: StreamScope) -> bool {
    let Some(pool) = state.db_pool.as_ref() else {
        return false;
    };
    let read = ProjectRead::begin(
        pool,
        scope.organization,
        scope.project,
        scope.actor,
        scope.session,
        ProjectReadPolicy::Unrestricted,
    )
    .await;
    match read {
        Ok(read) => read.commit().await.is_ok(),
        Err(_) => false,
    }
}

/// Python `matches`: same project, and either public or addressed to this actor.
fn matches(payload: &str, scope: StreamScope) -> Option<&'static str> {
    let event: Value = serde_json::from_str(payload).ok()?;
    let text = |key: &str| event.get(key).map(Value::as_str);
    let (organization, project, actor) = (
        scope.organization.to_string(),
        scope.project.to_string(),
        scope.actor.to_string(),
    );
    if text("o")?? != organization || text("p")?? != project {
        return None;
    }
    let addressed = event.get("a")?;
    if !addressed.is_null()
        && addressed.as_str() != Some(actor.as_str())
        && text("b")? != Some(actor.as_str())
    {
        return None;
    }
    Some(match event.get("kind").and_then(Value::as_str) {
        Some("spatial_change") => SPATIAL_CHANGE,
        Some("lock_change") => LOCK_CHANGE,
        _ => CHANGE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_follows_the_python_filter() {
        let scope = StreamScope {
            organization: Uuid::from_u128(1),
            project: Uuid::from_u128(2),
            actor: Uuid::from_u128(3),
            session: Uuid::from_u128(4),
        };
        let (o, p, me) = (scope.organization, scope.project, scope.actor);
        let other = Uuid::from_u128(9);
        let event = |a: Value, b: Value, kind: Value| {
            json!({"o": o, "p": p, "a": a, "b": b, "kind": kind}).to_string()
        };
        let cases = [
            (
                event(Value::Null, Value::Null, json!("spatial_change")),
                Some(SPATIAL_CHANGE),
            ),
            (
                event(json!(other), json!(me), json!("lock_change")),
                Some(LOCK_CHANGE),
            ),
            (event(json!(me), Value::Null, json!(5)), Some(CHANGE)),
            (event(json!(other), json!(other), json!("chat")), None),
            (json!({"o": o, "p": p}).to_string(), None),
            (
                json!({"o": o, "p": other, "a": null, "b": null}).to_string(),
                None,
            ),
            ("[1]".to_owned(), None),
            ("not json".to_owned(), None),
            // The recipient key is read only for addressed events.
            (json!({"o": o, "p": p, "a": null}).to_string(), Some(CHANGE)),
        ];
        for (payload, expected) in cases {
            assert_eq!(matches(&payload, scope), expected, "{payload}");
        }
    }
}
