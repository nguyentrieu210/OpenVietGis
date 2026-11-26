//! Reconciliation, protected household data, search, import history, document
//! history and stored document/export downloads.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_module_house_numbering::domain::{
    classify_reconciliation, match_certificate, EvidenceCandidate,
};
use ovg_storage::editing::authenticate_service;
use ovg_storage::editing::domain::py_strip;
use ovg_storage::house_numbering as store;
use ovg_storage::integration_hub::{self, EffectiveProjectLevel};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::hierarchy::query_int;
use super::{
    audit_actor, csrf, decode, module_edit, module_manage, module_read, module_service_read,
    opt_text_len, path_id, scope_ids, service_auth_error, text_len, HnResult, HouseNumberingError,
    BASE,
};
use crate::catalog::CatalogError;
use crate::identity::HumanSession;
use crate::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/imports"), get(list_imports))
        .route(
            &format!("{BASE}/buildings/{{building_id}}/documents"),
            get(document_history),
        )
        .route(
            &format!("{BASE}/documents/{{document_id}}/download"),
            get(download_document),
        )
        .route(
            &format!("{BASE}/exports/{{artifact_id}}/download"),
            get(download_export),
        )
        .route(
            &format!("{BASE}/reconciliation/rebuild"),
            post(rebuild_reconciliation),
        )
        .route(&format!("{BASE}/reconciliation/cases"), get(list_cases))
        .route(
            &format!("{BASE}/reconciliation/evidence/{{evidence_id}}/suggest"),
            get(suggest_evidence),
        )
        .route(
            &format!("{BASE}/reconciliation/cases/{{case_id}}/resolve"),
            post(resolve_case),
        )
        .route(&format!("{BASE}/private-households"), post(upsert_private))
        .route(
            &format!("{BASE}/buildings/{{building_id}}/private-households"),
            get(read_private),
        )
        .route(
            &format!("{BASE}/geometry-reviews"),
            get(list_geometry_reviews),
        )
        .route(&format!("{BASE}/search"), get(search))
        .route(
            &format!("{BASE}/integrations/v1/addresses"),
            get(integration_addresses),
        )
}

async fn integration_addresses(
    State(state): State<AppState>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
    request: Request,
) -> HnResult<Response> {
    let (organization_id, project_id) = scope_ids(&params)?;
    let q = query
        .get("query")
        .map(|value| value.trim())
        .filter(|value| text_len(value, 2, 120))
        .ok_or(HouseNumberingError::InvalidRequest)?;
    let limit = query_int(&query, "limit", 50, 1, 100)?;

    let (mut parts, _body) = request.into_parts();
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

    let mut read = if let Some(raw) = bearer {
        let pool = state.db_pool.as_ref().ok_or_else(|| {
            HouseNumberingError::Domain(ovg_core::AppError::service_unavailable(
                "DATABASE_UNAVAILABLE",
                "database unavailable",
            ))
        })?;
        let auth = authenticate_service(pool, organization_id, raw)
            .await
            .map_err(service_auth_error)?;
        module_service_read(&state, &auth, organization_id, project_id).await?
    } else {
        let auth = HumanSession::from_request_parts(&mut parts, &state)
            .await
            .map_err(|error| HouseNumberingError::Access(CatalogError::Identity(error)))?;
        module_read(&state, &auth, organization_id, project_id).await?
    };

    let result = store::integration_addresses(&mut read, q, limit).await?;
    read.commit().await?;
    let mut response = Json(result).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn list_imports(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    // Python's importer prepares edit access even for listing batches.
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::list_imports(&mut edit).await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn document_history(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::document_history(&mut read, building_id).await?;
    read.commit().await?;
    Ok(Json(result))
}

fn attachment(artifact: store::StoredArtifact) -> Response {
    // Starlette appends a UTF-8 charset to text media types.
    let content_type = if artifact.content_type.starts_with("text/")
        && !artifact.content_type.contains("charset")
    {
        format!("{}; charset=utf-8", artifact.content_type)
    } else {
        artifact.content_type.clone()
    };
    let disposition = format!("attachment; filename=\"{}\"", artifact.filename);
    let mut response = (StatusCode::OK, artifact.data).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

async fn download_document(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Response> {
    let (org, project) = scope_ids(&params)?;
    let document_id = path_id(&params, "document_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let artifact = store::download_document(
        &mut read,
        audit_actor(&auth),
        &state.config.artifact_store_dir,
        document_id,
    )
    .await?;
    read.commit().await?;
    Ok(attachment(artifact))
}

async fn download_export(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Response> {
    let (org, project) = scope_ids(&params)?;
    let artifact_id = path_id(&params, "artifact_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let can_manage = integration_hub::catalog_context(&mut read, auth.actor_id())
        .await?
        .level
        == EffectiveProjectLevel::Manage;
    let (artifact, _) = store::download_export(
        &mut read,
        audit_actor(&auth),
        &state.config.artifact_store_dir,
        artifact_id,
        can_manage,
    )
    .await?;
    read.commit().await?;
    Ok(attachment(artifact))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildBody {
    #[serde(default)]
    removed_building_ids: Vec<Uuid>,
    #[serde(default)]
    estimated_building_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveBody {
    expected_revision: i64,
    status: String,
    resolution: String,
    #[serde(default)]
    building_id: Option<Uuid>,
}

async fn rebuild_reconciliation(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: RebuildBody = decode(&body)?;
    if body.removed_building_ids.len() > 5000 || body.estimated_building_ids.len() > 5000 {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::rebuild_reconciliation(
        &mut edit,
        audit_actor(&auth),
        &body.removed_building_ids,
        &body.estimated_building_ids,
        |certificate, cad, survey, surveyed, exists| {
            classify_reconciliation(certificate, cad, survey, surveyed, exists)
                .map_err(|error| error.0)
        },
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn list_cases(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_cases(
        &mut read,
        query.get("status").map(String::as_str),
        query.get("case_class").map(String::as_str),
    )
    .await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn suggest_evidence(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let evidence_id = path_id(&params, "evidence_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let scope = store::certificate_scope(&mut read, evidence_id).await?;
    read.commit().await?;
    let Some(route_id) = scope.route_id.map(|id| id.to_string()) else {
        return Ok(Json(
            json!({"building_id": null, "reason": "no_route_scope"}),
        ));
    };
    let candidates: Vec<EvidenceCandidate> = scope
        .candidates
        .into_iter()
        .map(|(building, old_number, new_number)| EvidenceCandidate {
            building_id: building.to_string(),
            route_id: route_id.clone(),
            old_number,
            new_number,
        })
        .collect();
    let raw = scope.raw_number.unwrap_or_default();
    let raw = raw.trim();
    let raw = (!raw.is_empty()).then_some(raw);
    let (building_id, reason) = match_certificate(&candidates, &route_id, raw, raw)
        .map_err(|error| ovg_core::AppError::internal(error.0))?;
    Ok(Json(json!({"building_id": building_id, "reason": reason})))
}

async fn resolve_case(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let case_id = path_id(&params, "case_id")?;
    let body: ResolveBody = decode(&body)?;
    if body.expected_revision <= 0
        || !matches!(body.status.as_str(), "resolved" | "ignored")
        || !text_len(&body.resolution, 1, 1000)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::resolve_case(
        &mut edit,
        audit_actor(&auth),
        case_id,
        body.expected_revision,
        &body.status,
        &body.resolution,
        body.building_id,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateBody {
    building_id: Uuid,
    purpose: String,
    #[serde(default)]
    consent: bool,
    #[serde(default)]
    lawful_basis_ref: Option<String>,
    protected_data: serde_json::Map<String, Value>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    record_id: Option<Uuid>,
    #[serde(default)]
    expected_revision: Option<i64>,
}

/// Pydantic `datetime` input: ISO 8601 with or without offset.
fn valid_datetime(value: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(value).is_ok()
        || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").is_ok()
        || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f").is_ok()
}

async fn upsert_private(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: PrivateBody = decode(&body)?;
    if !text_len(&body.purpose, 1, 500)
        || !opt_text_len(body.lawful_basis_ref.as_deref(), 500)
        || body
            .expires_at
            .as_deref()
            .is_some_and(|value| !valid_datetime(value))
        || body.expected_revision.is_some_and(|value| value <= 0)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let protected = Value::Object(body.protected_data);
    let mut edit = module_manage(&state, &auth, org, project).await?;
    let result = store::upsert_private(
        &mut edit,
        audit_actor(&auth),
        store::PrivateInput {
            building_id: body.building_id,
            purpose: &body.purpose,
            consent: body.consent,
            lawful_basis_ref: body.lawful_basis_ref.as_deref(),
            protected_data: &protected,
            expires_at: body.expires_at.as_deref(),
            record_id: body.record_id,
            expected_revision: body.expected_revision,
        },
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn read_private(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let purpose = query
        .get("purpose")
        .filter(|value| text_len(value, 1, 500))
        .ok_or(HouseNumberingError::InvalidRequest)?;
    let mut edit = module_manage(&state, &auth, org, project).await?;
    let result = store::read_private(&mut edit, audit_actor(&auth), building_id, purpose).await?;
    edit.commit().await?;
    Ok(Json(result))
}

fn query_float(query: &HashMap<String, String>, key: &str) -> HnResult<Option<f64>> {
    query
        .get(key)
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .map_err(|_| HouseNumberingError::InvalidRequest)
        })
        .transpose()
}

async fn search(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let x = query_float(&query, "x")?;
    let y = query_float(&query, "y")?;
    let crs_code = query
        .get("crs_code")
        .map(|value| {
            value
                .trim()
                .parse::<i32>()
                .map_err(|_| HouseNumberingError::InvalidRequest)
        })
        .transpose()?;
    let max_distance = query_float(&query, "max_distance")?.unwrap_or(100.0);
    if !(max_distance > 0.0 && max_distance <= 10_000.0) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let limit = query_int(&query, "limit", 100, 1, 500)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::search(
        &mut read,
        store::SearchInput {
            query: query.get("query").map(String::as_str),
            x,
            y,
            crs_code,
            max_distance,
            limit,
        },
    )
    .await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn list_geometry_reviews(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let offset = query_int(&query, "offset", 0, i64::MIN, i64::MAX)?;
    let limit = query_int(&query, "limit", 100, i64::MIN, i64::MAX)?;
    // The route checks the page window before opening the module scope.
    if offset < 0 || !(1..=100).contains(&limit) {
        return Err(ovg_core::AppError::validation(
            "INVALID_GEOMETRY_REVIEW",
            "Lô hình học không hợp lệ",
        )
        .into());
    }
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_geometry_reviews(&mut read, offset, limit).await?;
    read.commit().await?;
    Ok(Json(result))
}
