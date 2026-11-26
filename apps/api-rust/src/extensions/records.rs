//! E14 extension data records, record history, and portable bundle export/import routes.

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::extensions::records::{
    create_record as store_create_record, export_bundle as store_export_bundle,
    import_bundle as store_import_bundle, list_records as store_list_records,
    record_history as store_record_history, update_record as store_update_record,
    CreateRecordInput, UpdateRecordInput,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;

use super::{
    authenticate_actor, authenticate_json, decode_body, open_project_scope, parse_uuid,
    project_actor_display, require_actor_csrf, ExtensionError,
};

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/extensions";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/{{extension_key}}/records"),
            get(list_records).post(create_record),
        )
        .route(
            &format!("{BASE}/{{extension_key}}/records/{{record_id}}"),
            axum::routing::put(update_record),
        )
        .route(
            &format!("{BASE}/{{extension_key}}/records/{{record_id}}/history"),
            get(list_record_history),
        )
        .route(
            &format!("{BASE}/{{extension_key}}/portable-export"),
            get(export_bundle),
        )
        .route(
            &format!("{BASE}/{{extension_key}}/portable-import"),
            post(import_bundle),
        )
}

#[derive(Debug, Deserialize)]
struct ListRecordsQuery {
    entity_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRecordBody {
    entity_name: String,
    payload: Value,
    #[serde(default)]
    linked_layer_id: Option<Uuid>,
    #[serde(default)]
    linked_feature_id: Option<String>,
    #[serde(default)]
    expected_link_context: Option<Value>,
    #[serde(default)]
    operation_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRecordBody {
    expected_revision: i32,
    #[serde(default)]
    expected_manifest_id: Option<Uuid>,
    payload: Value,
    #[serde(default)]
    expected_link_context: Option<Value>,
    #[serde(default)]
    operation_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableImportBody {
    expected_manifest_id: Uuid,
    bundle: Value,
    sha256: String,
}

async fn list_records(
    State(state): State<AppState>,
    Path((org, project, extension_key)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
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
    let Query(query) = Query::<ListRecordsQuery>::try_from_uri(&parts.uri)
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let items = store_list_records(&mut read, &extension_key, query.entity_name.as_deref())
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(json!({ "items": items })))
}

async fn create_record(
    State(state): State<AppState>,
    Path((org, project, extension_key)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<(StatusCode, Json<Value>), ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: CreateRecordBody = decode_body(body)?;
    if !body.payload.is_object()
        || body
            .expected_link_context
            .as_ref()
            .is_some_and(|v| !v.is_object())
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let display = project_actor_display(&edit, &actor);
    let rec = store_create_record(
        &mut edit,
        actor.actor_id(),
        &display,
        &extension_key,
        CreateRecordInput {
            entity_name: &body.entity_name,
            payload: &body.payload,
            linked_layer_id: body.linked_layer_id,
            linked_feature_id: body.linked_feature_id.as_deref(),
            expected_link_context: body.expected_link_context.as_ref(),
            operation_id: body.operation_id,
        },
    )
    .await
    .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok((StatusCode::CREATED, Json(rec)))
}

async fn update_record(
    State(state): State<AppState>,
    Path((org, project, extension_key, record)): Path<(String, String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let record_id = parse_uuid(&record)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: UpdateRecordBody = decode_body(body)?;
    if !(1..=2_147_483_646).contains(&body.expected_revision)
        || !body.payload.is_object()
        || body
            .expected_link_context
            .as_ref()
            .is_some_and(|v| !v.is_object())
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let display = project_actor_display(&edit, &actor);
    let rec = store_update_record(
        &mut edit,
        actor.actor_id(),
        &display,
        &extension_key,
        UpdateRecordInput {
            record_id,
            expected_revision: body.expected_revision,
            expected_manifest_id: body.expected_manifest_id,
            payload: &body.payload,
            expected_link_context: body.expected_link_context.as_ref(),
            operation_id: body.operation_id,
        },
    )
    .await
    .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(rec))
}

async fn list_record_history(
    State(state): State<AppState>,
    Path((org, project, extension_key, record)): Path<(String, String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let record_id = parse_uuid(&record)?;
    let mut read = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let items = store_record_history(&mut read, &extension_key, record_id)
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(json!({ "items": items })))
}

async fn export_bundle(
    State(state): State<AppState>,
    Path((org, project, extension_key)): Path<(String, String, String)>,
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
    let result = store_export_bundle(&mut read, &extension_key)
        .await
        .map_err(ExtensionError::Domain)?;
    read.commit().await.map_err(ExtensionError::Domain)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}

async fn import_bundle(
    State(state): State<AppState>,
    Path((org, project, extension_key)): Path<(String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<Json<Value>, ExtensionError> {
    let (actor, headers, body) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let project_id = parse_uuid(&project)?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        org_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: PortableImportBody = decode_body(body)?;
    let valid_sha = body.sha256.len() == 64
        && body
            .sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if !valid_sha || !body.bundle.is_object() {
        return Err(ExtensionError::InvalidRequest);
    }
    let display = project_actor_display(&edit, &actor);
    let result = store_import_bundle(
        &mut edit,
        actor.actor_id(),
        &display,
        &extension_key,
        body.expected_manifest_id,
        &body.bundle,
        &body.sha256,
    )
    .await
    .map_err(ExtensionError::Domain)?;
    edit.commit().await.map_err(ExtensionError::Domain)?;
    Ok(Json(result))
}
