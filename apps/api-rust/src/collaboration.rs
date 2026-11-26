//! E11 commit-delivered SSE invalidations: project chat, tasks, locks and edits
//! (`team_realtime.events`) and organization rooms and contacts (`org_chat.events`).
//! Activity history paging lives with the editing scopes.

use std::convert::Infallible;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::collaboration::ChangeListener;
use ovg_storage::tenancy::TenantTx;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::Instant;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::editing::{authenticate_actor, Actor};
use crate::AppState;

const HEARTBEAT_EVERY: Duration = Duration::from_secs(15);
const READY: &str = "retry: 3000\nevent: ready\ndata: {}\n\n";
const REVOKED: &str = "event: revoked\ndata: {}\n\n";
const UNAVAILABLE: &str = "event: unavailable\ndata: {}\n\n";
const HEARTBEAT: &str = ": heartbeat\n\n";
const CHANGE: &str = "event: change\ndata: {}\n\n";
const SPATIAL_CHANGE: &str = "event: spatial_change\ndata: {}\n\n";
const LOCK_CHANGE: &str = "event: lock_change\ndata: {}\n\n";
const CONTACTS: &str = "event: contacts\ndata: {}\n\n";

/// Dedicated PostgreSQL listener connections per process and module (Python `MAX_LISTENERS`).
static PROJECT_LISTENERS: Semaphore = Semaphore::const_new(40);
static CHAT_LISTENERS: Semaphore = Semaphore::const_new(40);

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/organizations/{organization_id}/projects/{project_id}/collaboration/events",
            get(project_events),
        )
        .route(
            "/organizations/{organization_id}/chat/events",
            get(chat_events),
        )
}

/// The Python stream a connection reproduces, with its authenticated scope.
enum Feed {
    Project {
        organization: Uuid,
        project: Uuid,
        actor: Uuid,
        session: Uuid,
    },
    Chat {
        organization: Uuid,
        actor: Uuid,
        session: Uuid,
        display_name: String,
    },
}

enum Event {
    Frame(&'static str),
    /// A room notice; delivered only for a room this member can read.
    Room(Option<String>),
    Contacts,
}

impl Feed {
    fn channel(&self) -> &'static str {
        match self {
            Self::Project { .. } => "ovg_team_collaboration",
            Self::Chat { .. } => "ovg_chat",
        }
    }

    fn slots(&self) -> &'static Semaphore {
        match self {
            Self::Project { .. } => &PROJECT_LISTENERS,
            Self::Chat { .. } => &CHAT_LISTENERS,
        }
    }

    fn parse(&self, payload: &str) -> Option<Event> {
        match self {
            Self::Project {
                organization,
                project,
                actor,
                ..
            } => matches(payload, *organization, *project, *actor).map(Event::Frame),
            Self::Chat {
                organization,
                actor,
                ..
            } => chat_event(payload, *organization, *actor),
        }
    }

    /// Re-authenticate without a long-lived transaction: the readable rooms
    /// (none for a project stream), or `None` once access is revoked.
    async fn allowed(&self, state: &AppState) -> Option<Vec<String>> {
        match self {
            Self::Project {
                organization,
                project,
                actor,
                session,
            } => {
                let pool = state.db_pool.as_ref()?;
                let read = ProjectRead::begin(
                    pool,
                    *organization,
                    *project,
                    *actor,
                    *session,
                    ProjectReadPolicy::Unrestricted,
                )
                .await
                .ok()?;
                read.commit().await.ok().map(|()| Vec::new())
            }
            Self::Chat {
                organization,
                actor,
                session,
                display_name,
            } => {
                let rooms = chat_rooms(state, *organization, *actor, *session, display_name);
                rooms.await.ok()
            }
        }
    }
}

/// `org_chat.org_scope` + `allowed_rooms` in one fresh transaction.
async fn chat_rooms(
    state: &AppState,
    organization: Uuid,
    actor: Uuid,
    session: Uuid,
    display_name: &str,
) -> Result<Vec<String>, AppError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        AppError::service_unavailable("DATABASE_UNAVAILABLE", "database unavailable")
    })?;
    let mut tx = TenantTx::begin(pool, actor, session, display_name).await?;
    let rooms = tx.chat_rooms(organization).await?;
    tx.commit().await?;
    Ok(rooms.iter().map(Uuid::to_string).collect())
}

impl Event {
    fn frame(self, rooms: &[String]) -> Option<String> {
        match self {
            Self::Frame(frame) => Some(frame.to_owned()),
            Self::Contacts => Some(CONTACTS.to_owned()),
            Self::Room(room) => room
                .filter(|room| rooms.contains(room))
                .map(|room| format!("event: chat\ndata: {{\"room_id\": \"{room}\"}}\n\n")),
        }
    }
}

async fn project_events(
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
        return Err(forbidden("Human project member required"));
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
    let feed = Feed::Project {
        organization,
        project,
        actor: auth.actor_id(),
        session: auth.session_id(),
    };
    Ok(stream(state, feed))
}

async fn chat_events(
    State(state): State<AppState>,
    Path(organization): Path<String>,
    request: Request,
) -> Result<Response, Response> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization)
        .await
        .map_err(IntoResponse::into_response)?;
    let Ok(organization) = Uuid::parse_str(&organization) else {
        return Err(CatalogError::InvalidRequest.into_response());
    };
    let Actor::Human(auth) = actor else {
        return Err(forbidden("Human organisation member required"));
    };
    let (actor, session) = (auth.actor_id(), auth.session_id());
    let rooms = chat_rooms(&state, organization, actor, session, auth.display_name()).await;
    rooms.map_err(|error| CatalogError::Application(error).into_response())?;
    let feed = Feed::Chat {
        organization,
        actor,
        session,
        display_name: auth.display_name().to_owned(),
    };
    Ok(stream(state, feed))
}

fn forbidden(detail: &str) -> Response {
    (StatusCode::FORBIDDEN, Json(json!({ "detail": detail }))).into_response()
}

fn stream(state: AppState, feed: Feed) -> Response {
    let (sender, receiver) = mpsc::channel(4);
    tokio::spawn(pump(state, sender, feed));
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
    (headers, Body::from_stream(frames)).into_response()
}

async fn pump(state: AppState, sender: mpsc::Sender<String>, feed: Feed) {
    let last = match feed.slots().try_acquire() {
        Ok(_slot) => listen(&state, &sender, &feed).await,
        Err(_) => Some(UNAVAILABLE),
    };
    if let Some(frame) = last {
        let _ = sender.send(frame.to_owned()).await;
    }
}

/// Deliver frames until the stream ends; returns the terminal frame, if any.
async fn listen(
    state: &AppState,
    sender: &mpsc::Sender<String>,
    feed: &Feed,
) -> Option<&'static str> {
    let Some(pool) = state.db_pool.as_ref() else {
        return Some(UNAVAILABLE);
    };
    // Authenticate after LISTEN and before the first refresh, then on every
    // relevant commit or heartbeat. Payloads carry no user-facing content.
    let Ok(mut listener) = ChangeListener::connect(pool, feed.channel()).await else {
        return Some(UNAVAILABLE);
    };
    if feed.allowed(state).await.is_none() {
        return Some(REVOKED);
    }
    sender.send(READY.to_owned()).await.ok()?;
    let mut heartbeat = Instant::now() + HEARTBEAT_EVERY;
    loop {
        let mut event = None;
        tokio::select! {
            () = sender.closed() => return None,
            notice = tokio::time::timeout_at(heartbeat, listener.next()) => {
                if let Ok(payload) = notice {
                    let Ok(payload) = payload else {
                        return Some(UNAVAILABLE);
                    };
                    event = feed.parse(&payload);
                }
            }
        }
        if event.is_none() && Instant::now() < heartbeat {
            continue;
        }
        let Some(rooms) = feed.allowed(state).await else {
            return Some(REVOKED);
        };
        let frame = match event {
            Some(event) => event.frame(&rooms),
            None => Some(HEARTBEAT.to_owned()),
        };
        if let Some(frame) = frame {
            sender.send(frame).await.ok()?;
        }
        heartbeat = Instant::now() + HEARTBEAT_EVERY;
    }
}

/// Python `team_realtime.matches`: same project, and public or addressed to this actor.
fn matches(payload: &str, organization: Uuid, project: Uuid, actor: Uuid) -> Option<&'static str> {
    let event: Value = serde_json::from_str(payload).ok()?;
    let text = |key: &str| event.get(key).map(Value::as_str);
    let (organization, project, actor) = (
        organization.to_string(),
        project.to_string(),
        actor.to_string(),
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

/// Python `org_chat.parse`: a room in this organization, or a contact change naming this actor.
fn chat_event(payload: &str, organization: Uuid, actor: Uuid) -> Option<Event> {
    let event: Value = serde_json::from_str(payload).ok()?;
    let kind = event.get("k")?;
    let organization = organization.to_string();
    if kind == "chat" && event.get("o")? == organization.as_str() {
        return Some(Event::Room(event.get("r")?.as_str().map(str::to_owned)));
    }
    if kind != "contacts" {
        return None;
    }
    let actor = actor.to_string();
    // Python `str(actor) in event["u"]`: list item, substring or key; other types raise.
    let named = match event.get("u")? {
        Value::Array(items) => items.contains(&Value::String(actor.clone())),
        Value::String(text) => text.contains(&actor),
        Value::Object(map) => map.contains_key(&actor),
        _ => return None,
    };
    named.then_some(Event::Contacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: Uuid = Uuid::from_u128(1);
    const P: Uuid = Uuid::from_u128(2);
    const ME: Uuid = Uuid::from_u128(3);
    const OTHER: Uuid = Uuid::from_u128(9);

    #[test]
    fn matches_follows_the_python_filter() {
        let event = |a: Value, b: Value, kind: Value| {
            json!({"o": O, "p": P, "a": a, "b": b, "kind": kind}).to_string()
        };
        let cases = [
            (
                event(Value::Null, Value::Null, json!("spatial_change")),
                Some(SPATIAL_CHANGE),
            ),
            (
                event(json!(OTHER), json!(ME), json!("lock_change")),
                Some(LOCK_CHANGE),
            ),
            (event(json!(ME), Value::Null, json!(5)), Some(CHANGE)),
            (event(json!(OTHER), json!(OTHER), json!("chat")), None),
            (json!({"o": O, "p": P}).to_string(), None),
            (
                json!({"o": O, "p": OTHER, "a": null, "b": null}).to_string(),
                None,
            ),
            ("[1]".to_owned(), None),
            ("not json".to_owned(), None),
            // The recipient key is read only for addressed events.
            (json!({"o": O, "p": P, "a": null}).to_string(), Some(CHANGE)),
        ];
        for (payload, expected) in cases {
            assert_eq!(matches(&payload, O, P, ME), expected, "{payload}");
        }
    }

    #[test]
    fn chat_event_follows_the_python_parser() {
        let room = |payload: Value| match chat_event(&payload.to_string(), O, ME) {
            Some(Event::Room(room)) => Some(room),
            _ => None,
        };
        let contacts = |payload: Value| {
            matches!(
                chat_event(&payload.to_string(), O, ME),
                Some(Event::Contacts)
            )
        };
        let id = OTHER.to_string();
        assert_eq!(
            room(json!({"k": "chat", "o": O, "r": id})),
            Some(Some(id.clone()))
        );
        assert_eq!(room(json!({"k": "chat", "o": O, "r": 5})), Some(None));
        assert_eq!(room(json!({"k": "chat", "o": OTHER, "r": id})), None);
        assert_eq!(room(json!({"k": "chat", "o": O})), None);
        assert!(contacts(json!({"k": "contacts", "u": [OTHER, ME]})));
        assert!(contacts(json!({"k": "contacts", "u": format!("x{ME}x")})));
        assert!(!contacts(json!({"k": "contacts", "u": [OTHER]})));
        assert!(!contacts(json!({"k": "contacts", "u": null})));
        assert!(chat_event("[]", O, ME).is_none());

        let rooms = [id.clone()];
        let frame = Event::Room(Some(id.clone())).frame(&rooms).unwrap();
        let expected = format!("event: chat\ndata: {{\"room_id\": \"{id}\"}}\n\n");
        assert_eq!(frame, expected);
        assert!(Event::Room(Some(ME.to_string())).frame(&rooms).is_none());
    }
}
