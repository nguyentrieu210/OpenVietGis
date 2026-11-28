//! E14 self-service developer publisher portal, signed submissions, and review queue.

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::extensions::marketplace::{DeveloperTx, DEVELOPER_QUOTA};
use ovg_storage::extensions::semantic_eq;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::CatalogError;
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

use super::validation::{
    decode_ed25519_public_key, load_catalog, marketplace_canonical, validate_catalog,
    verify_ed25519, CatalogRelease, MARKETPLACE_SCHEMA,
};
use super::{authenticate_human_json, decode_body, parse_uuid, ExtensionError};

const BASE: &str = "/developer/marketplace";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/me"), get(my_developer_state))
        .route(&format!("{BASE}/publisher"), post(register_publisher))
        .route(&format!("{BASE}/publisher/rotate-key"), post(rotate_key))
        .route(&format!("{BASE}/submissions"), post(submit_release))
        .route(
            &format!("{BASE}/submissions/{{submission_id}}/withdraw"),
            post(withdraw_submission),
        )
        .route(&format!("{BASE}/review-queue"), get(review_queue))
        .route(
            &format!("{BASE}/review-queue/{{submission_id}}/decision"),
            post(decide_submission),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublisherBody {
    name: String,
    public_key: String,
    proof: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmissionBody {
    publisher_id: String,
    manifest: Value,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionBody {
    decision: String,
    note: String,
}

#[derive(Debug, Deserialize)]
struct ReviewQueueQuery {
    #[serde(default = "default_state")]
    state: String,
    #[serde(default)]
    offset: i64,
}

fn default_state() -> String {
    "pending".to_owned()
}

fn private_no_store<T: IntoResponse>(inner: T) -> Response {
    ([(header::CACHE_CONTROL, "private, no-store")], inner).into_response()
}

fn can_review(state: &AppState, uid: Uuid) -> bool {
    state.config.marketplace_reviewer_user_ids.contains(&uid)
}

fn catalog_or_unavailable(state: &AppState) -> (Vec<CatalogRelease>, bool) {
    match load_catalog(&state.config) {
        Ok(items) => (items, true),
        Err(_) => (Vec::new(), false),
    }
}

fn is_published(
    row: &ovg_storage::extensions::marketplace::SubmissionRow,
    catalog: &[CatalogRelease],
) -> bool {
    row.status == "approved"
        && catalog.iter().any(|item| {
            item.publisher_id == row.publisher_id
                && item.signature == row.signature
                && semantic_eq(&item.manifest, &row.manifest)
        })
}

fn db_pool(state: &AppState) -> Result<&ovg_storage::DbPool, ExtensionError> {
    state.db_pool.as_ref().ok_or_else(|| {
        ExtensionError::Domain(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })
}

async fn my_developer_state(
    State(state): State<AppState>,
    auth: HumanSession,
) -> Result<Response, ExtensionError> {
    let uid = auth.actor_id();
    let mut tx = DeveloperTx::begin(db_pool(&state)?, uid, auth.session_id(), false)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    let publisher = tx.get_publisher().await.map_err(ExtensionError::Domain)?;
    let rows = tx
        .list_own_submissions()
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;

    let (catalog, available) = catalog_or_unavailable(&state);
    let submissions: Vec<Value> = rows
        .iter()
        .map(|r| r.wire(uid, is_published(r, &catalog)))
        .collect();
    Ok(private_no_store(Json(json!({
        "user_id": uid,
        "publisher": publisher.map(|p| p.wire()),
        "submissions": submissions,
        "can_review": can_review(&state, uid),
        "catalog_available": available,
        "quota": DEVELOPER_QUOTA,
    }))))
}

async fn register_publisher(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (auth, headers, body) = authenticate_human_json(&state, request).await?;
    require_csrf(&state, &auth, &headers)
        .map_err(|e| ExtensionError::Access(CatalogError::Identity(e)))?;
    let body: PublisherBody = decode_body(body)?;
    let name_len = body.name.chars().count();
    if !(1..=120).contains(&name_len) || body.public_key.len() != 44 || body.proof.len() != 88 {
        return Err(ExtensionError::InvalidRequest);
    }
    let uid = auth.actor_id();
    let trimmed = body.name.trim();
    if trimmed.is_empty() || trimmed != body.name {
        return Err(ExtensionError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "invalid publisher key proof",
        )));
    }
    let proof_payload = json!({
        "purpose": "openvietgis.publisher.v1",
        "user_id": uid.to_string(),
        "name": body.name,
        "public_key": body.public_key,
    });
    if decode_ed25519_public_key(&body.public_key).is_err()
        || verify_ed25519(&body.public_key, &proof_payload, &body.proof).is_err()
    {
        return Err(ExtensionError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "invalid publisher key proof",
        )));
    }

    let mut tx = DeveloperTx::begin(db_pool(&state)?, uid, auth.session_id(), false)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    tx.lock_user().await.map_err(ExtensionError::Domain)?;
    if tx
        .get_publisher()
        .await
        .map_err(ExtensionError::Domain)?
        .is_some()
    {
        return Err(ExtensionError::Domain(AppError::conflict(
            "CONFLICT",
            "publisher already registered",
        )));
    }
    let pid = format!("dev_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let pub_row = tx
        .insert_publisher(&pid, &body.name, &body.public_key)
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(private_no_store((
        StatusCode::CREATED,
        Json(pub_row.wire()),
    )))
}

async fn submit_release(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (auth, headers, body) = authenticate_human_json(&state, request).await?;
    require_csrf(&state, &auth, &headers)
        .map_err(|e| ExtensionError::Access(CatalogError::Identity(e)))?;
    let body: SubmissionBody = decode_body(body)?;
    if !(2..=32).contains(&body.publisher_id.chars().count())
        || !body.manifest.is_object()
        || body.signature.len() != 88
    {
        return Err(ExtensionError::InvalidRequest);
    }

    let uid = auth.actor_id();
    let mut tx = DeveloperTx::begin(db_pool(&state)?, uid, auth.session_id(), false)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    tx.lock_user().await.map_err(ExtensionError::Domain)?;
    let publisher = tx
        .get_publisher()
        .await
        .map_err(ExtensionError::Domain)?
        .ok_or_else(|| ExtensionError::Domain(AppError::forbidden("register publisher first")))?;
    if publisher.id != body.publisher_id {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "publisher ownership required",
        )));
    }

    let invalid_sub = || {
        ExtensionError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "invalid signed submission",
        ))
    };
    let manifest_bytes = marketplace_canonical(&body.manifest).map_err(|()| invalid_sub())?;
    if manifest_bytes.len() > 24_000 {
        return Err(invalid_sub());
    }
    let synthetic = json!({
        "schema": MARKETPLACE_SCHEMA,
        "publishers": [{
            "id": publisher.id,
            "name": publisher.name,
            "public_key": publisher.public_key,
        }],
        "releases": [{
            "publisher_id": body.publisher_id,
            "manifest": body.manifest,
            "signature": body.signature,
        }],
    });
    validate_catalog(&synthetic).map_err(|_| invalid_sub())?;

    let count = tx
        .count_own_submissions()
        .await
        .map_err(ExtensionError::Domain)?;
    if count >= DEVELOPER_QUOTA {
        return Err(ExtensionError::Domain(AppError::conflict(
            "CONFLICT",
            "submission quota reached; contact operator",
        )));
    }

    let ext_key = body.manifest["extension_key"].as_str().unwrap_or_default();
    let version = body.manifest["version"].as_str().unwrap_or_default();
    let inserted = tx
        .insert_submission(
            Uuid::new_v4(),
            &publisher.id,
            ext_key,
            version,
            &body.manifest,
            &body.signature,
        )
        .await
        .map_err(ExtensionError::Domain)?
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::conflict(
                "CONFLICT",
                "version already submitted; use a new version",
            ))
        })?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(private_no_store((
        StatusCode::CREATED,
        Json(inserted.wire(uid, false)),
    )))
}

async fn withdraw_submission(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: axum::http::HeaderMap,
    Path(submission): Path<String>,
) -> Result<Response, ExtensionError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|e| ExtensionError::Access(CatalogError::Identity(e)))?;
    let submission_id = parse_uuid(&submission)?;
    let uid = auth.actor_id();
    let mut tx = DeveloperTx::begin(db_pool(&state)?, uid, auth.session_id(), false)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    let row = tx
        .withdraw_submission(submission_id)
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(private_no_store(Json(row.wire(uid, false))))
}

async fn review_queue(
    State(state): State<AppState>,
    auth: HumanSession,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (parts, _) = request.into_parts();
    let Query(query) = Query::<ReviewQueueQuery>::try_from_uri(&parts.uri)
        .map_err(|_| ExtensionError::InvalidRequest)?;
    if !matches!(
        query.state.as_str(),
        "pending" | "approved" | "rejected" | "withdrawn"
    ) || !(0..=100_000).contains(&query.offset)
    {
        return Err(ExtensionError::InvalidRequest);
    }

    let uid = auth.actor_id();
    if !can_review(&state, uid) {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "platform reviewer required",
        )));
    }

    let mut tx = DeveloperTx::begin(db_pool(&state)?, uid, auth.session_id(), true)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    let rows = tx
        .list_review_queue(&query.state, query.offset)
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;

    let (catalog, available) = catalog_or_unavailable(&state);
    let next_offset = (rows.len() > 100).then_some(query.offset + 100);
    let items: Vec<Value> = rows
        .iter()
        .take(100)
        .map(|r| r.wire(uid, is_published(r, &catalog)))
        .collect();
    Ok(private_no_store(Json(json!({
        "items": items,
        "next_offset": next_offset,
        "catalog_available": available,
    }))))
}

async fn decide_submission(
    State(state): State<AppState>,
    Path(submission): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (auth, headers, body) = authenticate_human_json(&state, request).await?;
    require_csrf(&state, &auth, &headers)
        .map_err(|e| ExtensionError::Access(CatalogError::Identity(e)))?;
    let submission_id = parse_uuid(&submission)?;
    let body: DecisionBody = decode_body(body)?;
    if !matches!(body.decision.as_str(), "approved" | "rejected")
        || !(1..=2000).contains(&body.note.chars().count())
    {
        return Err(ExtensionError::InvalidRequest);
    }

    let uid = auth.actor_id();
    if !can_review(&state, uid) {
        return Err(ExtensionError::Domain(AppError::forbidden(
            "platform reviewer required",
        )));
    }
    let note = body.note.trim();
    if note.is_empty() {
        return Err(ExtensionError::Domain(AppError::validation(
            "VALIDATION_ERROR",
            "review note required",
        )));
    }

    let mut tx = DeveloperTx::begin(db_pool(&state)?, uid, auth.session_id(), true)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    let row = tx
        .decide_submission(submission_id, &body.decision, note)
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;

    let (catalog, _) = catalog_or_unavailable(&state);
    Ok(private_no_store(Json(
        row.wire(uid, is_published(&row, &catalog)),
    )))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RotationBody {
    publisher_id: String,
    expected_revision: i64,
    old_public_key: String,
    new_public_key: String,
    old_proof: String,
    new_proof: String,
}
fn verify_rotation(uid: Uuid, b: &RotationBody) -> Result<(), AppError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::Signature;
    let deny = || AppError::validation("INVALID_KEY_ROTATION", "invalid dual signing proof");
    if b.expected_revision <= 0
        || b.expected_revision >= 9_007_199_254_740_991
        || b.old_public_key == b.new_public_key
        || b.publisher_id.len() > 32
    {
        return Err(deny());
    }
    let payload = ovg_module_sdk::release_v2::canonical(
        &json!({"purpose":"openvietgis.publisher.rotate.v1","user_id":uid,"publisher_id":b.publisher_id,
   "expected_revision":b.expected_revision,"old_public_key":b.old_public_key,"new_public_key":b.new_public_key}),
    )?;
    for (k, s) in [
        (&b.old_public_key, &b.old_proof),
        (&b.new_public_key, &b.new_proof),
    ] {
        let key = decode_ed25519_public_key(k).map_err(|_| deny())?;
        if key.is_weak() || s.len() != 88 {
            return Err(deny());
        }
        let raw = STANDARD.decode(s).map_err(|_| deny())?;
        if STANDARD.encode(&raw) != *s {
            return Err(deny());
        }
        let sig = Signature::from_slice(&raw).map_err(|_| deny())?;
        key.verify_strict(&payload, &sig).map_err(|_| deny())?;
    }
    Ok(())
}
async fn rotate_key(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Response, ExtensionError> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 4096)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let (auth, headers, body) = authenticate_human_json(
        &state,
        Request::from_parts(parts, axum::body::Body::from(bytes)),
    )
    .await?;
    require_csrf(&state, &auth, &headers)
        .map_err(|e| ExtensionError::Access(CatalogError::Identity(e)))?;
    let b: RotationBody = decode_body(body)?;
    verify_rotation(auth.actor_id(), &b).map_err(ExtensionError::Domain)?;
    let mut tx = DeveloperTx::begin(db_pool(&state)?, auth.actor_id(), auth.session_id(), false)
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    let row = tx
        .rotate_key(
            &b.publisher_id,
            b.expected_revision,
            &b.old_public_key,
            &b.new_public_key,
            &b.old_proof,
            &b.new_proof,
        )
        .await
        .map_err(ExtensionError::Domain)?;
    tx.commit().await.map_err(ExtensionError::Domain)?;
    Ok(private_no_store(Json(row.wire())))
}
#[cfg(test)]
mod rotation_tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn rotation_binds_both_keys_and_revision() {
        let uid = Uuid::new_v4();
        let a = SigningKey::from_bytes(&[91; 32]);
        let b = SigningKey::from_bytes(&[92; 32]);
        let mut r = RotationBody {
            publisher_id: "dev_aaaaaaaaaaaaaaaaaaaaaaaa".into(),
            expected_revision: 1,
            old_public_key: STANDARD.encode(a.verifying_key().as_bytes()),
            new_public_key: STANDARD.encode(b.verifying_key().as_bytes()),
            old_proof: String::new(),
            new_proof: String::new(),
        };
        let payload=ovg_module_sdk::release_v2::canonical(&json!({"purpose":"openvietgis.publisher.rotate.v1","user_id":uid,"publisher_id":r.publisher_id,
    "expected_revision":1,"old_public_key":r.old_public_key,"new_public_key":r.new_public_key})).unwrap();
        r.old_proof = STANDARD.encode(a.sign(&payload).to_bytes());
        r.new_proof = STANDARD.encode(b.sign(&payload).to_bytes());
        assert!(verify_rotation(uid, &r).is_ok());
        assert!(verify_rotation(Uuid::new_v4(), &r).is_err());
        r.expected_revision = 2;
        assert!(verify_rotation(uid, &r).is_err());
        r.expected_revision = 1;
        r.new_proof = r.old_proof.clone();
        assert!(verify_rotation(uid, &r).is_err());
    }
}
