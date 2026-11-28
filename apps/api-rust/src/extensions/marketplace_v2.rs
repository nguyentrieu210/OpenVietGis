//! Additive, opt-in reviewed V2 metadata delivery. V1 registration and activation
//! remain canonical. Runtime delivery is scoped separately.
use super::{
    validation::{decode_ed25519_public_key, load_catalog, marketplace_canonical, CatalogRelease},
    ExtensionError,
};
use crate::{config::AppConfig, AppState};
use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::Signature;
use ovg_core::AppError;
use ovg_module_sdk::release_v2::{verify_bundle, Bundle, ContributionEnvelope, ReleaseTrust};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;

const MAX_FILE: u64 = 16 * 1024 * 1024;
static VERIFY_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
fn invalid() -> AppError {
    AppError::validation("INVALID_PLUGIN_CATALOG", "invalid or untrusted v2 catalog")
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema: String,
    epoch: u64,
    releases: Vec<Release>,
    catalog_signature: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Release {
    envelope: ContributionEnvelope,
    publisher_signature: String,
    operator_signature: String,
    contribution: String,
    assets: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    pub host: String,
    pub api: String,
}

pub(super) struct VerifiedRelease {
    pub metadata: Value,
    pub document: Vec<u8>,
}
pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/marketplace/v2/catalog", get(catalog))
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
fn signature(s: &str) -> Result<Signature, AppError> {
    Signature::from_slice(&bytes(s, 64)?).map_err(|_| invalid())
}
fn catalog_membership(c: &Catalog) -> Result<Vec<u8>, AppError> {
    use ovg_module_sdk::release_v2::canonical;
    let mut members = Vec::new();
    for r in &c.releases {
        members.push(json!({"envelope_digest":hash(&canonical(&r.envelope)?),
            "publisher_signature":r.publisher_signature,"operator_signature":r.operator_signature}));
    }
    canonical(
        &json!({"schema":"openvietgis.marketplace.membership.v2","epoch":c.epoch,"members":members}),
    )
}
fn verified_items(
    c: Catalog,
    existing: &[CatalogRelease],
    operator: &str,
    epoch: u64,
    profile: &Profile,
) -> Result<Vec<VerifiedRelease>, AppError> {
    if c.schema != "openvietgis.marketplace.v2"
        || c.epoch != epoch
        || epoch == 0
        || epoch > 9_007_199_254_740_991
        || c.releases.len() > 100
    {
        return Err(invalid());
    }
    if !matches!(
        profile.host.as_str(),
        "web" | "android" | "ios" | "windows" | "macos" | "linux" | "server"
    ) || profile.api.len() > 64
    {
        return Err(invalid());
    }
    let api = Version::parse(&profile.api).map_err(|_| invalid())?;
    let operator = decode_ed25519_public_key(operator).map_err(|_| invalid())?;
    operator
        .verify_strict(&catalog_membership(&c)?, &signature(&c.catalog_signature)?)
        .map_err(|_| invalid())?;
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for r in c.releases {
        let e = &r.envelope;
        e.validate()?;
        if !seen.insert((e.extension_key.clone(), e.version.clone())) {
            return Err(invalid());
        }
        let legacy = existing
            .iter()
            .find(|v| {
                v.publisher_id == e.publisher_id
                    && v.manifest["extension_key"].as_str() == Some(&e.extension_key)
                    && v.manifest["version"].as_str() == Some(&e.version)
            })
            .ok_or_else(invalid)?;
        let publisher =
            decode_ed25519_public_key(&legacy.publisher_public_key).map_err(|_| invalid())?;
        if publisher == operator {
            return Err(invalid());
        }
        let manifest_digest =
            hash(&marketplace_canonical(&legacy.manifest).map_err(|_| invalid())?);
        let key_id = hash(publisher.as_bytes());
        let mut allowed: BTreeSet<String> =
            serde_json::from_value(legacy.manifest["permissions"].clone())
                .map_err(|_| invalid())?;
        // V1 partner capabilities are deliberately not extension permissions.
        // Derive only reviewed read-only capabilities; GIS reads additionally require manager approval.
        if let Some(caps) = legacy.manifest["capabilities"].as_array() {
            for cap in ["host_context", "host_notice", "map_view"] {
                if caps.iter().any(|v| v.as_str() == Some(cap)) {
                    allowed.insert(format!("ext.{}.{cap}", e.extension_key));
                }
            }
        }
        let contribution = bytes(&r.contribution, 256 * 1024)?;
        let mut assets = BTreeMap::new();
        let mut total = 0usize;
        for (id, encoded) in &r.assets {
            let a = bytes(encoded, 10 * 1024 * 1024)?;
            total = total.checked_add(a.len()).ok_or_else(invalid)?;
            if total > 10 * 1024 * 1024 {
                return Err(invalid());
            }
            assets.insert(id.clone(), a);
        }
        let ps = signature(&r.publisher_signature)?;
        let os = signature(&r.operator_signature)?;
        let trust = ReleaseTrust {
            publisher_id: &legacy.publisher_id,
            extension_key: legacy.manifest["extension_key"]
                .as_str()
                .ok_or_else(invalid)?,
            version: legacy.manifest["version"].as_str().ok_or_else(invalid)?,
            key_id: &key_id,
            publisher_key: &publisher,
            operator_key: &operator,
            manifest_digest: &manifest_digest,
            allowed_permissions: &allowed,
            minimum_epoch: epoch,
            revoked: false,
        };
        let b = Bundle {
            envelope: e,
            publisher_signature: &ps,
            operator_signature: &os,
            epoch,
            contribution: &contribution,
            assets: &assets,
        };
        // Incompatible releases are not exposed. Never make a signature claim
        // for a skipped release.
        if !e.platforms.contains(&profile.host)
            || !VersionReq::parse(&e.host_api_range)
                .map_err(|_| invalid())?
                .matches(&api)
        {
            continue;
        }
        let digest = verify_bundle(&b, &trust, &profile.host, &api)?;
        // A signature is not runtime qualification. Only expose metadata;
        // contribution and assets remain server-side until broker/host integration.
        items.push(VerifiedRelease { metadata: json!({"envelope":e,"release_digest":digest,"catalog_epoch":epoch,"runtime_available":false,
            "publisher_signature":r.publisher_signature,"operator_signature":r.operator_signature}), document: contribution });
    }
    items.sort_by(|a, b| {
        a.metadata["envelope"]["extension_key"]
            .as_str()
            .cmp(&b.metadata["envelope"]["extension_key"].as_str())
            .then_with(|| {
                a.metadata["envelope"]["version"]
                    .as_str()
                    .cmp(&b.metadata["envelope"]["version"].as_str())
            })
    });
    Ok(items)
}
fn load(config: &AppConfig, p: &Profile) -> Result<Vec<VerifiedRelease>, AppError> {
    let Some(path) = config.marketplace_v2_catalog_path.as_deref() else {
        return Ok(vec![]);
    };
    let epoch = config.marketplace_v2_epoch.ok_or_else(invalid)?;
    let key = config
        .marketplace_operator_public_key
        .as_deref()
        .ok_or_else(invalid)?;
    let mut raw = Vec::new();
    File::open(path)
        .map_err(|_| invalid())?
        .take(MAX_FILE + 1)
        .read_to_end(&mut raw)
        .map_err(|_| invalid())?;
    if raw.len() as u64 > MAX_FILE {
        return Err(invalid());
    }
    let c: Catalog = serde_json::from_slice(&raw).map_err(|_| invalid())?;
    verified_items(c, &load_catalog(config)?, key, epoch, p)
}
pub(super) async fn load_verified(
    config: std::sync::Arc<AppConfig>,
    profile: Profile,
) -> Result<Vec<VerifiedRelease>, ExtensionError> {
    let slot = VERIFY_SLOTS.try_acquire().map_err(|_| {
        ExtensionError::Domain(AppError::service_unavailable(
            "PLUGIN_CATALOG_BUSY",
            "catalog verification capacity exhausted",
        ))
    })?;
    tokio::task::spawn_blocking(move || {
        let _slot = slot;
        load(&config, &profile)
    })
    .await
    .map_err(|_| ExtensionError::Domain(invalid()))?
    .map_err(ExtensionError::Domain)
}
async fn catalog(
    State(state): State<AppState>,
    Query(profile): Query<Profile>,
) -> Result<Response, ExtensionError> {
    let runtime_enabled = state.config.marketplace_v2_runtime_enabled
        && profile.host == "web"
        && profile.api == "1.0.0";
    let items: Vec<Value> = load_verified(state.config.clone(), profile)
        .await?
        .into_iter()
        .map(|r| r.metadata)
        .collect();
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"items":items,"profile":"metadata_only","web_read_only_enabled":runtime_enabled})),
    )
        .into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use ovg_module_sdk::release_v2::canonical;
    fn fixture() -> (Catalog, Vec<CatalogRelease>, String, Profile) {
        let pk = SigningKey::from_bytes(&[3; 32]);
        let op = SigningKey::from_bytes(&[4; 32]);
        let manifest =
            json!({"extension_key":"partner_pub_test","version":"1.0.0","permissions":[]});
        let contribution = b"{\"slot\":\"form\",\"widgets\":[],\"actions\":[]}";
        let e = ContributionEnvelope {
            schema_version: 2,
            publisher_id: "pub".into(),
            extension_key: "partner_pub_test".into(),
            version: "1.0.0".into(),
            manifest_digest: hash(&marketplace_canonical(&manifest).unwrap()),
            host_api_range: "^1".into(),
            platforms: vec!["web".into()],
            permissions: vec![],
            contribution_digest: hash(contribution),
            assets: vec![],
            dependencies: vec![],
            issued_at: "2026-11-29T00:00:00Z".into(),
            key_id: hash(pk.verifying_key().as_bytes()),
        };
        let encoded = canonical(&e).unwrap();
        let ps = STANDARD.encode(pk.sign(&encoded).to_bytes());
        let review = canonical(
            &json!({"schema":"openvietgis.review.v2","envelope_digest":hash(&encoded),"epoch":2}),
        )
        .unwrap();
        let os = STANDARD.encode(op.sign(&review).to_bytes());
        let mut catalog = Catalog {
            catalog_signature: String::new(),
            schema: "openvietgis.marketplace.v2".into(),
            epoch: 2,
            releases: vec![Release {
                envelope: e,
                publisher_signature: ps,
                operator_signature: os,
                contribution: STANDARD.encode(contribution),
                assets: BTreeMap::new(),
            }],
        };
        catalog.catalog_signature =
            STANDARD.encode(op.sign(&catalog_membership(&catalog).unwrap()).to_bytes());
        (
            catalog,
            vec![CatalogRelease {
                publisher_id: "pub".into(),
                publisher_name: "Pub".into(),
                publisher_public_key: STANDARD.encode(pk.verifying_key().as_bytes()),
                manifest,
                signature: String::new(),
            }],
            STANDARD.encode(op.verifying_key().as_bytes()),
            Profile {
                host: "web".into(),
                api: "1.0.0".into(),
            },
        )
    }
    #[test]
    fn python_signed_reference_catalog_verifies_all_releases() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/spec075/python-signed-catalog.json"
        ))
        .unwrap();
        let key = fixture["operator_public_key"].as_str().unwrap();
        let legacy = &fixture["v1"];
        super::super::validation::verify_ed25519(
            key,
            &legacy["payload"],
            legacy["signature"].as_str().unwrap(),
        )
        .unwrap();
        let existing = super::super::validation::validate_catalog(&legacy["payload"]).unwrap();
        let c: Catalog = serde_json::from_value(fixture["v2"].clone()).unwrap();
        let rows = verified_items(
            c,
            &existing,
            key,
            2,
            &Profile {
                host: "web".into(),
                api: "1.0.0".into(),
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        for row in rows {
            let doc = ovg_module_sdk::declarative::Document::parse(&row.document).unwrap();
            assert!(!doc.actions.is_empty());
            assert_eq!(row.metadata["runtime_available"], false);
        }
    }
    #[test]
    fn v2_metadata_uses_existing_catalog_trust() {
        let (c, existing, key, p) = fixture();
        let rows = verified_items(c, &existing, &key, 2, &p).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].metadata["runtime_available"], false);
        let (mut c, existing, key, p) = fixture();
        c.releases.clear();
        assert!(verified_items(c, &existing, &key, 2, &p).is_err());
        let (c, _, key, p) = fixture();
        assert!(verified_items(c, &[], &key, 2, &p).is_err());
        let (c, existing, key, p) = fixture();
        assert!(verified_items(c, &existing, &key, 3, &p).is_err());
        let (mut c, existing, key, p) = fixture();
        c.releases[0].contribution = STANDARD.encode(b"bad");
        assert!(verified_items(c, &existing, &key, 2, &p).is_err());
    }
}
