//! E01 read-only identity routes using the canonical browser session.

use axum::extract::{rejection::PathRejection, FromRequestParts, Path, State};
use axum::http::{header, request::Parts, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use ovg_core::AppError;
use ovg_storage::identity::{
    self, IdentityLink, OrganizationMembership, OrganizationProfile, SessionAuthenticationError,
    UserProfile, VerifiedSession,
};
use ovg_storage::DbPool;
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::{ApiError, AppState};

pub(crate) struct HumanSession {
    verified: VerifiedSession,
    token: String,
}

impl HumanSession {
    pub(crate) fn actor_id(&self) -> Uuid {
        self.verified.user_id()
    }

    pub(crate) fn session_id(&self) -> Uuid {
        self.verified.id()
    }

    pub(crate) fn display_name(&self) -> &str {
        self.verified.display_name()
    }
}

/// Port of `verify_request_csrf` for browser sessions on unsafe methods.
pub(crate) fn require_csrf(
    state: &AppState,
    session: &HumanSession,
    headers: &HeaderMap,
) -> Result<(), IdentityError> {
    let denied = || IdentityError::Http(StatusCode::FORBIDDEN, "CSRF validation failed");
    let secret = state.config.csrf_secret.as_deref().ok_or_else(denied)?;
    let candidate = headers
        .get("x-openvietgis-csrf")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let expected = make_csrf_token(&session.token, secret)?;
    if candidate.is_empty() || !constant_time_eq(expected.as_bytes(), candidate.as_bytes()) {
        return Err(denied());
    }
    Ok(())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

pub(crate) enum IdentityError {
    Http(StatusCode, &'static str),
    InvalidPayload,
    Application(ApiError),
}

impl From<AppError> for IdentityError {
    fn from(error: AppError) -> Self {
        Self::Application(error.into())
    }
}

impl IntoResponse for IdentityError {
    fn into_response(self) -> Response {
        match self {
            Self::Http(status, detail) => (status, Json(json!({ "detail": detail }))).into_response(),
            Self::InvalidPayload => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{ "loc": [], "msg": "invalid request payload", "type": "value_error" }]
                })),
            )
                .into_response(),
            Self::Application(error) => error.into_response(),
        }
    }
}

impl FromRequestParts<AppState> for HumanSession {
    type Rejection = IdentityError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let unauthorized = |detail| IdentityError::Http(StatusCode::UNAUTHORIZED, detail);

        // The reference API gives bearer credentials precedence. These self routes
        // require a browser identity, so a bearer header must not fall back to a cookie.
        if parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.get(..7))
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        {
            return Err(unauthorized("invalid service credential"));
        }

        let token = session_cookie(&parts.headers, &state.config.session_cookie_name)
            .ok_or_else(|| unauthorized("authentication required"))?;
        let verified = identity::authenticate_session(database(state)?, token)
            .await
            .map_err(|error| match error {
                SessionAuthenticationError::Invalid => unauthorized("invalid session"),
                SessionAuthenticationError::Revoked => unauthorized("session revoked"),
                SessionAuthenticationError::Expired => unauthorized("session expired"),
                SessionAuthenticationError::Database(error) => error.into(),
            })?;

        Ok(Self {
            verified,
            token: token.to_owned(),
        })
    }
}

fn session_cookie<'a>(headers: &'a HeaderMap, cookie_name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .rev()
        .filter_map(|entry| entry.split_once('='))
        .find_map(|(name, value)| (name.trim() == cookie_name).then_some(value.trim()))
        .map(|value| {
            value
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .unwrap_or(value)
        })
        .filter(|value| !value.is_empty())
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/bootstrap/status", get(bootstrap_status))
        .route("/me", get(get_me))
        .route("/me/organizations", get(list_my_organizations))
        .route(
            "/organizations/{organization_id}",
            get(get_organization).patch(crate::tenancy::update_organization),
        )
        .route("/me/identity-links", get(list_my_identity_links))
        .route("/me/avatar", get(get_my_avatar))
        .route("/auth/csrf", get(get_csrf))
}

type PrivateJson<T> = ([(header::HeaderName, &'static str); 1], Json<T>);

async fn bootstrap_status(State(state): State<AppState>) -> Result<Json<Value>, IdentityError> {
    let initialized = identity::installation_initialized(database(&state)?).await?;
    Ok(Json(json!({ "initialized": initialized })))
}

async fn get_me(session: HumanSession) -> PrivateJson<UserProfile> {
    (
        [(header::CACHE_CONTROL, "no-store, private")],
        Json(session.verified.into_user()),
    )
}

async fn list_my_organizations(
    State(state): State<AppState>,
    session: HumanSession,
) -> Result<Json<Vec<OrganizationMembership>>, IdentityError> {
    let organizations =
        identity::active_memberships(database(&state)?, &session.verified, None).await?;
    Ok(Json(organizations))
}

async fn get_organization(
    State(state): State<AppState>,
    session: HumanSession,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<OrganizationProfile>, IdentityError> {
    let Path(organization_id) = path.map_err(|_| IdentityError::InvalidPayload)?;
    let member =
        identity::active_memberships(database(&state)?, &session.verified, Some(organization_id))
            .await?
            .into_iter()
            .next()
            .ok_or(IdentityError::Http(
                StatusCode::NOT_FOUND,
                "organization not found",
            ))?;
    if member.organization.status != "active" && member.role != "owner" {
        return Err(IdentityError::Http(
            StatusCode::FORBIDDEN,
            "organization is not active",
        ));
    }
    if !matches!(member.role.as_str(), "owner" | "admin" | "member") {
        return Err(IdentityError::Http(
            StatusCode::FORBIDDEN,
            "missing permission: organization.read",
        ));
    }
    Ok(Json(member.organization))
}

async fn list_my_identity_links(
    State(state): State<AppState>,
    session: HumanSession,
) -> Result<Json<Vec<IdentityLink>>, IdentityError> {
    let links = identity::list_identity_links(database(&state)?, &session.verified).await?;
    Ok(Json(links))
}

async fn get_my_avatar(
    State(state): State<AppState>,
    session: HumanSession,
) -> Result<PrivateJson<Value>, IdentityError> {
    let data_url = identity::get_avatar(database(&state)?, &session.verified).await?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({ "data_url": data_url })),
    ))
}

async fn get_csrf(
    State(state): State<AppState>,
    session: HumanSession,
) -> Result<Json<Value>, IdentityError> {
    let secret = state
        .config
        .csrf_secret
        .as_deref()
        .ok_or(IdentityError::Http(
            StatusCode::SERVICE_UNAVAILABLE,
            "CSRF session unavailable",
        ))?;
    Ok(Json(
        json!({ "token": make_csrf_token(&session.token, secret)? }),
    ))
}

fn make_csrf_token(session_token: &str, secret: &str) -> Result<String, AppError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| AppError::internal("CSRF HMAC could not be initialized"))?;
    mac.update(session_token.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn database(state: &AppState) -> Result<&DbPool, AppError> {
    state.db_pool.as_ref().ok_or_else(|| {
        AppError::service_unavailable("DATABASE_UNAVAILABLE", "database unavailable")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_csrf_token_compatible_with_python() {
        let raw = "ovgs_10000000000000000000000000000001.s_example";
        assert_eq!(
            make_csrf_token(raw, "fixture-secret").unwrap(),
            "NRHT3hKZRKygmGgags2U9w6nS8GhV7UjAn6kYsTHxyg"
        );
    }

    #[test]
    fn csrf_comparison_is_exact() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
    }

    #[test]
    fn reads_the_configured_cookie_only() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "ovg_session=ignored; custom_session=\"actual\"; another=value"
                .parse()
                .unwrap(),
        );
        assert_eq!(session_cookie(&headers, "custom_session"), Some("actual"));
        assert_eq!(session_cookie(&headers, "missing"), None);
    }
}
