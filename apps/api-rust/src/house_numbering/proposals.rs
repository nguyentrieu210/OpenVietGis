//! Numbering proposal queue and approval workflow routes.

use super::module_approve;
use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_module_house_numbering_storage as store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::hierarchy::query_int;
use super::{
    audit_actor, csrf, decode, module_edit, module_read, opt_text_len, path_id,
    scope_ids, HnResult, HouseNumberingError, BASE,
};
use crate::identity::HumanSession;
use crate::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/proposals"),
            get(list_proposals).post(create_proposal),
        )
        .route(
            &format!("{BASE}/proposals/{{proposal_id}}"),
            get(get_proposal),
        )
        .route(
            &format!("{BASE}/proposals/{{proposal_id}}/transition"),
            post(transition_proposal),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalCreateBody {
    scope_kind: String,
    scope_id: Uuid,
    rule_id: Uuid,
    building_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionBody {
    expected_revision: i64,
    action: String,
    #[serde(default)]
    comment: Option<String>,
}

async fn create_proposal(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: ProposalCreateBody = decode(&body)?;
    if !matches!(body.scope_kind.as_str(), "route" | "area")
        || !(1..=5000).contains(&body.building_ids.len())
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::create_proposal(
        &mut edit,
        audit_actor(&auth),
        &body.scope_kind,
        body.scope_id,
        body.rule_id,
        &body.building_ids,
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_proposals(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let status = query.get("status").map(String::as_str);
    if status.is_some_and(|value| !matches!(value, "draft" | "submitted" | "approved")) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let offset = query_int(&query, "offset", 0, 0, i64::MAX)?;
    let limit = query_int(&query, "limit", 100, 1, 100)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_proposals(&mut read, status, offset, limit).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn get_proposal(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let proposal_id = path_id(&params, "proposal_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::get_proposal(&mut read, proposal_id).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn transition_proposal(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let proposal_id = path_id(&params, "proposal_id")?;
    let body: TransitionBody = decode(&body)?;
    if body.expected_revision <= 0
        || !matches!(
            body.action.as_str(),
            "submit" | "withdraw" | "approve" | "reject"
        )
        || !opt_text_len(body.comment.as_deref(), 1000)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let reviewer = matches!(body.action.as_str(), "approve" | "reject");
    let mut edit = if reviewer {
        module_approve(&state, &auth, org, project).await?
    } else {
        module_edit(&state, &auth, org, project).await?
    };
    let result = store::transition_proposal(
        &mut edit,
        audit_actor(&auth),
        proposal_id,
        body.expected_revision,
        &body.action,
        body.comment.as_deref(),
        reviewer,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}
