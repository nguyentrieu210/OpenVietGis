//! Opt-in identity candidate. No existing Web/service/project auth is changed.
//! Compiling this feature is not production authorization.
use super::{database, require_csrf, HumanSession, IdentityError, PrivateJson};
use crate::AppState;
use axum::{extract::{DefaultBodyLimit, State}, http::{header, HeaderMap, StatusCode}, routing::{get, post}, Json, Router};
use ovg_storage::mobile_identity as mobile;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

const CLIENT_ID: &str = "openvietgis-mobile";
fn invalid() -> IdentityError { IdentityError::Http(StatusCode::UNAUTHORIZED, "invalid mobile credential") }
fn unavailable() -> IdentityError { IdentityError::Http(StatusCode::SERVICE_UNAVAILABLE, "mobile identity unavailable") }
fn map_error(error: mobile::Error) -> IdentityError {
    match error { mobile::Error::Unavailable => unavailable(), _ => invalid() }
}
fn no_store(body: Value) -> PrivateJson<Value> {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(body))
}
fn config(state: &AppState) -> Result<(mobile::Client, String, String), IdentityError> {
    let url = url::Url::parse(&state.config.api_public_base_url).map_err(|_| unavailable())?;
    if !state.config.session_cookie_secure || state.config.csrf_secret.as_deref().is_none_or(str::is_empty)
        || url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty()
        || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
        || url.path() != "/" || url.port_or_known_default() != Some(443) {
        return Err(unavailable());
    }
    let issuer = url.origin().ascii_serialization();
    let redirect = format!("{issuer}/mobile/callback");
    let client = mobile::Client::reviewed(&issuer, CLIENT_ID, &redirect).map_err(map_error)?;
    Ok((client, issuer, redirect))
}
fn require_native(headers: &HeaderMap, bearer: bool) -> Result<(), IdentityError> {
    if headers.contains_key(header::COOKIE) || headers.contains_key(header::ORIGIN)
        || (!bearer && headers.contains_key(header::AUTHORIZATION)) {
        return Err(invalid());
    }
    Ok(())
}
fn access(headers: &HeaderMap) -> Result<&str, IdentityError> {
    require_native(headers, true)?;
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next().and_then(|v| v.to_str().ok()).ok_or_else(invalid)?;
    if values.next().is_some() { return Err(invalid()); }
    value.strip_prefix("Bearer ").filter(|v| v.starts_with("ovg_ma_")).ok_or_else(invalid)
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/mobile/auth/grant", post(grant))
        .route("/mobile/auth/exchange", post(exchange))
        .route("/mobile/auth/refresh", post(refresh))
        .route("/mobile/auth/session", get(session))
        .route("/mobile/auth/revoke", post(revoke))
        .layer(DefaultBodyLimit::max(8192))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant { client_id: String, redirect_uri: String, code_challenge: String, code_challenge_method: String, device_id: Uuid, consent: bool }
async fn grant(State(state): State<AppState>, human: HumanSession, headers: HeaderMap, Json(input): Json<Grant>) -> Result<PrivateJson<Value>, IdentityError> {
    let (client, issuer, redirect) = config(&state)?;
    require_csrf(&state, &human, &headers)?;
    // Explicit browser consent plus the existing verified human identity.
    if headers.contains_key(header::AUTHORIZATION) || !input.consent || input.client_id != CLIENT_ID
        || input.redirect_uri != redirect || input.code_challenge_method != "S256" {
        return Err(IdentityError::Http(StatusCode::FORBIDDEN, "mobile consent or client invalid"));
    }
    let code = mobile::issue_grant(database(&state)?, &human.verified, &client, &redirect, &input.code_challenge, input.device_id)
        .await.map_err(map_error)?;
    Ok(no_store(json!({"code":code,"iss":issuer,"expires_in":300})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange { client_id: String, code: String, redirect_uri: String, code_verifier: String }
async fn exchange(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Exchange>) -> Result<PrivateJson<Value>, IdentityError> {
    require_native(&headers, false)?;
    let (client, issuer, redirect) = config(&state)?;
    if input.client_id != CLIENT_ID || input.redirect_uri != redirect { return Err(invalid()); }
    let pool = database(&state)?;
    let tokens = mobile::exchange(pool, &client, &input.code, &redirect, &input.code_verifier).await.map_err(map_error)?;
    token_response(pool, &client, issuer, tokens).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Refresh { client_id: String, refresh_token: String }
async fn refresh(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Refresh>) -> Result<PrivateJson<Value>, IdentityError> {
    require_native(&headers, false)?;
    let (client, issuer, _) = config(&state)?;
    if input.client_id != CLIENT_ID { return Err(invalid()); }
    let pool = database(&state)?;
    let tokens = mobile::rotate(pool, &client, &input.refresh_token).await.map_err(map_error)?;
    token_response(pool, &client, issuer, tokens).await
}
async fn token_response(pool: &ovg_storage::DbPool, client: &mobile::Client, issuer: String, tokens: mobile::Tokens) -> Result<PrivateJson<Value>, IdentityError> {
    let actor = mobile::authenticate(pool, client, &tokens.access).await.map_err(map_error)?;
    Ok(no_store(json!({"access_token":tokens.access,"refresh_token":tokens.refresh,"token_type":"Bearer",
        "expires_in":600,"iss":issuer,"sub":actor.user_id,"session_id":actor.session_id,"device_id":actor.device_id})))
}
async fn session(State(state): State<AppState>, headers: HeaderMap) -> Result<PrivateJson<Value>, IdentityError> {
    let raw = access(&headers)?;
    let (client, issuer, _) = config(&state)?;
    let actor = mobile::authenticate(database(&state)?, &client, raw).await.map_err(map_error)?;
    Ok(no_store(json!({"iss":issuer,"sub":actor.user_id,"session_id":actor.session_id,"device_id":actor.device_id})))
}
async fn revoke(State(state): State<AppState>, headers: HeaderMap) -> Result<PrivateJson<Value>, IdentityError> {
    let raw = access(&headers)?;
    let (client, _, _) = config(&state)?;
    let pool = database(&state)?;
    let actor = mobile::authenticate(pool, &client, raw).await.map_err(map_error)?;
    mobile::revoke(pool, &actor).await.map_err(map_error)?;
    Ok(no_store(json!({"revoked":true})))
}
