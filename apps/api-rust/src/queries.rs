//! E04 attribute & spatial query routes matching the FastAPI R007 wire.

use axum::body::Bytes;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::queries::{self, QueryRequest, SavedQueryInput};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::identity::HumanSession;
use crate::AppState;

pub(crate) fn router() -> Router<AppState> {
    const PROJECT: &str = "/organizations/{organization_id}/projects/{project_id}";
    Router::new()
        .route(
            &format!("{PROJECT}/layers/{{layer_id}}/query"),
            post(execute_query),
        )
        .route(
            &format!("{PROJECT}/saved-queries"),
            get(list_saved_queries).post(create_saved_query),
        )
        .route(
            &format!("{PROJECT}/saved-queries/{{query_id}}"),
            delete(delete_saved_query),
        )
}

/// Pydantic `ExecuteQueryRequest` (extra=forbid, strict bounded ints).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteBody {
    #[serde(default)]
    attribute_filter: Option<Map<String, Value>>,
    #[serde(default)]
    spatial_filter: Option<Map<String, Value>>,
    #[serde(default)]
    expected_content_revision: Option<String>,
    #[serde(default)]
    expected_schema_revision: Option<String>,
    #[serde(default)]
    expected_query_fingerprint: Option<String>,
    #[serde(default)]
    sort_field: Option<String>,
    #[serde(default = "ascending")]
    sort_direction: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

/// Pydantic `CreateSavedQueryRequest` (extra=forbid).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBody {
    layer_id: Uuid,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    attribute_filter: Option<Map<String, Value>>,
    #[serde(default)]
    spatial_filter: Option<Map<String, Value>>,
    #[serde(default)]
    sort_field: Option<String>,
    #[serde(default = "ascending")]
    sort_direction: String,
}

#[derive(Deserialize)]
struct ListQuery {
    layer_id: Option<Uuid>,
}

fn ascending() -> String {
    "asc".to_owned()
}

const fn default_limit() -> i64 {
    100
}

async fn execute_query(
    auth: HumanSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
    body: Bytes,
) -> Result<Json<Value>, QueryError> {
    require_csrf(&auth, &state, &headers)?;
    let Path((organization_id, project_id, layer_id)) =
        path.map_err(|_| QueryError::InvalidRequest)?;
    let body: ExecuteBody = parse(&body)?;
    if !(1..=500).contains(&body.limit) || !(0..=25_000).contains(&body.offset) {
        return Err(QueryError::InvalidRequest);
    }
    let request = QueryRequest {
        attribute_filter: body.attribute_filter.map(Value::Object),
        spatial_filter: body.spatial_filter.map(Value::Object),
        expected_content_revision: body.expected_content_revision,
        expected_schema_revision: body.expected_schema_revision,
        expected_query_fingerprint: body.expected_query_fingerprint,
        sort_field: body.sort_field,
        sort_direction: body.sort_direction,
        limit: body.limit,
        offset: body.offset,
    };
    let mut read = scope(&state, &auth, organization_id, project_id, false).await?;
    let result = queries::execute_query(&mut read, auth.actor_id(), layer_id, &request)
        .await
        .map_err(QueryError::Domain)?;
    read.commit().await.map_err(QueryError::Domain)?;
    Ok(Json(result))
}

async fn create_saved_query(
    auth: HumanSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), QueryError> {
    require_csrf(&auth, &state, &headers)?;
    let Path((organization_id, project_id)) = path.map_err(|_| QueryError::InvalidRequest)?;
    let body: CreateBody = parse(&body)?;
    let input = SavedQueryInput {
        layer_id: body.layer_id,
        name: body.name,
        description: body.description,
        attribute_filter: body.attribute_filter.map(Value::Object),
        spatial_filter: body.spatial_filter.map(Value::Object),
        sort_field: body.sort_field,
        sort_direction: body.sort_direction,
    };
    let mut write = scope(&state, &auth, organization_id, project_id, true).await?;
    let result = queries::create_saved_query(&mut write, auth.actor_id(), &input)
        .await
        .map_err(QueryError::Domain)?;
    write.commit().await.map_err(QueryError::Domain)?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_saved_queries(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Value>, QueryError> {
    let Path((organization_id, project_id)) = path.map_err(|_| QueryError::InvalidRequest)?;
    let Query(query) = query.map_err(|_| QueryError::InvalidRequest)?;
    let mut read = scope(&state, &auth, organization_id, project_id, false).await?;
    let result = queries::list_saved_queries(&mut read, query.layer_id)
        .await
        .map_err(QueryError::Domain)?;
    read.commit().await.map_err(QueryError::Domain)?;
    Ok(Json(result))
}

async fn delete_saved_query(
    auth: HumanSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<StatusCode, QueryError> {
    require_csrf(&auth, &state, &headers)?;
    let Path((organization_id, project_id, query_id)) =
        path.map_err(|_| QueryError::InvalidRequest)?;
    let mut write = scope(&state, &auth, organization_id, project_id, true).await?;
    queries::delete_saved_query(&mut write, query_id)
        .await
        .map_err(QueryError::Domain)?;
    write.commit().await.map_err(QueryError::Domain)?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, QueryError> {
    // Serde structs may also deserialize from JSON arrays. The public contract
    // accepts only an object, including when every field has a default.
    if body.iter().copied().find(|byte| !byte.is_ascii_whitespace()) != Some(b'{') {
        return Err(QueryError::InvalidRequest);
    }
    serde_json::from_slice(body).map_err(|_| QueryError::InvalidRequest)
}

fn require_csrf(
    auth: &HumanSession,
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), QueryError> {
    let candidate = headers
        .get("x-openvietgis-csrf")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let valid = !candidate.is_empty()
        && state
            .config
            .csrf_secret
            .as_deref()
            .is_some_and(|secret| auth.csrf_matches(secret, candidate));
    if valid {
        Ok(())
    } else {
        Err(QueryError::Csrf)
    }
}

/// Project authorization in the request transaction. Queries are reads
/// (project.read, FOR SHARE); saved-query mutations need project.edit.
async fn scope<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
    edit: bool,
) -> Result<ProjectRead<'a>, QueryError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        QueryError::Access(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })?;
    let (actor, session) = (auth.actor_id(), auth.session_id());
    let read = if edit {
        ProjectRead::begin_edit(pool, organization_id, project_id, actor, session).await
    } else {
        ProjectRead::begin(
            pool,
            organization_id,
            project_id,
            actor,
            session,
            ProjectReadPolicy::Unrestricted,
        )
        .await
    };
    read.map_err(QueryError::Access)
}

#[derive(Debug)]
enum QueryError {
    InvalidRequest,
    Csrf,
    /// Tenant/project guard failures: FastAPI `{"detail": ...}` envelope.
    Access(AppError),
    /// `PlatformDomainError`: `{"code", "message"}` envelope.
    Domain(AppError),
}

impl IntoResponse for QueryError {
    fn into_response(self) -> Response {
        let (access, error) = match self {
            Self::InvalidRequest => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                    })),
                )
                    .into_response();
            }
            Self::Csrf => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"detail": "CSRF validation failed"})),
                )
                    .into_response();
            }
            Self::Access(error) => (true, error),
            Self::Domain(error) => (false, error),
        };
        if let AppError::Internal { internal_detail } = &error {
            tracing::error!(detail = %internal_detail, "query request failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response();
        }
        let status =
            StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let message = error.to_string();
        let body = match (access, error.code()) {
            (true, "PROJECT_NOT_WRITABLE") => {
                json!({"code": "PROJECT_NOT_WRITABLE", "detail": message})
            }
            (true, _) => json!({"detail": message}),
            (false, code) => json!({"code": code, "message": message}),
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_matches_pydantic_strictness() {
        let ok: ExecuteBody = parse(br#"{"attribute_filter": null, "limit": 5}"#).unwrap();
        assert_eq!(
            (ok.limit, ok.offset, ok.sort_direction.as_str()),
            (5, 0, "asc")
        );
        for bad in [
            &br#"{"limit": 5.0}"#[..],
            br#"{"limit": true}"#,
            br#"{"limit": "5"}"#,
            br#"{"unknown": 1}"#,
            br#"{"attribute_filter": [1]}"#,
            br#"{"sort_direction": null}"#,
            br#"[]"#,
            br#"[null, null, null, null, null, null, "asc", 5, 0]"#,
            br#"null"#,
        ] {
            assert!(parse::<ExecuteBody>(bad).is_err());
        }
        assert!(parse::<CreateBody>(br#"{"name": "x"}"#).is_err());
        assert!(parse::<CreateBody>(br#"{"layer_id": "nope", "name": "x"}"#).is_err());
    }

    #[test]
    fn errors_use_fastapi_envelopes() {
        let response =
            QueryError::Domain(AppError::validation("INVALID_FILTER", "bad")).into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = QueryError::Access(AppError::not_found("project not found")).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            QueryError::Csrf.into_response().status(),
            StatusCode::FORBIDDEN
        );
    }
}
