//! Online commercial entitlement verification. Signed authority-issued
//! certificates are not payment receipts or permission grants.
use super::{
    authenticate_actor, authenticate_json, decode_body,
    marketplace_v2::{load_verified, Profile, VerifiedRelease},
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
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use ovg_module_sdk::{
    entitlement::{verify_grant, CommercialPolicy, LicenseGrant},
    trust::fingerprint,
};
use ovg_storage::catalog::ProjectRead;
use serde::Deserialize;
use serde_json::{json, Value};

const BASE: &str = "/organizations/{organization_id}/marketplace/v2/licenses";
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, post(import))
        .route(&format!("{BASE}/{{digest}}"), get(status))
}
fn domain(e: AppError) -> ExtensionError {
    ExtensionError::Domain(e)
}
fn denied() -> AppError {
    AppError::forbidden("commercial entitlement required")
}
fn policy(row: &VerifiedRelease) -> Result<Option<CommercialPolicy>, AppError> {
    Ok(row.commercial_policy.as_deref().cloned())
}
fn authority(state: &AppState, p: &CommercialPolicy) -> Result<VerifyingKey, AppError> {
    let key = decode_ed25519_public_key(
        state
            .config
            .plugin_license_public_key
            .as_deref()
            .ok_or_else(denied)?,
    )
    .map_err(|_| denied())?;
    if fingerprint(key.as_bytes()) != p.authority_key_id || key.is_weak() {
        return Err(denied());
    }
    if [
        &state.config.plugin_sandbox_payment_public_key,
        &state.config.plugin_sandbox_offer_public_key,
    ]
    .into_iter()
    .any(|v| {
        v.as_deref()
            .and_then(|s| decode_ed25519_public_key(s).ok())
            .is_some_and(|k| k == key)
    }) {
        return Err(denied());
    }
    Ok(key)
}
pub(super) async fn require_graph_license(
    state: &AppState,
    read: &mut ProjectRead<'_>,
    pins: &Value,
    catalog: &[VerifiedRelease],
) -> Result<(), AppError> {
    for pin in pins.as_array().ok_or_else(denied)? {
        let digest = pin["release_digest"].as_str().ok_or_else(denied)?;
        let row = catalog
            .iter()
            .find(|r| r.metadata["release_digest"].as_str() == Some(digest))
            .ok_or_else(denied)?;
        if let Some(p) = policy(row)? {
            let k = authority(state, &p)?;
            let key = pin["key"].as_str().ok_or_else(denied)?;
            let status =
                ovg_storage::extensions::entitlement::project_license(read, digest, key, &p, &k)
                    .await?;
            if !status.allowed() {
                return Err(denied());
            }
        }
    }
    Ok(())
}
async fn catalog(state: &AppState) -> Result<Vec<VerifiedRelease>, ExtensionError> {
    load_verified(
        state,
        Profile {
            host: "web".into(),
            api: "1.0.0".into(),
        },
    )
    .await
}
fn selected<'a>(
    rows: &'a [VerifiedRelease],
    digest: &str,
) -> Result<(&'a VerifiedRelease, CommercialPolicy), ExtensionError> {
    let row = rows
        .iter()
        .find(|r| r.metadata["release_digest"].as_str() == Some(digest))
        .ok_or_else(|| domain(AppError::not_found("reviewed release not found")))?;
    let policy = policy(row).map_err(domain)?.ok_or_else(|| {
        domain(AppError::not_found(
            "release does not require a commercial license",
        ))
    })?;
    Ok((row, policy))
}
fn reply(v: Value) -> Response {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(v)).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Signed {
    certificate: LicenseGrant,
    signature: String,
}
async fn import(
    State(state): State<AppState>,
    Path(org): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (parts, body) = req.into_parts();
    let data = axum::body::to_bytes(body, 16384)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let (actor, headers, body) = authenticate_json(
        &state,
        Request::from_parts(parts, axum::body::Body::from(data)),
        &org,
    )
    .await?;
    require_actor_csrf(&state, &actor, &headers)?;
    if !actor.is_human() {
        return Err(domain(denied()));
    }
    let body: Signed = decode_body(body)?;
    let org_id = parse_uuid(&org)?;
    let rows = catalog(&state).await?;
    let (row, p) = selected(&rows, &body.certificate.release_digest)?;
    if row.metadata["envelope"]["extension_key"].as_str() != Some(&body.certificate.extension_key)
        || body.signature.len() != 88
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let raw = STANDARD
        .decode(&body.signature)
        .map_err(|_| ExtensionError::InvalidRequest)?;
    if STANDARD.encode(&raw) != body.signature {
        return Err(ExtensionError::InvalidRequest);
    }
    let sig = Signature::from_slice(&raw).map_err(|_| ExtensionError::InvalidRequest)?;
    let k = authority(&state, &p).map_err(domain)?;
    let verified =
        verify_grant(body.certificate, &sig, &k, &p, org_id, chrono::Utc::now()).map_err(domain)?;
    if p.revokes(verified.grant()) {
        return Err(domain(AppError::conflict(
            "LICENSE_CENTRALLY_REVOKED",
            "certificate has been centrally revoked; renewal required",
        )));
    }
    let mut tx = open_org_scope(&state, &actor, org_id).await?;
    let result = tx.import_license(&verified, &sig).await.map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(result))
}
async fn status(
    State(state): State<AppState>,
    Path((org, digest)): Path<(String, String)>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (mut parts, _) = req.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    if !actor.is_human() {
        return Err(domain(denied()));
    }
    let rows = catalog(&state).await?;
    let (row, p) = selected(&rows, &digest)?;
    let key = row.metadata["envelope"]["extension_key"]
        .as_str()
        .ok_or(ExtensionError::InvalidRequest)?;
    let k = authority(&state, &p).map_err(domain)?;
    let mut tx = open_org_scope(&state, &actor, parse_uuid(&org)?).await?;
    let status = tx
        .license_status(&digest, key, &p, &k)
        .await
        .map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(
        json!({"release_digest":digest,"state":status,"allowed":status.allowed()}),
    ))
}
