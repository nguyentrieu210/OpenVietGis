//! Administrative hierarchy (areas) and route catalog routes.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, put};
use axum::{Json, Router};
use ovg_module_house_numbering::canonical_prefix;
use ovg_module_house_numbering_storage as store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::{
    audit_actor, csrf, decode, module_edit, module_read, opt_text_len, path_id, query_positive,
    scope_ids, text_len, HnResult, HouseNumberingError, BASE,
};
use ovg_module_http::ModuleSession as HumanSession;
use ovg_module_http::ModuleHttpState as AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/areas/{{area_id}}"),
            put(update_area).delete(delete_area),
        )
        .route(
            &format!("{BASE}/routes/{{route_id}}"),
            put(update_route).delete(delete_route),
        )
        .route(&format!("{BASE}/numbering-targets"), get(numbering_targets))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AreaUpdateBody {
    expected_revision: i64,
    name: String,
    kind: String,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default)]
    feature_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentBody {
    source_id: Uuid,
    feature_id: String,
    #[serde(default)]
    code: String,
}

fn default_street() -> String {
    "street".to_owned()
}
fn default_forward() -> String {
    "forward".to_owned()
}
fn default_manual() -> String {
    "manual".to_owned()
}
fn default_draft() -> String {
    "draft".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteCreateBody {
    name: String,
    #[serde(default = "default_street")]
    route_type: String,
    #[serde(default)]
    parent_route_id: Option<Uuid>,
    #[serde(default)]
    prefix: Option<String>,
    source_id: Uuid,
    feature_id: String,
    #[serde(default = "default_forward")]
    direction_state: String,
    #[serde(default = "default_manual")]
    source_label: String,
    #[serde(default)]
    segments: Vec<SegmentBody>,
    #[serde(default = "default_draft")]
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteUpdateBody {
    expected_revision: i64,
    name: String,
    direction_state: String,
    status: String,
}

async fn update_area(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let area_id = path_id(&params, "area_id")?;
    let body: AreaUpdateBody = decode(&body)?;
    if body.expected_revision <= 0
        || !text_len(&body.name, 1, 200)
        || !matches!(body.kind.as_str(), "current" | "historic" | "custom")
        || !opt_text_len(body.feature_id.as_deref(), 128)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::update_area(
        &mut edit,
        audit_actor(&auth),
        area_id,
        store::AreaUpdateInput {
            expected_revision: body.expected_revision,
            name: &body.name,
            kind: &body.kind,
            parent_id: body.parent_id,
            source_id: body.source_id,
            feature_id: body.feature_id.as_deref(),
        },
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn delete_area(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<StatusCode> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let area_id = path_id(&params, "area_id")?;
    let expected_revision = query_positive(&query, "expected_revision")?;
    let mut edit = module_edit(&state, &auth, org, project).await?;
    store::delete_area(&mut edit, audit_actor(&auth), area_id, expected_revision).await?;
    edit.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn create_route(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: RouteCreateBody = decode(&body)?;
    let prefix = match body.prefix.as_deref() {
        Some(value) if value.chars().count() > 100 => {
            return Err(HouseNumberingError::InvalidRequest)
        }
        Some(value) => {
            Some(canonical_prefix(value).map_err(|_| HouseNumberingError::InvalidRequest)?)
        }
        None => None,
    };
    if !text_len(&body.name, 1, 200)
        || !matches!(body.route_type.as_str(), "street" | "alley")
        || !text_len(&body.feature_id, 1, 128)
        || !matches!(body.direction_state.as_str(), "forward" | "reverse")
        || !text_len(&body.source_label, 1, 200)
        || body.segments.len() > 200
        || body.segments.iter().any(|segment| {
            !text_len(&segment.feature_id, 1, 128) || segment.code.chars().count() > 160
        })
        || !matches!(body.status.as_str(), "draft" | "confirmed")
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::create_route(
        &mut edit,
        audit_actor(&auth),
        store::RouteCreateInput {
            name: &body.name,
            route_type: &body.route_type,
            parent_route_id: body.parent_route_id,
            prefix: prefix.as_deref(),
            source_id: body.source_id,
            feature_id: &body.feature_id,
            direction_state: &body.direction_state,
            source_label: &body.source_label,
            status: &body.status,
            segments: body
                .segments
                .iter()
                .map(|segment| store::RouteSegmentInput {
                    source_id: segment.source_id,
                    feature_id: &segment.feature_id,
                    code: &segment.code,
                })
                .collect(),
        },
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_route(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let route_id = path_id(&params, "route_id")?;
    let body: RouteUpdateBody = decode(&body)?;
    if body.expected_revision <= 0
        || !text_len(&body.name, 1, 200)
        || !matches!(body.direction_state.as_str(), "forward" | "reverse")
        || !matches!(body.status.as_str(), "draft" | "confirmed")
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::update_route(
        &mut edit,
        audit_actor(&auth),
        route_id,
        store::RouteUpdateInput {
            expected_revision: body.expected_revision,
            name: &body.name,
            direction_state: &body.direction_state,
            status: &body.status,
        },
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn delete_route(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<StatusCode> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let route_id = path_id(&params, "route_id")?;
    let mut edit = module_edit(&state, &auth, org, project).await?;
    store::delete_route(&mut edit, audit_actor(&auth), route_id).await?;
    edit.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pydantic lax boolean parsing for query strings.
pub(super) fn query_bool(
    query: &HashMap<String, String>,
    key: &str,
    default: bool,
) -> HnResult<bool> {
    match query.get(key).map(|value| value.to_ascii_lowercase()) {
        None => Ok(default),
        Some(value) => match value.as_str() {
            "1" | "on" | "t" | "true" | "y" | "yes" => Ok(true),
            "0" | "off" | "f" | "false" | "n" | "no" => Ok(false),
            _ => Err(HouseNumberingError::InvalidRequest),
        },
    }
}

/// Pydantic int query parameter with inclusive bounds.
pub(super) fn query_int(
    query: &HashMap<String, String>,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> HnResult<i64> {
    let value = match query.get(key) {
        None => default,
        Some(raw) => raw
            .trim()
            .parse::<i64>()
            .map_err(|_| HouseNumberingError::InvalidRequest)?,
    };
    if value < min || value > max {
        return Err(HouseNumberingError::InvalidRequest);
    }
    Ok(value)
}

async fn numbering_targets(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let route_id = path_id(&query, "route_id")?;
    let include = query_bool(&query, "include_descendants", true)?;
    let limit = query_int(&query, "limit", 5000, 1, 5000)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::numbering_targets(&mut read, route_id, include, limit).await?;
    read.commit().await?;
    Ok(Json(result))
}
