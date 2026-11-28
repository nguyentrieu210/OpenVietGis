//! Field survey, assignment, progress and survey sign-off routes.

use super::module_approve;
use super::module_survey;
use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use ovg_module_house_numbering_storage as store;
use ovg_storage::integration_hub::{self, EffectiveProjectLevel};
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::{
    audit_actor, csrf, decode, module_manage, module_read, path_id, scope_ids,
    HnResult, HouseNumberingError, BASE,
};
use ovg_module_http::ModuleSession as HumanSession;
use ovg_module_http::ModuleHttpState as AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/assignments"),
            get(list_assignments).post(create_assignment),
        )
        .route(
            &format!("{BASE}/assignments/{{assignment_id}}"),
            patch(reassign_assignment),
        )
        .route(&format!("{BASE}/surveys"), get(list_surveys))
        .route(
            &format!("{BASE}/surveys/{{survey_id}}/review"),
            post(review_survey),
        )
        .route(&format!("{BASE}/survey/sync"), post(sync_surveys))
        .route(
            &format!("{BASE}/survey/photos"),
            post(upload_photo).layer(DefaultBodyLimit::max(PHOTO_BODY_LIMIT)),
        )
        .route(&format!("{BASE}/progress"), get(progress))
        .route(
            &format!("{BASE}/survey-signoffs"),
            get(list_signoffs).post(sign_scope),
        )
        .route(
            &format!("{BASE}/survey-signoffs/{{signoff_id}}/revoke"),
            post(revoke_signoff),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentBody {
    scope_kind: String,
    scope_id: Uuid,
    assignee_actor_id: Uuid,
    starts_on: chrono::NaiveDate,
    #[serde(default)]
    ends_on: Option<chrono::NaiveDate>,
    #[serde(default)]
    target_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReassignBody {
    expected_revision: i64,
    assignee_actor_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewBody {
    expected_revision: i64,
    decision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SyncBody {
    operations: Vec<Map<String, Value>>,
    receipt_version: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignBody {
    source_ids: Vec<Uuid>,
    ap: String,
    #[serde(default)]
    ten_duong: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokeBody {
    reason: String,
}

/// Pydantic `str_strip_whitespace` then a length check.
fn stripped(value: &str, min: usize, max: usize) -> HnResult<&str> {
    let value = value.trim();
    let count = value.chars().count();
    if count < min || count > max {
        return Err(HouseNumberingError::InvalidRequest);
    }
    Ok(value)
}

async fn create_assignment(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: AssignmentBody = decode(&body)?;
    let scope_kind = match body.scope_kind.as_str() {
        "area" => "area",
        "route" => "route",
        _ => return Err(HouseNumberingError::InvalidRequest),
    };
    let target_count = match body.target_count {
        None => None,
        Some(value) if value >= 0 => Some(i32::try_from(value).unwrap_or(i32::MAX)),
        Some(_) => return Err(HouseNumberingError::InvalidRequest),
    };
    let mut edit = module_manage(&state, &auth, org, project).await?;
    let result = store::create_assignment(
        &mut edit,
        audit_actor(&auth),
        store::AssignmentInput {
            scope_kind,
            scope_id: body.scope_id,
            assignee_actor_id: body.assignee_actor_id,
            starts_on: body.starts_on,
            ends_on: body.ends_on,
            target_count,
        },
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn reassign_assignment(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let assignment_id = path_id(&params, "assignment_id")?;
    let body: ReassignBody = decode(&body)?;
    if body.expected_revision <= 0 {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_manage(&state, &auth, org, project).await?;
    let result = store::reassign_assignment(
        &mut edit,
        audit_actor(&auth),
        assignment_id,
        body.expected_revision,
        body.assignee_actor_id,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn list_assignments(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_assignments(&mut read).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn list_surveys(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_surveys(&mut read, query.get("state").map(String::as_str)).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn review_survey(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let survey_id = path_id(&params, "survey_id")?;
    let body: ReviewBody = decode(&body)?;
    if body.expected_revision <= 0 || !matches!(body.decision.as_str(), "accepted" | "rejected") {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_approve(&state, &auth, org, project).await?;
    let result = store::review_survey(
        &mut edit,
        audit_actor(&auth),
        survey_id,
        body.expected_revision,
        &body.decision,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn sync_surveys(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: SyncBody = decode(&body)?;
    if !(1..=500).contains(&body.operations.len()) || !matches!(body.receipt_version,None|Some(2)) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_survey(&state, &auth, org, project).await?;
    let can_manage = integration_hub::catalog_context(&mut edit, auth.actor_id())
        .await?
        .level
        == EffectiveProjectLevel::Manage;
    let can_manage = can_manage && super::has_module_permission(&mut edit, "ext.house_numbering.manage").await?;
    let operations: Vec<Value> = body.operations.into_iter().map(Value::Object).collect();
    let result = store::sync_surveys(
        &mut edit,
        audit_actor(&auth),
        can_manage,
        &state.artifact_store_dir,
        &operations,
        body.receipt_version,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn progress(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::progress(&mut read).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn list_signoffs(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let source_id = path_id(&query, "source_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_signoffs(&mut read, source_id).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn sign_scope(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: SignBody = decode(&body)?;
    if !(1..=20).contains(&body.source_ids.len()) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let ap = stripped(&body.ap, 1, 200)?;
    let ten_duong = match body.ten_duong.as_deref() {
        Some(value) => Some(stripped(value, 1, 300)?),
        None => None,
    };
    let note = match body.note.as_deref() {
        Some(value) => Some(stripped(value, 0, 1000)?),
        None => None,
    };
    let mut edit = module_approve(&state, &auth, org, project).await?;
    let result = store::sign_scope(
        &mut edit,
        audit_actor(&auth),
        &body.source_ids,
        ap,
        ten_duong,
        note,
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn revoke_signoff(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let signoff_id = path_id(&params, "signoff_id")?;
    let body: RevokeBody = decode(&body)?;
    let reason = stripped(&body.reason, 3, 1000)?;
    let mut edit = module_approve(&state, &auth, org, project).await?;
    let result = store::revoke_signoff(&mut edit, audit_actor(&auth), signoff_id, reason).await?;
    edit.commit().await?;
    Ok(Json(result))
}

/// Room for the 15 MiB photo limit plus multipart framing.
const PHOTO_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// Minimal `multipart/form-data` reader: `(content_type, bytes)` of the `file` part.
fn multipart_file(headers: &HeaderMap, body: &[u8]) -> Option<(String, Vec<u8>)> {
    let content_type = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let boundary = content_type.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| value.trim().trim_matches('"').to_owned())
    })?;
    let delimiter = format!("--{boundary}").into_bytes();
    let find = |haystack: &[u8], needle: &[u8], from: usize| {
        haystack
            .get(from..)?
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|index| index + from)
    };
    let mut cursor = find(body, &delimiter, 0)? + delimiter.len();
    loop {
        if body.get(cursor..cursor + 2) == Some(&b"--"[..]) {
            return None;
        }
        let head_start = find(body, b"\r\n", cursor)? + 2;
        let head_end = find(body, b"\r\n\r\n", head_start)?;
        let head = std::str::from_utf8(&body[head_start..head_end]).ok()?;
        let data_start = head_end + 4;
        let mut closing = b"\r\n".to_vec();
        closing.extend_from_slice(&delimiter);
        let data_end = find(body, &closing, data_start)?;
        let mut name = None;
        let mut part_type = None;
        for line in head.split("\r\n") {
            let (key, value) = line.split_once(':')?;
            if key.trim().eq_ignore_ascii_case("content-disposition") {
                name = value.split(';').find_map(|item| {
                    let (k, v) = item.trim().split_once('=')?;
                    (k.trim() == "name").then(|| v.trim().trim_matches('"').to_owned())
                });
            } else if key.trim().eq_ignore_ascii_case("content-type") {
                part_type = Some(value.trim().to_owned());
            }
        }
        if name.as_deref() == Some("file") {
            return Some((
                part_type.unwrap_or_else(|| "application/octet-stream".to_owned()),
                body[data_start..data_end].to_vec(),
            ));
        }
        cursor = data_end + closing.len();
    }
}

async fn upload_photo(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let (content_type, data) =
        multipart_file(&headers, &body).ok_or(HouseNumberingError::InvalidRequest)?;
    let mut edit = module_survey(&state, &auth, org, project).await?;
    let result = store::upload_photo(
        &mut edit,
        audit_actor(&auth),
        &state.artifact_store_dir,
        &content_type,
        &data,
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}
