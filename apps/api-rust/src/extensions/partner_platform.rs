//! E14 persistent partner contributions and project-scoped integration event journal routes.

use axum::extract::{Path, Query, Request, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::extensions::partner::{
    list_contributions as store_list_contributions, list_events as store_list_events,
    save_contributions as store_save_contributions,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

use super::{
    authenticate_actor, authenticate_json, decode_body, open_project_scope, parse_uuid,
    require_actor_csrf, ExtensionError,
};

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/extensions/partner-contributions"),
            get(list_contributions),
        )
        .route(
            &format!("{BASE}/extensions/partner-contributions/{{manifest_id}}"),
            put(save_contributions),
        )
        .route(&format!("{BASE}/integrations/events"), get(list_events))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveContributionsBody {
    expected_revision: i32,
    document: Value,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    after: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
}

async fn list_contributions(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let mut read = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let items = store_list_contributions(&mut read)
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "items": items })),
    )
        .into_response())
}

async fn save_contributions(
    State(state): State<AppState>,
    Path((org, project, manifest)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let manifest_id = parse_uuid(&manifest)?;
    let body: SaveContributionsBody = decode_body(body)?;
    if !(0..=2_147_483_646).contains(&body.expected_revision) || !body.document.is_object() {
        return Err(ExtensionError::InvalidRequest);
    }
    if !actor.is_human() {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "human project manager required",
        )));
    }
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let result = store_save_contributions(
        &mut edit,
        manifest_id,
        body.expected_revision,
        &body.document,
    )
    .await
    .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(result))
}

async fn list_events(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let Query(query) = Query::<EventsQuery>::try_from_uri(&parts.uri)
        .map_err(|_| ExtensionError::InvalidRequest)?;
    if query.after < 0 || !(1..=200).contains(&query.limit) {
        return Err(ExtensionError::InvalidRequest);
    }
    let mut read = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = store_list_events(&mut read, query.after, query.limit)
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
