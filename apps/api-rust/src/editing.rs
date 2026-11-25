//! E03 editing routes (R006): changeset commit/list/detail/revert and snapping.
//!
//! FastAPI order is preserved: malformed JSON (422) -> session (401) ->
//! `project_access_guard` (404/403) -> CSRF (403) -> body/query validation (422)
//! -> project scope and permission -> domain.

use axum::body::to_bytes;
use axum::extract::{FromRequestParts, Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::editing::domain::py_strip;
use ovg_storage::editing::{snap, CommitRequest, EditError, EditScope, Guard, Permission};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::{HumanSession, IdentityError};
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
        .route(&format!("{PROJECT}/editing/snap"), post(compute_snap))
}

type Ids = Path<(String, String, String)>;

async fn commit_changeset(
    State(state): State<AppState>,
    Path((organization, project, source)): Ids,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (session, headers, body) = authenticate_json(&state, request).await?;
    let scope = open(&state, &session, &organization, &project, Guard::ScopedAllowed).await?;
    require_csrf(&state, &session, &headers)?;
    let body: CommitRequest = decode(body)?;
    let schema_ok = body.base_schema_revision.as_deref().is_none_or(|s| {
        s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    });
    if !schema_ok || body.summary.chars().count() > 500 || reserved(&body.operation_id) {
        return Err(EditError::InvalidRequest.into());
    }
    let (Some(mut scope), Ok(source_id)) = (scope, Uuid::parse_str(&source)) else {
        return Err(EditError::InvalidRequest.into());
    };
    let result = scope.commit_changeset(source_id, body).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_changesets(
    session: HumanSession,
    State(state): State<AppState>,
    Path((organization, project, source)): Ids,
    uri: Uri,
) -> Result<Json<Value>, Failure> {
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
    session: HumanSession,
    State(state): State<AppState>,
    Path((organization, project, changeset)): Ids,
) -> Result<Json<Value>, Failure> {
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
    let (session, headers, body) = authenticate_json(&state, request).await?;
    let scope = open(&state, &session, &organization, &project, Guard::ScopedDenied).await?;
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
    target: (f64, f64),
    #[serde(default)]
    vertices: Vec<(f64, f64)>,
    #[serde(default)]
    segments: Vec<((f64, f64), (f64, f64))>,
    tolerance: f64,
}

async fn compute_snap(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (session, headers, body) = authenticate_json(&state, request).await?;
    let scope = open(&state, &session, &organization, &project, Guard::ScopedAllowed).await?;
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

/// FastAPI decodes a JSON body before dependencies run, then authenticates.
async fn authenticate_json(
    state: &AppState,
    request: Request,
) -> Result<(HumanSession, HeaderMap, Option<Value>), Failure> {
    let (mut parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| EditError::InvalidRequest)?;
    let json = if bytes.is_empty() || !json_content_type(&parts.headers) {
        None
    } else {
        Some(serde_json::from_slice(&bytes).map_err(|_| EditError::InvalidRequest)?)
    };
    let session = HumanSession::from_request_parts(&mut parts, state)
        .await
        .map_err(|error| Failure::Identity(Box::new(error)))?;
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
    let mime = value.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    mime.split_once('/').is_some_and(|(main, sub)| {
        main == "application" && (sub == "json" || sub.ends_with("+json"))
    })
}

/// Project guard; skipped (like the oracle) when the path IDs are not UUIDs.
async fn open<'a>(
    state: &'a AppState,
    session: &HumanSession,
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
    let scope = EditScope::begin(
        pool,
        organization_id,
        project_id,
        session.actor_id(),
        session.session_id(),
        guard,
    )
    .await?;
    Ok(Some(scope))
}

fn require_csrf(
    state: &AppState,
    session: &HumanSession,
    headers: &HeaderMap,
) -> Result<(), Failure> {
    let candidate = headers
        .get("x-openvietgis-csrf")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let valid = !candidate.is_empty()
        && state
            .config
            .csrf_secret
            .as_deref()
            .is_some_and(|secret| session.csrf_matches(candidate, secret));
    if valid {
        Ok(())
    } else {
        Err(EditError::detail(403, "CSRF validation failed").into())
    }
}

fn decode<T: DeserializeOwned>(body: Option<Value>) -> Result<T, Failure> {
    body.and_then(|value| serde_json::from_value(value).ok())
        .ok_or(Failure::Edit(EditError::InvalidRequest))
}

/// Field-sync recovery operation IDs belong only to the internal repository.
fn reserved(operation_id: &str) -> bool {
    py_strip(operation_id).starts_with("field-sync-")
}

fn query_limit(uri: &Uri) -> Result<i64, Failure> {
    let Query(pairs) = Query::<Vec<(String, String)>>::try_from_uri(uri)
        .map_err(|_| EditError::InvalidRequest)?;
    match pairs.into_iter().rev().find(|(key, _)| key == "limit") {
        None => Ok(50),
        Some((_, value)) => value
            .parse::<i64>()
            .ok()
            .filter(|limit| (1..=200).contains(limit))
            .ok_or(Failure::Edit(EditError::InvalidRequest)),
    }
}

enum Failure {
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
            EditError::Spatial(status, code, detail) => {
                (http(status), Json(json!({ "code": code, "detail": detail }))).into_response()
            }
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
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
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
            ("POST", "/changesets/00000000-0000-0000-0000-000000000004/revert"),
            ("POST", "/editing/snap"),
        ] {
            let (status, body) = call(method, path, "{}").await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            assert_eq!(body["detail"], "authentication required");
        }
    }
}
