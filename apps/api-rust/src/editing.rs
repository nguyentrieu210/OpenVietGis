//! E03 editing routes (R006): changeset commit/list/detail/revert and snapping.
//!
//! FastAPI order is preserved: malformed JSON (422) -> session (401) ->
//! `project_access_guard` (404/403) -> CSRF (403) -> body/query validation (422)
//! -> project scope and permission -> domain.

use axum::body::to_bytes;
use axum::extract::{FromRequestParts, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Timelike, Utc};
use ovg_core::{ActorContext, ActorType, AppError, TenantScope};
use ovg_storage::collaboration::{isoformat, ActivityPosition};
use ovg_storage::editing::domain::{py_strip, Segment, Xy};
use ovg_storage::editing::{
    authenticate_service, snap, CommitRequest, EditError, EditScope, Guard, Permission, ServiceAuth,
};
use ovg_sync::activity_cursor::{
    ActivityCursor, ActivityCursorScope, ActivityStream, CURSOR_MAX_LENGTH,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::{self, HumanSession, IdentityError};
use crate::AppState;

// shortcut: FastAPI has no body cap; 64 MiB covers 2000 ops x 4096 vertices of JSON.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn router() -> Router<AppState> {
    const PROJECT: &str = "/organizations/{organization_id}/projects/{project_id}";
    Router::new()
        .route(
            &format!("{PROJECT}/sources/{{source_id}}/changesets"),
            get(list_changesets).post(commit_changeset),
        )
        .route(
            &format!("{PROJECT}/changesets/{{changeset_id}}"),
            get(get_changeset),
        )
        .route(
            &format!("{PROJECT}/changesets/{{changeset_id}}/revert"),
            post(revert_changeset),
        )
        .route(&format!("{PROJECT}/editing/relations"), get(list_relations).post(create_relation))
        .route(&format!("{PROJECT}/editing/relations/{{relation_id}}/delete"), post(delete_relation))
        .route(&format!("{PROJECT}/editing/snap"), post(compute_snap))
        .route(
            &format!("{PROJECT}/collaboration/activities"),
            get(list_activities),
        )
}

/// R011 activity history (`collaboration.list_activities`): edits merged with reviews,
/// keyset paged by a cursor bound to the actor, credential and project.
async fn list_activities(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    uri: Uri,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::Read).await?;
    let limit = query_limit(&uri)?;
    let cursor = query_cursor(&uri)?;
    let org = Uuid::parse_str(&organization).ok();
    let ids = org.zip(Uuid::parse_str(&project).ok());
    let (Some(mut scope), Some((organization_id, project_id))) = (scope, ids) else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;

    // Python datetimes carry microseconds; the cursor and SQL comparisons use the same.
    let now = Utc::now();
    let now = now
        .with_nanosecond(now.nanosecond() / 1_000 * 1_000)
        .unwrap_or(now);
    let (actor_id, credential_id) = scope.actor();
    let actor_type = match actor {
        Actor::Human(_) => ActorType::Human,
        Actor::Service(_) => ActorType::ServicePrincipal,
    };
    let cursor_scope = ActivityCursorScope::from(&TenantScope {
        organization_id,
        workspace_id: None,
        project_id: Some(project_id),
        actor: ActorContext {
            actor_type,
            actor_id,
            credential_id,
            email: None,
        },
        permissions: Vec::new(),
    });
    let position = cursor
        .map(|token| ActivityCursor::decode(&token, &cursor_scope, now))
        .transpose()
        .map_err(platform)?;
    let cutoff = position.map_or(now, |cursor| cursor.cutoff);
    let after = position.map(|cursor| ActivityPosition {
        created_at: cursor.created_at,
        stream: cursor.stream as i32,
        id: cursor.id,
    });
    let Some(mut rows) = scope.list_activities(cutoff, after, limit).await? else {
        return Err(platform(AppError::validation(
            "INVALID_ACTIVITY_CURSOR",
            "activity continuation unavailable; refresh the activity list",
        )));
    };
    scope.commit().await?;

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
        next_cursor = Some(cursor.encode(&cursor_scope).map_err(platform)?);
    }
    let items: Vec<Value> = rows.into_iter().map(|row| row.wire).collect();
    let body = json!({
        "items": items,
        "cutoff": isoformat(cutoff),
        "next_cursor": next_cursor,
    });
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// FastAPI `cursor: Query(min_length=1, max_length=512)`; the last repeated value wins.
fn query_cursor(uri: &Uri) -> Result<Option<String>, Failure> {
    let Query(pairs) =
        Query::<Vec<(String, String)>>::try_from_uri(uri).map_err(|_| EditError::InvalidRequest)?;
    let cursor = pairs.into_iter().rev().find(|(key, _)| key == "cursor");
    let cursor = cursor.map(|(_, token)| token);
    let length = cursor.as_deref().map_or(1, |token| token.chars().count());
    if length == 0 || length > CURSOR_MAX_LENGTH {
        return Err(EditError::InvalidRequest.into());
    }
    Ok(cursor)
}

/// `PlatformDomainError` envelope for a domain `AppError`.
fn platform(error: AppError) -> Failure {
    Failure::Edit(EditError::Platform {
        status: error.status_code(),
        code: error.code(),
        message: error.to_string(),
        details: None,
    })
}

type Ids = Path<(String, String, String)>;

async fn list_relations(
    State(state): State<AppState>,
    Path((organization,project)): Path<(String,String)>,
    request: Request,
) -> Result<Json<Value>,Failure> {
    let (mut parts,_) = request.into_parts();
    let actor=authenticate_actor(&state,&mut parts,&organization).await?;
    let Some(mut scope)=open(&state,&actor,&organization,&project,Guard::Read).await? else {
        return Err(EditError::InvalidRequest.into());
    };
    let result=scope.list_relations().await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn create_relation(
    State(state): State<AppState>,
    Path((organization,project)): Path<(String,String)>,
    request: Request,
) -> Result<Json<Value>,Failure> {
    let (actor,headers,body)=authenticate_json(&state,request,&organization).await?;
    let Some(mut scope)=open(&state,&actor,&organization,&project,Guard::ScopedDenied).await? else {
        return Err(EditError::InvalidRequest.into());
    };
    require_csrf(&state,&actor,&headers)?;
    let body: ovg_storage::editing::RelationRequest=decode(body)?;
    let result=scope.create_relation(body).await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn delete_relation(
    State(state): State<AppState>,
    Path((organization,project,id)): Ids,
    request: Request,
) -> Result<Json<Value>,Failure> {
    let (mut parts,_) = request.into_parts();
    let actor=authenticate_actor(&state,&mut parts,&organization).await?;
    let Some(mut scope)=open(&state,&actor,&organization,&project,Guard::ScopedDenied).await? else {
        return Err(EditError::InvalidRequest.into());
    };
    require_csrf(&state,&actor,&parts.headers)?;
    let id=Uuid::parse_str(&id).map_err(|_|EditError::InvalidRequest)?;
    let result=scope.delete_relation(id).await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn commit_changeset(
    State(state): State<AppState>,
    Path((organization, project, source)): Ids,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (session, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(
        &state,
        &session,
        &organization,
        &project,
        Guard::ScopedAllowed,
    )
    .await?;
    require_csrf(&state, &session, &headers)?;
    let mut body = body;
    let prevent_overlap = match body.as_mut().and_then(Value::as_object_mut) {
        Some(object) => match object.remove("prevent_overlap") {
            None | Some(Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            _ => return Err(EditError::InvalidRequest.into()),
        },
        None => false,
    };
    let body: CommitRequest = decode(body)?;
    let schema_ok = body
        .base_schema_revision
        .as_deref()
        .is_none_or(|s| s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    if !schema_ok || body.summary.chars().count() > 500 || reserved(&body.operation_id) {
        return Err(EditError::InvalidRequest.into());
    }
    let (Some(mut scope), Ok(source_id)) = (scope, Uuid::parse_str(&source)) else {
        return Err(EditError::InvalidRequest.into());
    };
    let result = if prevent_overlap {
        scope.commit_changeset_no_overlap(source_id, body).await?
    } else {
        scope.commit_changeset(source_id, body).await?
    };
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_changesets(
    State(state): State<AppState>,
    Path((organization, project, source)): Ids,
    uri: Uri,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let session = authenticate_actor(&state, &mut parts, &organization).await?;
    let scope = open(&state, &session, &organization, &project, Guard::Read).await?;
    let limit = query_limit(&uri)?;
    let (Some(mut scope), Ok(source_id)) = (scope, Uuid::parse_str(&source)) else {
        return Err(EditError::InvalidRequest.into());
    };
    let result = scope.list_changesets(source_id, limit).await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn get_changeset(
    State(state): State<AppState>,
    Path((organization, project, changeset)): Ids,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let session = authenticate_actor(&state, &mut parts, &organization).await?;
    let scope = open(&state, &session, &organization, &project, Guard::Read).await?;
    let (Some(mut scope), Ok(changeset_id)) = (scope, Uuid::parse_str(&changeset)) else {
        return Err(EditError::InvalidRequest.into());
    };
    let result = scope.get_changeset(changeset_id).await?;
    scope.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevertBody {
    operation_id: String,
    #[serde(default)]
    summary: Option<String>,
}

async fn revert_changeset(
    State(state): State<AppState>,
    Path((organization, project, changeset)): Ids,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (session, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(
        &state,
        &session,
        &organization,
        &project,
        Guard::ScopedDenied,
    )
    .await?;
    require_csrf(&state, &session, &headers)?;
    let body: RevertBody = decode(body)?;
    if reserved(&body.operation_id) {
        return Err(EditError::InvalidRequest.into());
    }
    let (Some(mut scope), Ok(changeset_id)) = (scope, Uuid::parse_str(&changeset)) else {
        return Err(EditError::InvalidRequest.into());
    };
    let result = scope
        .revert_changeset(changeset_id, body.operation_id, body.summary)
        .await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapBody {
    target: Xy,
    #[serde(default)]
    vertices: Vec<Xy>,
    #[serde(default)]
    segments: Vec<Segment>,
    tolerance: f64,
}

async fn compute_snap(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (session, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(
        &state,
        &session,
        &organization,
        &project,
        Guard::ScopedAllowed,
    )
    .await?;
    require_csrf(&state, &session, &headers)?;
    let body: SnapBody = decode(body)?;
    let tolerance_ok = body.tolerance > 0.0 && body.tolerance <= 10_000.0;
    let (Some(mut scope), true) = (scope, tolerance_ok) else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Edit).await?;
    let result = snap(body.target, &body.vertices, &body.segments, body.tolerance);
    scope.commit().await?;
    Ok(Json(result))
}

pub(crate) enum Actor {
    Human(HumanSession),
    Service(ServiceAuth),
}

pub(crate) async fn authenticate_actor(
    state: &AppState,
    parts: &mut axum::http::request::Parts,
    organization: &str,
) -> Result<Actor, Failure> {
    let bearer = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        })
        .map(|value| py_strip(&value[7..]));
    if let Some(raw) = bearer {
        let org = Uuid::parse_str(organization)
            .map_err(|_| EditError::detail(401, "invalid service credential"))?;
        let pool = state
            .db_pool
            .as_ref()
            .ok_or_else(|| EditError::detail(503, "database unavailable"))?;
        return Ok(Actor::Service(authenticate_service(pool, org, raw).await?));
    }
    HumanSession::from_request_parts(parts, state)
        .await
        .map(Actor::Human)
        .map_err(|error| Failure::Identity(Box::new(error)))
}

pub(crate) type Authenticated = (Actor, HeaderMap, Option<Value>);

/// FastAPI decodes a JSON body before dependencies run, then authenticates.
pub(crate) async fn authenticate_json(
    state: &AppState,
    request: Request,
    organization: &str,
) -> Result<Authenticated, Failure> {
    let (mut parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| EditError::InvalidRequest)?;
    let json = if bytes.is_empty() || !json_content_type(&parts.headers) {
        None
    } else {
        Some(serde_json::from_slice(&bytes).map_err(|_| EditError::InvalidRequest)?)
    };
    let session = authenticate_actor(state, &mut parts, organization).await?;
    Ok((session, parts.headers, json))
}

fn json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get(header::CONTENT_TYPE) else {
        return true;
    };
    let value = value.to_str().unwrap_or("invalid");
    if value.is_empty() {
        return true;
    }
    let mime = value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    mime.split_once('/').is_some_and(|(main, sub)| {
        main == "application" && (sub == "json" || sub.ends_with("+json"))
    })
}

/// Project guard; skipped (like the oracle) when the path IDs are not UUIDs.
pub(crate) async fn open<'a>(
    state: &'a AppState,
    session: &Actor,
    organization: &str,
    project: &str,
    guard: Guard,
) -> Result<Option<EditScope<'a>>, Failure> {
    let (Ok(organization_id), Ok(project_id)) =
        (Uuid::parse_str(organization), Uuid::parse_str(project))
    else {
        return Ok(None);
    };
    let pool = state
        .db_pool
        .as_ref()
        .ok_or_else(|| EditError::detail(503, "database unavailable"))?;
    let scope = match session {
        Actor::Human(session) => {
            EditScope::begin(
                pool,
                organization_id,
                project_id,
                session.actor_id(),
                session.session_id(),
                guard,
            )
            .await?
        }
        Actor::Service(auth) => {
            EditScope::begin_service(pool, organization_id, project_id, auth, guard).await?
        }
    };
    Ok(Some(scope))
}

pub(crate) fn require_csrf(
    state: &AppState,
    session: &Actor,
    headers: &HeaderMap,
) -> Result<(), Failure> {
    let Actor::Human(session) = session else {
        return Ok(());
    };
    identity::require_csrf(state, session, headers)
        .map_err(|error| Failure::Identity(Box::new(error)))
}

pub(crate) fn decode<T: DeserializeOwned>(body: Option<Value>) -> Result<T, Failure> {
    body.and_then(|value| serde_json::from_value(value).ok())
        .ok_or(Failure::Edit(EditError::InvalidRequest))
}

/// Field-sync recovery operation IDs belong only to the internal repository.
fn reserved(operation_id: &str) -> bool {
    py_strip(operation_id).starts_with("field-sync-")
}

fn query_limit(uri: &Uri) -> Result<i64, Failure> {
    let Query(pairs) =
        Query::<Vec<(String, String)>>::try_from_uri(uri).map_err(|_| EditError::InvalidRequest)?;
    match pairs.into_iter().rev().find(|(key, _)| key == "limit") {
        None => Ok(50),
        Some((_, value)) => value
            .parse::<i64>()
            .ok()
            .filter(|limit| (1..=200).contains(limit))
            .ok_or(Failure::Edit(EditError::InvalidRequest)),
    }
}

pub(crate) enum Failure {
    Edit(EditError),
    Identity(Box<IdentityError>),
}

impl From<EditError> for Failure {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Identity(error) => return error.into_response(),
            Self::Edit(error) => error,
        };
        let http =
            |code: u16| StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        match error {
            EditError::Detail(code, detail) => {
                (http(code), Json(json!({ "detail": detail }))).into_response()
            }
            EditError::Platform {
                status,
                code,
                message,
                details,
            } => {
                let mut body = json!({ "code": code, "message": message });
                let details = details.filter(|d| d.as_object().is_none_or(|m| !m.is_empty()));
                if let Some(details) = details {
                    body["details"] = details;
                }
                (http(status), Json(body)).into_response()
            }
            EditError::Spatial(status, code, detail) => (
                http(status),
                Json(json!({ "code": code, "detail": detail })),
            )
                .into_response(),
            EditError::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            EditError::Internal(detail) => {
                tracing::error!(detail = %detail, "editing request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{build_router, AppConfig, AppState};
    use axum::body::Body;
    use http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    const BASE: &str = concat!(
        "/api/v1/organizations/00000000-0000-0000-0000-000000000001",
        "/projects/00000000-0000-0000-0000-000000000002"
    );

    async fn call(method: &str, path: &str, body: &str) -> (StatusCode, Value) {
        let app = build_router(AppState::new(AppConfig::default(), None).unwrap());
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("{BASE}{path}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn malformed_json_precedes_authentication_like_fastapi() {
        let source = "/sources/00000000-0000-0000-0000-000000000003/changesets";
        let (status, body) = call("POST", source, "{").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["detail"][0]["msg"], "invalid request payload");

        for (method, path) in [
            ("POST", source),
            ("GET", source),
            ("GET", "/changesets/00000000-0000-0000-0000-000000000004"),
            (
                "POST",
                "/changesets/00000000-0000-0000-0000-000000000004/revert",
            ),
            ("POST", "/editing/snap"),
        ] {
            let (status, body) = call(method, path, "{}").await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            assert_eq!(body["detail"], "authentication required");
        }
    }
}
