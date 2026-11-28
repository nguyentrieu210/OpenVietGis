//! Declarative V2 bundle moderation linked to the immutable legacy submission.
//! Review is not release signing: the operator private key stays offline.
use super::{
    authenticate_human_json, decode_body, parse_uuid,
    validation::{decode_ed25519_public_key, marketplace_canonical},
    ExtensionError,
};
use crate::{
    catalog::CatalogError,
    identity::{require_csrf, HumanSession},
    AppState,
};
use axum::{
    extract::{Path, Query, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::Signature;
use ovg_core::AppError;
use ovg_module_sdk::{
    declarative::Document,
    host_profile::{read_only_policies, validate_read_only},
    release_v2::{canonical, ContributionEnvelope},
};
use ovg_storage::{
    extensions::marketplace::{DeveloperTx, SubmissionRow},
    DbPool,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
const BASE: &str = "/developer/marketplace/v2";
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/submissions"), get(list))
        .route(
            &format!("{BASE}/submissions/{{id}}"),
            get(detail).post(attach),
        )
        .route(&format!("{BASE}/submissions/{{id}}/decision"), post(decide))
        .route(
            &format!("{BASE}/submissions/{{id}}/withdraw"),
            post(withdraw),
        )
        .route(&format!("{BASE}/submissions/{{id}}/export"), get(export))
}
fn domain(e: AppError) -> ExtensionError {
    ExtensionError::Domain(e)
}
fn invalid() -> AppError {
    AppError::validation("INVALID_V2_SUBMISSION", "invalid signed declarative bundle")
}
fn sha(v: &[u8]) -> String {
    hex::encode(Sha256::digest(v))
}
fn bytes(s: &str, max: usize) -> Result<Vec<u8>, AppError> {
    if s.len() > max.div_ceil(3) * 4 {
        return Err(invalid());
    }
    let b = STANDARD.decode(s).map_err(|_| invalid())?;
    if b.len() > max || STANDARD.encode(&b) != s {
        return Err(invalid());
    }
    Ok(b)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublisherBundle {
    envelope: ContributionEnvelope,
    publisher_signature: String,
    contribution: String,
    assets: BTreeMap<String, String>,
}
fn verify_submission(b: &PublisherBundle, p: &SubmissionRow) -> Result<String, AppError> {
    let e = &b.envelope;
    e.validate()?;
    let vk = decode_ed25519_public_key(&p.public_key).map_err(|_| invalid())?;
    if e.publisher_id != p.publisher_id
        || e.extension_key != p.manifest["extension_key"].as_str().unwrap_or_default()
        || e.version != p.manifest["version"].as_str().unwrap_or_default()
        || e.key_id != sha(vk.as_bytes())
        || e.manifest_digest != sha(&marketplace_canonical(&p.manifest).map_err(|_| invalid())?)
        || !e.platforms.iter().any(|v| v == "web")
        || !semver::VersionReq::parse(&e.host_api_range)
            .map_err(|_| invalid())?
            .matches(&semver::Version::new(1, 0, 0))
        || !b.assets.is_empty()
        || !e.assets.is_empty()
        || p.manifest["entity_schemas"]
            .as_object()
            .is_none_or(|v| !v.is_empty())
    {
        return Err(invalid());
    }
    let canonical_bytes = canonical(e)?;
    let sig = Signature::from_slice(&bytes(&b.publisher_signature, 64)?).map_err(|_| invalid())?;
    vk.verify_strict(&canonical_bytes, &sig)
        .map_err(|_| invalid())?;
    let document = bytes(&b.contribution, 256 * 1024)?;
    if sha(&document) != e.contribution_digest {
        return Err(invalid());
    }
    let caps: BTreeSet<String> =
        serde_json::from_value(p.manifest["capabilities"].clone()).map_err(|_| invalid())?;
    let perms: BTreeSet<String> = e.permissions.iter().cloned().collect();
    validate_read_only(
        &Document::parse(&document)?,
        &perms,
        &read_only_policies(&e.extension_key, &caps),
    )?;
    Ok(sha(&canonical_bytes))
}
fn pool(state: &AppState) -> Result<&DbPool, ExtensionError> {
    state.db_pool.as_ref().ok_or_else(|| {
        domain(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })
}
fn reviewer(state: &AppState, auth: &HumanSession) -> bool {
    state
        .config
        .marketplace_reviewer_user_ids
        .contains(&auth.actor_id())
}
fn reply(v: Value) -> Response {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(v)).into_response()
}
async fn auth(
    state: &AppState,
    req: Request,
) -> Result<(HumanSession, Option<Value>), ExtensionError> {
    let (parts, body) = req.into_parts();
    let data = axum::body::to_bytes(body, 512 * 1024)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let (auth, headers, body) = authenticate_human_json(
        state,
        Request::from_parts(parts, axum::body::Body::from(data)),
    )
    .await?;
    require_csrf(state, &auth, &headers)
        .map_err(|e| ExtensionError::Access(CatalogError::Identity(e)))?;
    Ok((auth, body))
}
async fn tx<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    review: bool,
) -> Result<DeveloperTx<'a>, ExtensionError> {
    if review && !reviewer(state, auth) {
        return Err(domain(AppError::forbidden("platform reviewer required")));
    }
    DeveloperTx::begin(pool(state)?, auth.actor_id(), auth.session_id(), review)
        .await
        .map_err(ExtensionError::from_catalog_app)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    #[serde(default)]
    review: bool,
    #[serde(default = "all")]
    status: String,
    #[serde(default)]
    offset: i64,
}
fn all() -> String {
    "all".into()
}
async fn list(
    State(state): State<AppState>,
    auth: HumanSession,
    Query(q): Query<ListQuery>,
) -> Result<Response, ExtensionError> {
    if !matches!(
        q.status.as_str(),
        "all" | "pending" | "approved" | "rejected" | "withdrawn"
    ) || !(0..=100000).contains(&q.offset)
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let mut tx = tx(&state, &auth, q.review).await?;
    let mut items = tx.v2_list(&q.status, q.offset).await.map_err(domain)?;
    let next = (items.len() > 100).then_some(q.offset + 100);
    items.truncate(100);
    tx.commit().await.map_err(domain)?;
    Ok(reply(json!({"items":items,"next_offset":next})))
}
async fn detail(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(id): Path<String>,
) -> Result<Response, ExtensionError> {
    let mut tx = tx(&state, &auth, reviewer(&state, &auth)).await?;
    let r = tx.v2_detail(parse_uuid(&id)?).await.map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(r))
}
async fn attach(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (auth, body) = auth(&state, req).await?;
    let bundle: PublisherBundle = decode_body(body)?;
    let id = parse_uuid(&id)?;
    let mut tx = tx(&state, &auth, false).await?;
    let p = tx.parent_submission(id).await.map_err(domain)?;
    if p.owner_user_id != auth.actor_id() {
        return Err(domain(AppError::forbidden("publisher ownership required")));
    }
    let digest = verify_submission(&bundle, &p).map_err(domain)?;
    let r = tx
        .attach_v2(
            id,
            &serde_json::to_value(bundle).map_err(|_| ExtensionError::InvalidRequest)?,
            &digest,
        )
        .await
        .map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(r))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    decision: String,
    note: String,
}
fn note(s: &str) -> Result<&str, ExtensionError> {
    let s = s.trim();
    if !(1..=2000).contains(&s.chars().count()) {
        return Err(ExtensionError::InvalidRequest);
    }
    Ok(s)
}
async fn decide(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (auth, body) = auth(&state, req).await?;
    let b: Decision = decode_body(body)?;
    let id = parse_uuid(&id)?;
    let mut tx = tx(&state, &auth, true).await?;
    let r = tx.v2_detail(id).await.map_err(domain)?;
    let p = tx.parent_submission(id).await.map_err(domain)?;
    let bundle: PublisherBundle =
        serde_json::from_value(r["bundle"].clone()).map_err(|_| ExtensionError::InvalidRequest)?;
    if verify_submission(&bundle, &p).map_err(domain)?
        != r["release_digest"].as_str().unwrap_or_default()
    {
        return Err(domain(invalid()));
    }
    let result = tx
        .v2_decide(id, &b.decision, note(&b.note)?)
        .await
        .map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(result))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Withdrawal {
    reason: String,
}
async fn withdraw(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (auth, body) = auth(&state, req).await?;
    let b: Withdrawal = decode_body(body)?;
    let mut tx = tx(&state, &auth, reviewer(&state, &auth)).await?;
    let r = tx
        .v2_withdraw(parse_uuid(&id)?, note(&b.reason)?)
        .await
        .map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(r))
}
async fn export(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(id): Path<String>,
) -> Result<Response, ExtensionError> {
    let mut tx = tx(&state, &auth, true).await?;
    let id = parse_uuid(&id)?;
    let r = tx.v2_detail(id).await.map_err(domain)?;
    let p = tx.parent_submission(id).await.map_err(domain)?;
    if r["status"] != "approved"
        || r["withdrawal_requested"] == true
        || p.status != "approved"
        || p.owner_user_id == auth.actor_id()
    {
        return Err(domain(AppError::forbidden(
            "independently approved release required",
        )));
    }
    let b: PublisherBundle =
        serde_json::from_value(r["bundle"].clone()).map_err(|_| ExtensionError::InvalidRequest)?;
    if verify_submission(&b, &p).map_err(domain)?
        != r["release_digest"].as_str().unwrap_or_default()
    {
        return Err(domain(invalid()));
    }
    tx.commit().await.map_err(domain)?;
    Ok(reply(
        json!({"bundle":b,"publisher_public_key":p.public_key,"release_digest":r["release_digest"],"requires_offline_operator_signature":true}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    #[test]
    fn verifies_publisher_bundle_and_rejects_tampering() {
        let f: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/spec075/python-signed-catalog.json"
        ))
        .unwrap();
        let v = &f["v1"]["payload"];
        for item in f["v2"]["releases"].as_array().unwrap() {
            let mut raw = item.clone();
            raw.as_object_mut().unwrap().remove("operator_signature");
            let bundle: PublisherBundle = serde_json::from_value(raw).unwrap();
            let m = v["releases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["manifest"]["extension_key"] == bundle.envelope.extension_key)
                .unwrap();
            let p = SubmissionRow {
                id: Uuid::new_v4(),
                publisher_id: "fixture".into(),
                owner_user_id: Uuid::new_v4(),
                manifest: m["manifest"].clone(),
                signature: m["signature"].as_str().unwrap().into(),
                status: "approved".into(),
                review_note: None,
                reviewed_at: None,
                created_at: chrono::Utc::now(),
                publisher_name: "Fixture".into(),
                public_key: v["publishers"][0]["public_key"].as_str().unwrap().into(),
            };
            assert!(verify_submission(&bundle, &p).is_ok());
            let mut bad = bundle;
            bad.envelope.version = "2.0.0".into();
            assert!(verify_submission(&bad, &p).is_err());
        }
    }
}
