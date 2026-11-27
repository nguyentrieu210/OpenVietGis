//! Shared FastAPI-parity actor auth, body-first JSON parsing, CSRF, and
//! `EditScope` transaction opening for E08 routes.

use axum::body::to_bytes;
use axum::extract::{FromRequestParts, Request};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use ovg_storage::editing::domain::py_strip;
use ovg_storage::editing::{authenticate_service, EditError, EditScope, Guard, ServiceAuth};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::{self, HumanSession, IdentityError};
use crate::AppState;

const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

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
                tracing::error!(detail = %detail, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}
