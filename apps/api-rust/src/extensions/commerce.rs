//! Explicitly synthetic checkout/replay APIs. No external provider, card data,
//! paid activation or automatic financial settlement. Disabled without a
//! dedicated sandbox verification key.
use super::{
    authenticate_actor, authenticate_json, decode_body,
    marketplace_v2::{load_verified, Profile},
    open_org_scope, parse_uuid, require_actor_csrf,
    validation::decode_ed25519_public_key,
    ExtensionError,
};
use crate::AppState;
use axum::{
    extract::{Path, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use ovg_module_sdk::commerce::{Quote, SandboxEvent};
use serde::Deserialize;
use serde_json::Value;
const BASE: &str = "/organizations/{organization_id}/marketplace/sandbox/orders";
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, post(create))
        .route(&format!("{BASE}/{{order_id}}"), get(snapshot))
        .route(&format!("{BASE}/{{order_id}}/events"), post(event))
}
fn key(state: &AppState) -> Result<VerifyingKey, ExtensionError> {
    let raw = state
        .config
        .plugin_sandbox_payment_public_key
        .as_deref()
        .ok_or_else(|| ExtensionError::Domain(AppError::not_found("sandbox commerce disabled")))?;
    let k = decode_ed25519_public_key(raw).map_err(|_| {
        ExtensionError::Domain(AppError::internal(
            "invalid sandbox verification configuration",
        ))
    })?;
    if state
        .config
        .marketplace_operator_public_key
        .as_deref()
        .and_then(|v| decode_ed25519_public_key(v).ok())
        .is_some_and(|v| v == k)
    {
        return Err(ExtensionError::Domain(AppError::internal(
            "sandbox and operator keys must differ",
        )));
    }
    Ok(k)
}
fn response(v: Value) -> Response {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(v)).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    quote: Quote,
    release_digest: String,
}
async fn limited(req: Request) -> Result<Request, ExtensionError> {
    let (parts, body) = req.into_parts();
    let b = axum::body::to_bytes(body, 8192)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    Ok(Request::from_parts(parts, axum::body::Body::from(b)))
}
async fn create(
    State(state): State<AppState>,
    Path(org): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    key(&state)?;
    let (actor, headers, body) = authenticate_json(&state, limited(req).await?, &org).await?;
    require_actor_csrf(&state, &actor, &headers)?;
    if !actor.is_human() {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "human administrator required",
        )));
    }
    let body: Create = decode_body(body)?;
    let org_id = parse_uuid(&org)?;
    if body.quote.organization_id != org_id {
        return Err(ExtensionError::InvalidRequest);
    }
    // A quote is synthetic and user-defined; do not treat this as a price offer.
    let catalog = load_verified(
        state.config.clone(),
        Profile {
            host: "web".into(),
            api: "1.0.0".into(),
        },
    )
    .await?;
    if !catalog.iter().any(|r| {
        r.metadata["release_digest"].as_str() == Some(&body.release_digest)
            && r.metadata["envelope"]["extension_key"].as_str() == Some(&body.quote.extension_key)
    }) {
        return Err(ExtensionError::Domain(AppError::not_found(
            "reviewed release not found",
        )));
    }
    let mut tx = open_org_scope(&state, &actor, org_id).await?;
    let v = tx
        .create_sandbox_order(&body.quote, &body.release_digest)
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(response(v))
}
async fn snapshot(
    State(state): State<AppState>,
    Path((org, id)): Path<(String, String)>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let k = key(&state)?;
    let (mut parts, _) = req.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let mut tx = open_org_scope(&state, &actor, parse_uuid(&org)?).await?;
    let v = tx
        .sandbox_snapshot(parse_uuid(&id)?, &k, Utc::now())
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(response(v))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventBody {
    event: SandboxEvent,
    signature: String,
}
async fn event(
    State(state): State<AppState>,
    Path((org, id)): Path<(String, String)>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let k = key(&state)?;
    let (actor, headers, body) = authenticate_json(&state, limited(req).await?, &org).await?;
    require_actor_csrf(&state, &actor, &headers)?;
    let body: EventBody = decode_body(body)?;
    let org_id = parse_uuid(&org)?;
    if body.event.order_id != parse_uuid(&id)?
        || body.event.organization_id != org_id
        || body.signature.len() != 88
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let bytes = STANDARD
        .decode(&body.signature)
        .map_err(|_| ExtensionError::InvalidRequest)?;
    if STANDARD.encode(&bytes) != body.signature {
        return Err(ExtensionError::InvalidRequest);
    }
    let sig = Signature::from_slice(&bytes).map_err(|_| ExtensionError::InvalidRequest)?;
    let mut tx = open_org_scope(&state, &actor, org_id).await?;
    let v = tx
        .record_sandbox_event(body.event, &sig, &k, Utc::now())
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(response(v))
}
