//! E11 project collaboration: activity history paging and commit-delivered SSE invalidations.

use std::convert::Infallible;
use std::time::Duration;

use axum::body::Body;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Timelike, Utc};
use ovg_core::{ActorContext, ActorType, AppError, TenantScope};
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::collaboration::{self, ActivityPosition, ChangeListener};
use ovg_sync::activity_cursor::{
    ActivityCursor, ActivityCursorScope, ActivityStream, CURSOR_MAX_LENGTH,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::Instant;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/collaboration";
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
    Router::new()
        .route(&format!("{BASE}/activities"), get(activities))
        .route(&format!("{BASE}/events"), get(events))
}

enum Failure {
    Access(CatalogError),
    /// `PlatformDomainError` envelope: `{"code", "message"}`.
    Domain(AppError),
}

impl From<CatalogError> for Failure {
    fn from(error: CatalogError) -> Self {
        Self::Access(error)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "collaboration request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => (
                StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(json!({"code": error.code(), "message": error.to_string()})),
            )
                .into_response(),
        }
    }
}

#[derive(Deserialize)]
struct ActivityQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn activities(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<ActivityQuery>, QueryRejection>,
) -> Result<Response, Failure> {
    let Path((organization, project)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization,
        project,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let Query(query) = query.map_err(|_| CatalogError::InvalidRequest)?;
    let limit = query.limit.unwrap_or(50);
    let cursor_chars = query.cursor.as_deref().map(|token| token.chars().count());
    if !(1..=200).contains(&limit)
        || cursor_chars == Some(0)
        || cursor_chars > Some(CURSOR_MAX_LENGTH)
    {
        return Err(CatalogError::InvalidRequest.into());
    }

    // Python datetimes carry microseconds; the cursor and SQL comparisons use the same.
    let now = Utc::now();
    let now = now
        .with_nanosecond(now.nanosecond() / 1_000 * 1_000)
        .unwrap_or(now);
    let scope = ActivityCursorScope::from(&TenantScope {
        organization_id: organization,
        workspace_id: None,
        project_id: Some(project),
        actor: ActorContext {
            actor_type: ActorType::Human,
            actor_id: auth.actor_id(),
            credential_id: None,
            email: None,
        },
        permissions: Vec::new(),
    });
    let position = query
        .cursor
        .map(|token| ActivityCursor::decode(&token, &scope, now))
        .transpose()
        .map_err(Failure::Domain)?;
    let cutoff = position.map_or(now, |cursor| cursor.cutoff);
    let after = position.map(|cursor| ActivityPosition {
        created_at: cursor.created_at,
        stream: cursor.stream as i32,
        id: cursor.id,
    });
    let page = collaboration::list_activities(&mut read, cutoff, after, limit).await;
    let Some(mut rows) = page.map_err(Failure::Domain)? else {
        return Err(Failure::Domain(AppError::validation(
            "INVALID_ACTIVITY_CURSOR",
            "activity continuation unavailable; refresh the activity list",
        )));
    };
    read.commit().await.map_err(Failure::Domain)?;

    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let mut next_cursor = None;
    if let Some(last) = rows.last().filter(|_| has_more) {
        let stream = if last.position.stream == 1 {
            ActivityStream::Edit
        } else {
            ActivityStream::Review
        };
        let cursor = ActivityCursor {
            cutoff,
            created_at: last.position.created_at,
            stream,
            id: last.position.id,
        };
        next_cursor = Some(cursor.encode(&scope).map_err(Failure::Domain)?);
    }
    let items: Vec<Value> = rows.into_iter().map(|row| row.wire).collect();
    let body = json!({
        "items": items,
        "cutoff": collaboration::isoformat(cutoff),
        "next_cursor": next_cursor,
    });
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[derive(Clone, Copy)]
struct StreamScope {
    organization: Uuid,
    project: Uuid,
    actor: Uuid,
    session: Uuid,
}

async fn events(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, Failure> {
    let Path((organization, project)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let read = project_read(
        &state,
        &auth,
        organization,
        project,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    read.commit().await.map_err(Failure::Domain)?;
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
