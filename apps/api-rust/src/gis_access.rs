//! E01 project-scoped GIS access groups, rules, filtered exports, and changeset reviews.

use std::collections::{BTreeSet, HashMap};

use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::editing::{EditError, Guard, Permission};
use ovg_storage::gis_access::{self, RuleInput};
use ovg_storage::tenancy::{self, AUTHORIZATION_MANAGE_GRANTS, PROJECT_MANAGE, PROJECT_READ};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::edit_auth::{
    self, authenticate_actor, authenticate_json, require_csrf as actor_csrf, Failure,
};
use crate::identity::{require_csrf, HumanSession, IdentityError};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/access";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, get(get_access))
        .route(&format!("{BASE}/mine"), get(mine))
        .route(&format!("{BASE}/groups"), post(create_group))
        .route(&format!("{BASE}/groups/{{group_id}}"), put(replace_group))
        .route(&format!("{BASE}/rules"), post(create_rule))
        .route(&format!("{BASE}/rules/batch"), post(batch_rules))
        .route(&format!("{BASE}/rules/{{rule_id}}"), delete(revoke_rule))
        .route(&format!("{BASE}/export/{{layer_id}}"), get(export_layer))
        .route(&format!("{BASE}/reviews"), get(list_reviews))
        .route(
            &format!("{BASE}/reviews/{{changeset_id}}"),
            post(decide_review),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupBody {
    name: String,
    #[serde(default)]
    user_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleBody {
    #[serde(default)]
    user_id: Option<Uuid>,
    #[serde(default)]
    group_id: Option<Uuid>,
    layer_id: Uuid,
    actions: Vec<String>,
    #[serde(default)]
    feature_ids: Option<Vec<String>>,
    #[serde(default)]
    attribute_filter: Option<Value>,
    #[serde(default)]
    region: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchRulesBody {
    #[serde(default)]
    rules: Vec<RuleBody>,
    #[serde(default)]
    revoke_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionBody {
    decision: String,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    #[serde(default = "default_geojson")]
    format: String,
}

fn default_geojson() -> String {
    "geojson".to_owned()
}

fn validate_rule_body(body: RuleBody) -> Result<RuleInput, GisAccessError> {
    if body.user_id.is_some() == body.group_id.is_some() {
        return Err(GisAccessError::Unprocessable("choose one subject"));
    }
    if body.actions.is_empty() || body.actions.len() > 7 {
        return Err(GisAccessError::Unprocessable("invalid actions"));
    }
    for action in &body.actions {
        if !matches!(
            action.as_str(),
            "read" | "create" | "geometry" | "attributes" | "delete" | "export" | "approve"
        ) {
            return Err(GisAccessError::Unprocessable("invalid action"));
        }
    }
    if !body.actions.iter().any(|a| a == "read") {
        return Err(GisAccessError::Unprocessable("read permission required"));
    }
    if let Some(ref fids) = body.feature_ids {
        if fids.is_empty()
            || fids.len() > 50_000
            || fids.iter().any(|f| f.is_empty() || f.chars().count() > 128)
        {
            return Err(GisAccessError::Unprocessable("invalid feature IDs"));
        }
    }
    if let Some(ref af) = body.attribute_filter {
        let valid_af = af.as_object().is_some_and(|m| !m.is_empty())
            && serde_json::to_string(af).is_ok_and(|s| s.len() <= 10_000);
        if !valid_af {
            return Err(GisAccessError::Unprocessable("invalid attribute filter"));
        }
    }
    if let Some(ref reg) = body.region {
        let valid_reg = reg
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| matches!(t, "Polygon" | "MultiPolygon"))
            && serde_json::to_string(reg).is_ok_and(|s| s.len() <= 100_000);
        if !valid_reg {
            return Err(GisAccessError::Unprocessable("region must be polygon"));
        }
    }
    Ok(RuleInput {
        user_id: body.user_id,
        group_id: body.group_id,
        layer_id: body.layer_id,
        actions: body.actions,
        feature_ids: body.feature_ids,
        attribute_filter: body.attribute_filter,
        region: body.region,
    })
}

async fn open_admin_scope<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    org_id: Uuid,
    project_id: Uuid,
    write: bool,
) -> Result<tenancy::TenantTx<'a>, GisAccessError> {
    let mut tx = crate::tenancy::begin(state, auth)
        .await
        .map_err(GisAccessError::Tenancy)?;
    tx.member(org_id, write, false)
        .await
        .map_err(GisAccessError::Domain)?;
    tx.require(org_id, None, None, AUTHORIZATION_MANAGE_GRANTS)
        .await
        .map_err(GisAccessError::Domain)?;
    let perm = if write { PROJECT_MANAGE } else { PROJECT_READ };
    tx.require(org_id, None, Some(project_id), perm)
        .await
        .map_err(GisAccessError::Domain)?;
    Ok(tx)
}

async fn get_access(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, GisAccessError> {
    let mut tx = open_admin_scope(&state, &auth, org_id, project_id, false).await?;
    let body = gis_access::get_access_admin(&mut tx, org_id, project_id)
        .await
        .map_err(GisAccessError::Domain)?;
    tx.commit().await.map_err(GisAccessError::Domain)?;
    Ok(Json(body))
}

async fn mine(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let session = authenticate_actor(&state, &mut parts, &org).await?;
    let Some(mut scope) =
        edit_auth::open(&state, &session, &org, &proj, Guard::ReadScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let body = gis_access::my_access(&mut scope).await?;
    scope.commit().await?;
    Ok(Json(body))
}

async fn create_group(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>), GisAccessError> {
    require_csrf(&state, &auth, &headers).map_err(GisAccessError::Identity)?;
    let req: GroupBody = serde_json::from_slice(&body)
        .map_err(|_| GisAccessError::Unprocessable("invalid group payload"))?;
    if req.name.is_empty() || req.name.chars().count() > 160 || req.user_ids.len() > 500 {
        return Err(GisAccessError::Unprocessable("invalid group payload"));
    }
    let mut tx = open_admin_scope(&state, &auth, org_id, project_id, true).await?;
    let gid = gis_access::create_group(
        &mut tx,
        org_id,
        project_id,
        auth.actor_id(),
        auth.display_name(),
        &req.name,
        &req.user_ids,
    )
    .await
    .map_err(GisAccessError::Edit)?;
    tx.commit().await.map_err(GisAccessError::Domain)?;
    Ok((StatusCode::CREATED, Json(json!({ "id": gid }))))
}

async fn replace_group(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((org_id, project_id, group_id)): Path<(Uuid, Uuid, Uuid)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, GisAccessError> {
    require_csrf(&state, &auth, &headers).map_err(GisAccessError::Identity)?;
    let req: GroupBody = serde_json::from_slice(&body)
        .map_err(|_| GisAccessError::Unprocessable("invalid group payload"))?;
    if req.name.is_empty() || req.name.chars().count() > 160 || req.user_ids.len() > 500 {
        return Err(GisAccessError::Unprocessable("invalid group payload"));
    }
    let mut tx = open_admin_scope(&state, &auth, org_id, project_id, true).await?;
    let gid = gis_access::replace_group(
        &mut tx,
        org_id,
        project_id,
        auth.actor_id(),
        auth.display_name(),
        group_id,
        &req.name,
        &req.user_ids,
    )
    .await
    .map_err(GisAccessError::Edit)?;
    tx.commit().await.map_err(GisAccessError::Domain)?;
    Ok(Json(json!({ "id": gid })))
}

async fn create_rule(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>), GisAccessError> {
    require_csrf(&state, &auth, &headers).map_err(GisAccessError::Identity)?;
    let parsed: RuleBody = serde_json::from_slice(&body)
        .map_err(|_| GisAccessError::Unprocessable("invalid rule payload"))?;
    let rule = validate_rule_body(parsed)?;
    let mut tx = open_admin_scope(&state, &auth, org_id, project_id, true).await?;
    let rid = gis_access::create_rule(
        &mut tx,
        org_id,
        project_id,
        auth.actor_id(),
        auth.display_name(),
        &rule,
    )
    .await
    .map_err(GisAccessError::Edit)?;
    tx.commit().await.map_err(GisAccessError::Domain)?;
    Ok((StatusCode::CREATED, Json(json!({ "id": rid }))))
}

async fn batch_rules(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, GisAccessError> {
    require_csrf(&state, &auth, &headers).map_err(GisAccessError::Identity)?;
    if body.len() > 2_000_000 {
        return Err(GisAccessError::Unprocessable("batch payload too large"));
    }
    let parsed: BatchRulesBody = serde_json::from_slice(&body)
        .map_err(|_| GisAccessError::Unprocessable("invalid batch rules payload"))?;
    if (parsed.rules.is_empty() && parsed.revoke_ids.is_empty())
        || (!parsed.rules.is_empty() && !parsed.revoke_ids.is_empty())
        || parsed.rules.len() > 200
        || parsed.revoke_ids.len() > 200
    {
        return Err(GisAccessError::Unprocessable(
            "invalid batch rules selection",
        ));
    }
    let mut rules = Vec::with_capacity(parsed.rules.len());
    for item in parsed.rules {
        rules.push(validate_rule_body(item)?);
    }
    let mut tx = open_admin_scope(&state, &auth, org_id, project_id, true).await?;
    let res = gis_access::batch_rules(
        &mut tx,
        org_id,
        project_id,
        auth.actor_id(),
        auth.display_name(),
        &rules,
        &parsed.revoke_ids,
    )
    .await
    .map_err(GisAccessError::Edit)?;
    tx.commit().await.map_err(GisAccessError::Domain)?;
    Ok(Json(res))
}

async fn revoke_rule(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((org_id, project_id, rule_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, GisAccessError> {
    require_csrf(&state, &auth, &headers).map_err(GisAccessError::Identity)?;
    let mut tx = open_admin_scope(&state, &auth, org_id, project_id, true).await?;
    gis_access::revoke_rule(
        &mut tx,
        org_id,
        project_id,
        auth.actor_id(),
        auth.display_name(),
        rule_id,
    )
    .await
    .map_err(GisAccessError::Edit)?;
    tx.commit().await.map_err(GisAccessError::Domain)?;
    Ok(StatusCode::NO_CONTENT)
}

fn csv_safe(val: &str) -> String {
    let safe = if val.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{val}")
    } else {
        val.to_owned()
    };
    if safe.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn json_cell_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(true)) => "True".to_owned(),
        Some(Value::Bool(false)) => "False".to_owned(),
        Some(other) => other.to_string(),
    }
}

async fn export_layer(
    State(state): State<AppState>,
    Path((org, proj, layer_id)): Path<(String, String, Uuid)>,
    query: Result<Query<ExportQuery>, QueryRejection>,
    request: Request,
) -> Result<Response, Failure> {
    let Query(q) = query.map_err(|_| EditError::InvalidRequest)?;
    if !matches!(q.format.as_str(), "geojson" | "csv") {
        return Err(EditError::InvalidRequest.into());
    }
    let (mut parts, _) = request.into_parts();
    let session = authenticate_actor(&state, &mut parts, &org).await?;
    let Some(mut scope) =
        edit_auth::open(&state, &session, &org, &proj, Guard::ReadScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let rows = gis_access::export_layer_rows(&mut scope, layer_id).await?;
    scope.commit().await?;

    if q.format == "geojson" {
        let features: Vec<Value> = rows
            .into_iter()
            .map(|r| {
                json!({
                    "type": "Feature",
                    "id": r.feature_id,
                    "properties": r.attributes,
                    "geometry": r.geometry,
                })
            })
            .collect();
        let body = serde_json::to_string(&json!({
            "type": "FeatureCollection",
            "features": features,
        }))
        .map_err(|e| EditError::Internal(e.to_string()))?;
        let mut resp = Response::new(Body::from(body));
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/geo+json"),
        );
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        resp.headers_mut().insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=features.geojson"),
        );
        return Ok(resp);
    }

    let mut keys_set = BTreeSet::new();
    for r in &rows {
        if let Some(obj) = r.attributes.as_object() {
            for k in obj.keys() {
                keys_set.insert(k.clone());
            }
        }
    }
    let keys: Vec<String> = keys_set.into_iter().collect();
    let mut csv_buf = String::new();
    let mut header_row = Vec::with_capacity(keys.len() + 2);
    header_row.push("feature_id".to_owned());
    for k in &keys {
        header_row.push(csv_safe(k));
    }
    header_row.push("geometry".to_owned());
    csv_buf.push_str(&header_row.join(","));
    csv_buf.push_str("\r\n");

    for r in rows {
        let map: HashMap<String, Value> = match r.attributes {
            Value::Object(m) => m.into_iter().collect(),
            _ => HashMap::new(),
        };
        let mut cells = Vec::with_capacity(keys.len() + 2);
        cells.push(csv_safe(&r.feature_id));
        for k in &keys {
            let s = json_cell_str(map.get(k));
            cells.push(csv_safe(&s));
        }
        let geom_json = r
            .geometry
            .map_or_else(|| "null".to_owned(), |g| g.to_string());
        cells.push(csv_safe(&geom_json));
        csv_buf.push_str(&cells.join(","));
        csv_buf.push_str("\r\n");
    }

    let mut resp = Response::new(Body::from(csv_buf));
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv"));
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=features.csv"),
    );
    Ok(resp)
}

async fn list_reviews(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let session = authenticate_actor(&state, &mut parts, &org).await?;
    let Some(mut scope) =
        edit_auth::open(&state, &session, &org, &proj, Guard::ReadScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    if scope.actor_kind() != "human" {
        return Err(EditError::detail(403, "human reviewer required").into());
    }
    scope.authorize(Permission::Read).await?;
    if !scope.is_scoped() {
        scope.authorize(Permission::Manage).await?;
    }
    let body = gis_access::list_reviews(&mut scope).await?;
    scope.commit().await?;
    Ok(Json(body))
}

async fn decide_review(
    State(state): State<AppState>,
    Path((org, proj, changeset_id)): Path<(String, String, Uuid)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (session, headers, body) = authenticate_json(&state, request, &org).await?;
    actor_csrf(&state, &session, &headers)?;
    let parsed: DecisionBody = edit_auth::decode(body)?;
    if !matches!(parsed.decision.as_str(), "approved" | "rejected")
        || parsed.note.chars().count() > 500
    {
        return Err(EditError::InvalidRequest.into());
    }
    let Some(mut scope) =
        edit_auth::open(&state, &session, &org, &proj, Guard::ScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    if scope.actor_kind() != "human" {
        return Err(EditError::detail(403, "human reviewer required").into());
    }
    scope.authorize(Permission::Read).await?;
    if !scope.is_scoped() {
        scope.authorize(Permission::Manage).await?;
    }
    let rid =
        gis_access::decide_review(&mut scope, changeset_id, &parsed.decision, &parsed.note).await?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": rid }))))
}

enum GisAccessError {
    Identity(IdentityError),
    Tenancy(crate::tenancy::TenancyError),
    Edit(EditError),
    Domain(AppError),
    Unprocessable(&'static str),
}

impl IntoResponse for GisAccessError {
    fn into_response(self) -> Response {
        match self {
            Self::Identity(e) => e.into_response(),
            Self::Tenancy(e) => e.into_response(),
            Self::Unprocessable(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "detail": msg })),
            )
                .into_response(),
            Self::Edit(EditError::Detail(code, msg)) => {
                let st = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (st, Json(json!({ "detail": msg }))).into_response()
            }
            Self::Edit(EditError::Internal(detail)) => {
                tracing::error!(detail = %detail, "gis_access error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Edit(e) => Failure::Edit(e).into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "gis_access domain error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(e) => {
                let st = StatusCode::from_u16(e.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (st, Json(e.to_envelope(None))).into_response()
            }
        }
    }
}
