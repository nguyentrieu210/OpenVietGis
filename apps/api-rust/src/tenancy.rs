//! E01 authentication session, organization, membership, workspace, project and
//! audit routes for verified browser sessions, matching the FastAPI wire.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ovg_core::AppError;
use ovg_storage::tenancy::{
    organization_role_permissions, GrantInput, InvitationInput, InvitationProof, InvitationResult,
    Membership, Project, TenantTx, Workspace, AUDIT_READ, ORGANIZATION_DELETE,
    ORGANIZATION_LIFECYCLE, ORGANIZATION_MANAGE_MEMBERS, ORGANIZATION_MANAGE_SETTINGS,
    ORGANIZATION_OWNER_TRANSFER, PROJECT_ARCHIVE, PROJECT_DELETE, PROJECT_MANAGE,
    PROJECT_MOVE_WORKSPACE, PROJECT_READ, WORKSPACE_MANAGE, WORKSPACE_READ,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::identity::{require_csrf, HumanSession, IdentityError};
use crate::AppState;

pub(crate) fn router() -> Router<AppState> {
    const ORG: &str = "/organizations/{organization_id}";
    Router::new()
        .route("/auth/providers", get(list_identity_providers))
        .route("/auth/logout", post(logout))
        .route("/organizations", post(create_organization))
        .route(&format!("{ORG}/members"), get(list_members))
        .route(
            &format!("{ORG}/members/{{membership_id}}"),
            patch(update_membership),
        )
        .route(
            &format!("{ORG}/effective-permissions"),
            get(effective_permissions),
        )
        .route(
            &format!("{ORG}/workspaces"),
            get(list_workspaces).post(create_workspace),
        )
        .route(
            &format!("{ORG}/workspaces/{{workspace_id}}"),
            patch(update_workspace),
        )
        .route(
            &format!("{ORG}/projects"),
            get(list_projects).post(create_project),
        )
        .route(
            &format!("{ORG}/projects/{{project_id}}"),
            get(get_project).patch(update_project),
        )
        .route(
            &format!("{ORG}/ownership-transfer"),
            post(transfer_ownership),
        )
        .route(
            &format!("{ORG}/grants"),
            get(list_resource_grants).post(create_resource_grant),
        )
        .route(
            &format!("{ORG}/grants/{{grant_id}}/revoke"),
            post(revoke_resource_grant),
        )
        .route(
            &format!("{ORG}/service-principals"),
            get(list_service_principals).post(create_service_principal),
        )
        .route(
            &format!("{ORG}/service-principals/{{principal_id}}/revoke"),
            post(revoke_service_principal),
        )
        .route(
            &format!("{ORG}/service-principals/{{principal_id}}/credentials"),
            get(list_service_credentials).post(issue_service_credential),
        )
        .route(
            &format!(
                "{ORG}/service-principals/{{principal_id}}/credentials/{{credential_id}}/revoke"
            ),
            post(revoke_service_credential),
        )
        .route(
            &format!("{ORG}/invitations"),
            get(list_invitations).post(create_invitation),
        )
        .route(
            &format!("{ORG}/invitations/{{invitation_id}}/revoke"),
            post(revoke_invitation),
        )
        .route("/invitations/check", post(check_invitation))
        .route("/invitations/accept", post(accept_invitation))
        .route(&format!("{ORG}/audit-events"), get(list_audit_events))
}

pub(crate) enum TenancyError {
    InvalidRequest,
    Detail(StatusCode, String),
    Internal,
    Identity(IdentityError),
}

impl From<AppError> for TenancyError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Internal { internal_detail } => {
                tracing::error!(detail = %internal_detail, "tenancy request failed");
                Self::Internal
            }
            AppError::Validation { .. } => Self::InvalidRequest,
            error => Self::Detail(
                StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                error.to_string(),
            ),
        }
    }
}

impl From<IdentityError> for TenancyError {
    fn from(error: IdentityError) -> Self {
        Self::Identity(error)
    }
}

impl IntoResponse for TenancyError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            Self::Detail(status, detail) => {
                (status, Json(json!({ "detail": detail }))).into_response()
            }
            Self::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Identity(error) => error.into_response(),
        }
    }
}

type Result<T> = std::result::Result<T, TenancyError>;

fn detail(status: StatusCode, message: &str) -> TenancyError {
    TenancyError::Detail(status, message.to_owned())
}

fn forbidden(message: &str) -> TenancyError {
    detail(StatusCode::FORBIDDEN, message)
}

fn not_found(message: &str) -> TenancyError {
    detail(StatusCode::NOT_FOUND, message)
}

fn bad_request(message: &str) -> TenancyError {
    detail(StatusCode::BAD_REQUEST, message)
}

pub(crate) async fn begin(state: &AppState, session: &HumanSession) -> Result<TenantTx> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        AppError::service_unavailable("DATABASE_UNAVAILABLE", "database unavailable")
    })?;
    Ok(TenantTx::begin(
        pool,
        session.actor_id(),
        session.session_id(),
        session.display_name(),
    )
    .await?)
}

pub(crate) fn uuid(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|_| TenancyError::InvalidRequest)
}

/// JSON object body checked like a Pydantic model with `extra="forbid"`.
struct Body(Map<String, Value>);

impl Body {
    fn parse(bytes: &[u8], allowed: &[&str]) -> Result<Self> {
        match serde_json::from_slice(bytes) {
            Ok(Value::Object(map)) if map.keys().all(|key| allowed.contains(&key.as_str())) => {
                Ok(Self(map))
            }
            _ => Err(TenancyError::InvalidRequest),
        }
    }

    fn provided(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Optional string of 1..=`max` characters; `strip` applies Pydantic
    /// `strip_whitespace` before the length check.
    fn text(&self, key: &str, max: usize, strip: bool) -> Result<Option<String>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => {
                let value = if strip { value.trim() } else { value.as_str() };
                let length = value.chars().count();
                if (1..=max).contains(&length) {
                    Ok(Some(value.to_owned()))
                } else {
                    Err(TenancyError::InvalidRequest)
                }
            }
            Some(_) => Err(TenancyError::InvalidRequest),
        }
    }

    fn required_text(&self, key: &str, max: usize, strip: bool) -> Result<String> {
        self.text(key, max, strip)?
            .ok_or(TenancyError::InvalidRequest)
    }

    fn choice(&self, key: &str, choices: &[&'static str]) -> Result<Option<&'static str>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => choices
                .iter()
                .find(|choice| **choice == value.as_str())
                .copied()
                .map(Some)
                .ok_or(TenancyError::InvalidRequest),
            Some(_) => Err(TenancyError::InvalidRequest),
        }
    }

    fn uuid(&self, key: &str) -> Result<Option<Uuid>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => uuid(value).map(Some),
            Some(_) => Err(TenancyError::InvalidRequest),
        }
    }
}

/// `_clean_name`: the domain strips and rejects blank names.
fn clean_name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(bad_request("name is required"));
    }
    Ok(name.to_owned())
}

/// `_clean_slug`: lowercase `^[a-z0-9]+(?:-[a-z0-9]+)*$`.
fn clean_slug(value: &str) -> Result<String> {
    let slug = value.trim().to_lowercase();
    let valid = !slug.is_empty()
        && slug.split('-').all(|part| {
            !part.is_empty() && part.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'))
        });
    if !valid {
        return Err(bad_request("invalid slug"));
    }
    Ok(slug)
}

const ORGANIZATION_STATUSES: [&str; 3] = ["active", "suspended", "deleted"];
const MEMBERSHIP_ROLES: [&str; 4] = ["owner", "admin", "member", "guest"];
const MEMBERSHIP_STATUSES: [&str; 3] = ["active", "suspended", "revoked"];
const LIFECYCLE_STATUSES: [&str; 3] = ["active", "archived", "deleted"];

async fn list_identity_providers(State(state): State<AppState>) -> Json<Vec<Value>> {
    Json(
        state
            .config
            .oidc_providers
            .iter()
            .map(|(id, provider)| json!({ "id": id, "display_name": provider.display_name }))
            .collect(),
    )
}

/// `scheme`, `netloc` and `path` like Python `urlsplit`.
fn split_url(url: &str) -> (&str, &str, &str) {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (netloc, path) = rest
        .find('/')
        .map_or((rest, ""), |index| rest.split_at(index));
    (scheme, netloc, path)
}

/// Starlette `delete_cookie` header value.
fn deleted_cookie(name: &str, path: &str, secure: bool, http_only: bool) -> String {
    let mut cookie = format!(
        "{name}=\"\"; expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0; Path={path}; SameSite=lax"
    );
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

async fn logout(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
) -> Result<Response> {
    require_csrf(&state, &session, &headers)?;
    let mut tx = begin(&state, &session).await?;
    tx.revoke_session(session.session_id()).await?;
    tx.commit().await?;

    let config = &state.config;
    let mut cookies = vec![deleted_cookie(
        &config.session_cookie_name,
        "/",
        config.session_cookie_secure,
        true,
    )];
    // The OIDC transaction cookie of the reference session middleware.
    let has_oidc_state = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .any(|entry| entry.trim_start().starts_with("ovg_oidc_state="));
    if has_oidc_state {
        cookies.push(deleted_cookie(
            "ovg_oidc_state",
            "/",
            config.session_cookie_secure,
            false,
        ));
    }
    // A reverse-proxied Keycloak realm on the web origin keeps SSO cookies;
    // discard them too so the next login cannot resume the previous account.
    if let Some(provider) = config.oidc_providers.get("keycloak") {
        let (scheme, netloc, path) = split_url(&provider.issuer);
        let (web_scheme, web_netloc, _) = split_url(&config.web_base_url);
        let (api_scheme, api_netloc, _) = split_url(&config.api_public_base_url);
        let realm = path.trim_end_matches('/');
        if (scheme, netloc) == (web_scheme, web_netloc)
            && (scheme, netloc) == (api_scheme, api_netloc)
            && realm.starts_with("/realms/")
        {
            for name in [
                "KEYCLOAK_IDENTITY",
                "KEYCLOAK_SESSION",
                "KEYCLOAK_IDENTITY_LEGACY",
                "KEYCLOAK_SESSION_LEGACY",
                "KEYCLOAK_REMEMBER_ME",
                "AUTH_SESSION_ID",
                "AUTH_SESSION_ID_LEGACY",
                "KC_AUTH_SESSION_HASH",
                "KC_RESTART",
            ] {
                for cookie_path in [format!("{realm}/"), realm.to_owned()] {
                    cookies.push(deleted_cookie(name, &cookie_path, scheme == "https", true));
                }
            }
        }
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for cookie in cookies {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response_headers.append(header::SET_COOKIE, value);
        }
    }
    Ok(response)
}

async fn create_organization(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>)> {
    require_csrf(&state, &session, &headers)?;
    let body = Body::parse(&body, &["name", "slug"])?;
    let name = body.required_text("name", 200, false)?;
    let slug = body.required_text("slug", 120, false)?;
    let mut tx = begin(&state, &session).await?;
    if !tx.installation_initialized().await? {
        return Err(detail(
            StatusCode::CONFLICT,
            "installation is not initialized",
        ));
    }
    let (organization, owner) = tx
        .create_organization(&clean_name(&name)?, &clean_slug(&slug)?)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "organization": organization, "owner_membership": owner })),
    ))
}

pub(crate) async fn update_organization(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let body = Body::parse(&body, &["name", "status"])?;
    let name = body.text("name", 200, false)?;
    let status = body.choice("status", &ORGANIZATION_STATUSES)?;
    if name.is_none() && status.is_none() {
        return Err(TenancyError::InvalidRequest);
    }

    let mut tx = begin(&state, &session).await?;
    let (organization, membership) = tx.member(organization_id, true, true).await?;
    let permissions = organization_role_permissions(&membership.role);
    if name.is_some()
        && (organization.status != "active" || !permissions.contains(ORGANIZATION_MANAGE_SETTINGS))
    {
        return Err(forbidden("organization settings update is not allowed"));
    }
    if let Some(status) = status {
        let required = if status == "deleted" {
            ORGANIZATION_DELETE
        } else {
            ORGANIZATION_LIFECYCLE
        };
        if !permissions.contains(required) {
            return Err(forbidden(&format!("missing permission: {required}")));
        }
    }
    // Service-level recheck under the membership row lock.
    let required = if status.is_some() {
        ORGANIZATION_LIFECYCLE
    } else {
        ORGANIZATION_MANAGE_SETTINGS
    };
    let caller = tx.own_membership_for_update(organization_id).await?;
    if !caller.is_some_and(|caller| {
        caller.status == "active" && organization_role_permissions(&caller.role).contains(required)
    }) {
        return Err(forbidden("insufficient organization authority"));
    }
    if organization.status != "active" && status.is_none() {
        return Err(bad_request("organization is not active"));
    }
    if organization.status != "active" && name.is_some() {
        return Err(bad_request(
            "restore organization before changing its settings",
        ));
    }
    let new_status = status.map_or_else(|| organization.status.clone(), str::to_owned);
    if organization.status == "deleted" && !matches!(new_status.as_str(), "deleted" | "active") {
        return Err(bad_request("deleted organization can only be restored"));
    }
    let new_name = match &name {
        Some(name) => clean_name(name)?,
        None => organization.name.clone(),
    };
    tx.update_organization(organization_id, &new_name, &new_status)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({
        "id": organization.id,
        "name": new_name,
        "slug": organization.slug,
        "status": new_status,
    })))
}

async fn list_members(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
) -> Result<Json<Vec<Value>>> {
    let organization_id = uuid(&organization_id)?;
    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    tx.require(organization_id, None, None, ORGANIZATION_MANAGE_MEMBERS)
        .await?;
    let members = tx.list_members(organization_id).await?;
    tx.commit().await?;
    Ok(Json(members))
}

async fn update_membership(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, membership_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Membership>> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let membership_id = uuid(&membership_id)?;
    let body = Body::parse(&body, &["role", "status"])?;
    let role = body.choice("role", &MEMBERSHIP_ROLES)?;
    let status = body.choice("status", &MEMBERSHIP_STATUSES)?;
    if role.is_none() && status.is_none() {
        return Err(TenancyError::InvalidRequest);
    }

    let mut tx = begin(&state, &session).await?;
    let (_, acting) = tx.member(organization_id, true, false).await?;
    let target = tx
        .membership(organization_id, membership_id, false)
        .await?
        .ok_or_else(|| not_found("membership not found"))?;
    tx.require(organization_id, None, None, ORGANIZATION_MANAGE_MEMBERS)
        .await?;
    if (target.role == "owner" || role == Some("owner"))
        && !organization_role_permissions(&acting.role).contains(ORGANIZATION_OWNER_TRANSFER)
    {
        return Err(forbidden("Owner authority is required for an Owner change"));
    }
    // Service-level checks under the caller and target row locks.
    let caller = tx
        .own_membership_for_update(organization_id)
        .await?
        .filter(|caller| {
            caller.status == "active"
                && organization_role_permissions(&caller.role).contains(ORGANIZATION_MANAGE_MEMBERS)
        })
        .ok_or_else(|| forbidden("insufficient organization authority"))?;
    let target = tx
        .membership(organization_id, membership_id, true)
        .await?
        .ok_or_else(|| not_found("membership not found"))?;
    let new_role = role.map_or_else(|| target.role.clone(), str::to_owned);
    let new_status = status.map_or_else(|| target.status.clone(), str::to_owned);
    if (target.role == "owner" || new_role == "owner") && caller.role != "owner" {
        return Err(forbidden("Owner required for ownership changes"));
    }
    let stops_being_active_owner = target.role == "owner"
        && target.status == "active"
        && (new_role != "owner" || new_status != "active");
    if stops_being_active_owner
        && tx
            .count_other_active_owners(organization_id, target.id)
            .await?
            == 0
    {
        return Err(bad_request("organization must retain an active Owner"));
    }
    tx.set_membership(organization_id, target.id, &new_role, &new_status)
        .await?;
    tx.audit(
        organization_id,
        "organization.membership.update",
        "organization_membership",
        Some(target.id),
        "success",
        json!({ "role": new_role, "status": new_status }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Membership {
        role: new_role,
        status: new_status,
        ..target
    }))
}

/// Transfer ownership atomically; promote first, then demote the old owner.
async fn transfer_ownership(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<Response> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let body = Body::parse(&body, &["target_membership_id"])?;
    let target_id = body
        .uuid("target_membership_id")?
        .ok_or(TenancyError::InvalidRequest)?;
    let mut tx = begin(&state, &session).await?;
    let (_, acting) = tx.member(organization_id, true, false).await?;
    tx.membership(organization_id, target_id, false)
        .await?
        .ok_or_else(|| not_found("membership not found"))?;
    if !organization_role_permissions(&acting.role).contains(ORGANIZATION_OWNER_TRANSFER) {
        tx.audit(
            organization_id,
            "organization.owner.transfer",
            "organization",
            None,
            "denied",
            json!({ "reason": "insufficient_owner_authority" }),
        )
        .await?;
        tx.commit().await?;
        return Ok(forbidden("Owner authority required").into_response());
    }
    let mut previous = tx
        .own_membership_for_update(organization_id)
        .await?
        .filter(|member| member.status == "active" && member.role == "owner")
        .ok_or_else(|| forbidden("active Owner required"))?;
    let mut promoted = tx
        .membership(organization_id, target_id, true)
        .await?
        .filter(|member| member.status == "active")
        .ok_or_else(|| not_found("active target membership required"))?;
    tx.set_membership(organization_id, promoted.id, "owner", "active")
        .await?;
    promoted.role = "owner".to_owned();
    if previous.id != promoted.id {
        tx.set_membership(organization_id, previous.id, "admin", "active")
            .await?;
        previous.role = "admin".to_owned();
    }
    tx.audit(
        organization_id,
        "organization.owner.transfer",
        "organization_membership",
        Some(promoted.id),
        "success",
        json!({ "previous_owner_membership_id": previous.id.to_string() }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "previous_owner": previous, "new_owner": promoted })).into_response())
}

#[derive(Deserialize)]
struct ScopeQuery {
    workspace_id: Option<String>,
    project_id: Option<String>,
}

async fn effective_permissions(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
    query: std::result::Result<Query<ScopeQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>> {
    let organization_id = uuid(&organization_id)?;
    let Query(query) = query.map_err(|_| TenancyError::InvalidRequest)?;
    let workspace_id = query.workspace_id.as_deref().map(uuid).transpose()?;
    let project_id = query.project_id.as_deref().map(uuid).transpose()?;

    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    if let Some(workspace_id) = workspace_id {
        if tx.workspace(organization_id, workspace_id).await?.is_none() {
            return Err(not_found("workspace not found"));
        }
    }
    if let Some(project_id) = project_id {
        let project = tx.project(organization_id, project_id).await?;
        if !project
            .is_some_and(|project| workspace_id.is_none() || project.workspace_id == workspace_id)
        {
            return Err(not_found("project not found"));
        }
    }
    let permissions = tx
        .permissions(organization_id, workspace_id, project_id)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "permissions": permissions })))
}

async fn list_workspaces(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
) -> Result<Json<Vec<Workspace>>> {
    let organization_id = uuid(&organization_id)?;
    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    let all = tx
        .permissions(organization_id, None, None)
        .await?
        .contains(WORKSPACE_READ);
    let mut visible = Vec::new();
    for workspace in tx.list_workspaces(organization_id).await? {
        if all
            || tx
                .permissions(organization_id, Some(workspace.id), None)
                .await?
                .contains(WORKSPACE_READ)
        {
            visible.push(workspace);
        }
    }
    tx.commit().await?;
    Ok(Json(visible))
}

async fn create_workspace(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Workspace>)> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let body = Body::parse(&body, &["name", "slug"])?;
    let name = body.required_text("name", 200, false)?;
    let slug = body.required_text("slug", 120, false)?;

    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, true, false).await?;
    tx.require(organization_id, None, None, WORKSPACE_MANAGE)
        .await?;
    let workspace = tx
        .create_workspace(organization_id, &clean_name(&name)?, &clean_slug(&slug)?)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

async fn update_workspace(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, workspace_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Workspace>> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let workspace_id = uuid(&workspace_id)?;
    let body = Body::parse(&body, &["name", "status"])?;
    let name = body.text("name", 200, false)?;
    let status = body.choice("status", &LIFECYCLE_STATUSES)?;
    if name.is_none() && status.is_none() {
        return Err(TenancyError::InvalidRequest);
    }

    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, true, false).await?;
    let existing = tx
        .workspace(organization_id, workspace_id)
        .await?
        .ok_or_else(|| not_found("workspace not found"))?;
    tx.require(organization_id, Some(workspace_id), None, WORKSPACE_MANAGE)
        .await?;
    let status_only = body.provided("status") && !body.provided("name");
    if existing.status == "deleted" && (!status_only || status != Some("active") || name.is_some())
    {
        return Err(bad_request(
            "deleted workspace requires a status-only restore",
        ));
    }
    let new_status = status.map_or_else(|| existing.status.clone(), str::to_owned);
    if new_status == "deleted"
        && tx
            .count_active_projects(organization_id, workspace_id)
            .await?
            > 0
    {
        return Err(bad_request(
            "workspace with active projects cannot be deleted",
        ));
    }
    let new_name = match &name {
        Some(name) => clean_name(name)?,
        None => existing.name.clone(),
    };
    tx.update_workspace(&existing, &new_name, &new_status)
        .await?;
    tx.commit().await?;
    Ok(Json(Workspace {
        name: new_name,
        status: new_status,
        ..existing
    }))
}

#[derive(Deserialize)]
struct WorkspaceFilter {
    workspace_id: Option<String>,
}

async fn list_projects(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
    query: std::result::Result<Query<WorkspaceFilter>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<Project>>> {
    let organization_id = uuid(&organization_id)?;
    let Query(query) = query.map_err(|_| TenancyError::InvalidRequest)?;
    let workspace_id = query.workspace_id.as_deref().map(uuid).transpose()?;

    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    if let Some(workspace_id) = workspace_id {
        let workspace = tx.workspace(organization_id, workspace_id).await?;
        if workspace.is_none_or(|workspace| workspace.status == "deleted") {
            return Err(not_found("workspace not found"));
        }
    }
    let all = tx
        .permissions(organization_id, workspace_id, None)
        .await?
        .contains(PROJECT_READ);
    let mut visible = Vec::new();
    for project in tx.list_projects(organization_id, workspace_id).await? {
        if all
            || tx
                .permissions(organization_id, None, Some(project.id))
                .await?
                .contains(PROJECT_READ)
        {
            visible.push(project);
        }
    }
    tx.commit().await?;
    Ok(Json(visible))
}

async fn create_project(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Project>)> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let body = Body::parse(&body, &["name", "slug", "workspace_id"])?;
    let name = body.required_text("name", 200, true)?;
    let slug = body.required_text("slug", 120, false)?;
    let workspace_id = body.uuid("workspace_id")?;

    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, true, false).await?;
    let workspace = match workspace_id {
        Some(workspace_id) => Some(
            tx.workspace(organization_id, workspace_id)
                .await?
                .ok_or_else(|| not_found("workspace not found"))?,
        ),
        None => None,
    };
    tx.require(organization_id, workspace_id, None, PROJECT_MANAGE)
        .await?;
    if workspace.is_some_and(|workspace| workspace.status == "deleted") {
        return Err(bad_request("deleted workspace cannot receive projects"));
    }
    let project = tx
        .create_project(organization_id, workspace_id, &name, &clean_slug(&slug)?)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn get_project(
    State(state): State<AppState>,
    session: HumanSession,
    Path((organization_id, project_id)): Path<(String, String)>,
) -> Result<Json<Project>> {
    let organization_id = uuid(&organization_id)?;
    let project_id = uuid(&project_id)?;
    let mut tx = begin(&state, &session).await?;
    // Reading the project itself is a GIS-safe operation for scoped members.
    tx.project_guard(organization_id, project_id, true).await?;
    tx.member(organization_id, false, false).await?;
    let project = tx
        .project(organization_id, project_id)
        .await?
        .filter(|project| project.status != "deleted")
        .ok_or_else(|| not_found("project not found"))?;
    tx.require(organization_id, None, Some(project_id), PROJECT_READ)
        .await?;
    tx.commit().await?;
    Ok(Json(project))
}

async fn update_project(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Project>> {
    // The project access guard precedes CSRF and payload validation.
    let ids = uuid(&organization_id).and_then(|org| Ok((org, uuid(&project_id)?)));
    let mut tx = begin(&state, &session).await?;
    if let Ok((organization_id, project_id)) = ids {
        if tx.project_guard(organization_id, project_id, false).await? {
            return Err(forbidden("Thao tác chưa được cấp quyền trong phạm vi GIS"));
        }
    }
    require_csrf(&state, &session, &headers)?;
    let (organization_id, project_id) = ids?;
    let body = Body::parse(&body, &["name", "workspace_id", "status"])?;
    let name = body.text("name", 200, true)?;
    let workspace_supplied = body.provided("workspace_id");
    let workspace_id = body.uuid("workspace_id")?;
    let status = body.choice("status", &LIFECYCLE_STATUSES)?;
    if name.is_none() && status.is_none() && !workspace_supplied {
        return Err(TenancyError::InvalidRequest);
    }

    tx.member(organization_id, true, false).await?;
    let existing = tx
        .project(organization_id, project_id)
        .await?
        .ok_or_else(|| not_found("project not found"))?;
    if let Some(workspace_id) = workspace_id {
        if tx.workspace(organization_id, workspace_id).await?.is_none() {
            return Err(not_found("workspace not found"));
        }
    }
    let scope = (organization_id, None, Some(project_id));
    if name.is_some() {
        tx.require(scope.0, scope.1, scope.2, PROJECT_MANAGE)
            .await?;
    }
    if workspace_supplied {
        tx.require(scope.0, scope.1, scope.2, PROJECT_MOVE_WORKSPACE)
            .await?;
    }
    if matches!(status, Some("archived" | "active")) {
        tx.require(scope.0, scope.1, scope.2, PROJECT_ARCHIVE)
            .await?;
    }
    if status == Some("deleted") {
        tx.require(scope.0, scope.1, scope.2, PROJECT_DELETE)
            .await?;
    }
    let deleted = existing.status == "deleted";
    if deleted && status == Some("active") {
        tx.require(scope.0, scope.1, scope.2, PROJECT_DELETE)
            .await?;
        if body.provided("name") || workspace_supplied {
            return Err(bad_request(
                "deleted project requires a status-only restore",
            ));
        }
    }

    // Service-level lifecycle rules.
    if deleted && (status != Some("active") || name.is_some() || workspace_supplied) {
        return Err(bad_request(
            "deleted project requires a status-only restore",
        ));
    }
    if existing.status == "archived"
        && (name.is_some() || (workspace_supplied && status != Some("active")))
    {
        return Err(bad_request(
            "archived project must be restored before normal mutation",
        ));
    }
    let target_workspace = if workspace_supplied {
        workspace_id
    } else {
        existing.workspace_id
    };
    if let (true, Some(workspace_id)) = (workspace_supplied, workspace_id) {
        let workspace = tx
            .workspace(organization_id, workspace_id)
            .await?
            .ok_or_else(|| bad_request("workspace does not belong to organization"))?;
        if workspace.status == "deleted" {
            return Err(bad_request("deleted workspace cannot receive projects"));
        }
    }
    let new_status = status.map_or_else(|| existing.status.clone(), str::to_owned);
    if let (true, Some(workspace_id)) = (new_status == "active", target_workspace) {
        let workspace_status = tx
            .workspace(organization_id, workspace_id)
            .await?
            .map(|workspace| workspace.status);
        if deleted && workspace_status.as_deref() != Some("active") {
            return Err(bad_request(
                "restore the workspace before the deleted project",
            ));
        }
        if matches!(workspace_status.as_deref(), None | Some("deleted")) {
            return Err(bad_request(
                "restore project with an explicit valid workspace or ungroup it",
            ));
        }
    }
    let updated = Project {
        name: match &name {
            Some(name) => clean_name(name)?,
            None => existing.name.clone(),
        },
        workspace_id: target_workspace,
        status: new_status,
        ..existing.clone()
    };
    tx.update_project(&existing, &updated).await?;
    tx.commit().await?;
    Ok(Json(updated))
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<String>,
}

async fn create_resource_grant(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>)> {
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    let body = Body::parse(
        &body,
        &[
            "subject_kind",
            "subject_id",
            "scope_kind",
            "workspace_id",
            "project_id",
            "role",
        ],
    )?;
    let input = GrantInput {
        subject_kind: body
            .choice("subject_kind", &["membership", "service_principal"])?
            .ok_or(TenancyError::InvalidRequest)?,
        subject_id: body
            .uuid("subject_id")?
            .ok_or(TenancyError::InvalidRequest)?,
        scope_kind: body
            .choice("scope_kind", &["organization", "workspace", "project"])?
            .ok_or(TenancyError::InvalidRequest)?,
        workspace_id: body.uuid("workspace_id")?,
        project_id: body.uuid("project_id")?,
        role: body
            .choice("role", &["manager", "editor", "contributor", "viewer"])?
            .ok_or(TenancyError::InvalidRequest)?,
    };
    let mut tx = begin(&state, &session).await?;
    let grant = tx.create_grant(organization_id, input).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(grant)))
}

async fn revoke_resource_grant(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, grant_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    require_csrf(&state, &session, &headers)?;
    let mut tx = begin(&state, &session).await?;
    tx.revoke_grant(uuid(&organization_id)?, uuid(&grant_id)?)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_service_principals(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
) -> Result<Json<Vec<Value>>> {
    let organization_id = uuid(&organization_id)?;
    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    let items = tx.list_principals(organization_id).await?;
    tx.commit().await?;
    Ok(Json(items))
}

async fn create_service_principal(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>)> {
    require_csrf(&state, &session, &headers)?;
    let body = Body::parse(&body, &["name", "description"])?;
    let name = body.required_text("name", 200, false)?;
    let description = match body.0.get("description") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.as_str()),
        _ => return Err(TenancyError::InvalidRequest),
    };
    let mut tx = begin(&state, &session).await?;
    let value = tx
        .create_principal(uuid(&organization_id)?, &name, description)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn revoke_service_principal(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, principal_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    require_csrf(&state, &session, &headers)?;
    let mut tx = begin(&state, &session).await?;
    tx.revoke_principal(uuid(&organization_id)?, uuid(&principal_id)?)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_service_credentials(
    State(state): State<AppState>,
    session: HumanSession,
    Path((organization_id, principal_id)): Path<(String, String)>,
) -> Result<Response> {
    let organization_id = uuid(&organization_id)?;
    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    let items = tx
        .list_credentials(organization_id, uuid(&principal_id)?)
        .await?;
    tx.commit().await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(items)).into_response())
}

async fn issue_service_credential(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, principal_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response> {
    require_csrf(&state, &session, &headers)?;
    let body = Body::parse(&body, &["name", "permission_ceiling", "expires_at"])?;
    let name = body.required_text("name", 200, false)?;
    let ceiling = match body.0.get("permission_ceiling") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or(TenancyError::InvalidRequest)
                })
                .collect::<Result<BTreeSet<_>>>()?,
        ),
        _ => return Err(TenancyError::InvalidRequest),
    };
    let expires_at = match body.0.get("expires_at") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(
            DateTime::parse_from_rfc3339(value)
                .map_err(|_| TenancyError::InvalidRequest)?
                .with_timezone(&Utc),
        ),
        _ => return Err(TenancyError::InvalidRequest),
    };
    let mut tx = begin(&state, &session).await?;
    let value = tx
        .issue_credential(
            uuid(&organization_id)?,
            uuid(&principal_id)?,
            &name,
            ceiling,
            expires_at,
        )
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(value),
    )
        .into_response())
}

async fn revoke_service_credential(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((organization_id, principal_id, credential_id)): Path<(String, String, String)>,
) -> Result<StatusCode> {
    require_csrf(&state, &session, &headers)?;
    let mut tx = begin(&state, &session).await?;
    tx.revoke_credential(
        uuid(&organization_id)?,
        uuid(&principal_id)?,
        uuid(&credential_id)?,
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_invitations(
    State(state): State<AppState>,
    session: HumanSession,
    Path(org): Path<String>,
) -> Result<Json<Vec<Value>>> {
    let mut tx = begin(&state, &session).await?;
    let items = tx.list_invitations(uuid(&org)?).await?;
    tx.commit().await?;
    Ok(Json(items))
}

async fn create_invitation(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Response> {
    require_csrf(&state, &session, &headers)?;
    let input = InvitationInput::parse(&body)?;
    let mut tx = begin(&state, &session).await?;
    let value = tx.issue_invitation(uuid(&org)?, input).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)).into_response())
}

async fn revoke_invitation(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    Path((org, id)): Path<(String, String)>,
) -> Result<StatusCode> {
    require_csrf(&state, &session, &headers)?;
    let mut tx = begin(&state, &session).await?;
    tx.revoke_invitation(uuid(&org)?, uuid(&id)?).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn check_invitation(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    require_csrf(&state, &session, &headers)?;
    let proof = InvitationProof::parse(&body)?;
    let mut tx = begin(&state, &session).await?;
    let status = tx.check_invitation(&proof, session.session_id()).await?;
    tx.commit().await?;
    Ok(Json(json!({"status":status})))
}

async fn accept_invitation(
    State(state): State<AppState>,
    session: HumanSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    require_csrf(&state, &session, &headers)?;
    let proof = InvitationProof::parse(&body)?;
    let mut tx = begin(&state, &session).await?;
    let result = tx.accept_invitation(&proof, session.session_id()).await?;
    tx.commit().await?;
    match result {
        InvitationResult::Accepted(value) => Ok(Json(value).into_response()),
        InvitationResult::VerifiedExpired => Ok(bad_request("invalid invitation").into_response()),
    }
}

async fn list_resource_grants(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
) -> Result<Json<Vec<Value>>> {
    let organization_id = uuid(&organization_id)?;
    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    let grants = tx.list_grants(organization_id).await?;
    tx.commit().await?;
    Ok(Json(grants))
}

async fn list_audit_events(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
    query: std::result::Result<Query<AuditQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<Value>>> {
    let organization_id = uuid(&organization_id)?;
    let Query(query) = query.map_err(|_| TenancyError::InvalidRequest)?;
    let limit = match query.limit.as_deref() {
        None => 50,
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|limit| (1..=200).contains(limit))
            .ok_or(TenancyError::InvalidRequest)?,
    };
    let mut tx = begin(&state, &session).await?;
    tx.member(organization_id, false, false).await?;
    tx.require(organization_id, None, None, AUDIT_READ).await?;
    let events = tx.list_audit(organization_id, limit).await?;
    tx.commit().await?;
    Ok(Json(events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_the_reference_pattern() {
        assert_eq!(clean_slug(" Ha-Noi-2 ").ok(), Some("ha-noi-2".to_owned()));
        for invalid in ["", "-a", "a-", "a--b", "a_b", "đà"] {
            assert!(clean_slug(invalid).is_err(), "{invalid}");
        }
        assert!(clean_name("   ").is_err());
    }

    #[test]
    fn bodies_reject_unknown_keys_and_bad_types() {
        assert!(Body::parse(br#"{"name":"a","extra":1}"#, &["name"]).is_err());
        assert!(Body::parse(b"[]", &["name"]).is_err());
        let body = Body::parse(
            br#"{"name":"  x  ","status":"archived"}"#,
            &["name", "status"],
        )
        .ok()
        .unwrap();
        assert_eq!(
            body.text("name", 200, true).ok().flatten(),
            Some("x".to_owned())
        );
        assert_eq!(
            body.choice("status", &LIFECYCLE_STATUSES).ok().flatten(),
            Some("archived")
        );
        assert!(body.choice("name", &LIFECYCLE_STATUSES).is_err());
        let blank = Body::parse(br#"{"name":"   "}"#, &["name"]).ok().unwrap();
        assert!(blank.text("name", 200, true).is_err());
        assert!(blank.text("name", 200, false).is_ok());
    }

    #[test]
    fn urls_split_like_python() {
        assert_eq!(
            split_url("https://gis.example/realms/ovg/"),
            ("https", "gis.example", "/realms/ovg/")
        );
        assert_eq!(
            split_url("https://gis.example"),
            ("https", "gis.example", "")
        );
    }
}
