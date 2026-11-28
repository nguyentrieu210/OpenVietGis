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
use ovg_module_sdk::release_v2::{
    verify_bundle_integrity, Bundle, ContributionEnvelope, ReleaseTrust,
};
use ovg_module_sdk::trust::{verify_policy, TrustPolicy, VerifiedTrust};
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
    pub manifest: Value,
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
    // Index the already-validated V1 identities once per catalog.
    let mut indexed = BTreeMap::new();
    for row in existing {
        let key = row.manifest["extension_key"].as_str().ok_or_else(invalid)?;
        let version = row.manifest["version"].as_str().ok_or_else(invalid)?;
        if indexed
            .insert((row.publisher_id.as_str(), key, version), row)
            .is_some()
        {
            return Err(invalid());
        }
    }
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for r in c.releases {
        let e = &r.envelope;
        e.validate()?;
        if !seen.insert((e.extension_key.clone(), e.version.clone())) {
            return Err(invalid());
        }
        let legacy = *indexed
            .get(&(
                e.publisher_id.as_str(),
                e.extension_key.as_str(),
                e.version.as_str(),
            ))
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
            if caps.iter().any(|v| v.as_str() == Some("map_view")) {
                allowed.insert(format!("ext.{}.processing_plan", e.extension_key));
            }
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
        // Authenticate all members before admitting the global checkpoint,
        // even when the requested host cannot run a particular release.
        let digest = verify_bundle_integrity(&b, &trust)?;
        if !e.platforms.contains(&profile.host)
            || !VersionReq::parse(&e.host_api_range)
                .map_err(|_| invalid())?
                .matches(&api)
        {
            continue;
        }
        // A signature is not runtime qualification. Only expose metadata;
        // contribution and assets remain server-side until broker/host integration.
        items.push(VerifiedRelease { metadata: json!({"envelope":e,"release_digest":digest,"catalog_epoch":epoch,"runtime_available":false,
            "publisher_signature":r.publisher_signature,"operator_signature":r.operator_signature}), document: contribution, manifest:legacy.manifest.clone() });
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedPolicy {
    policy: TrustPolicy,
    signature: String,
}
fn bounded_file(path: &str, max: u64) -> Result<Vec<u8>, AppError> {
    let mut raw = Vec::new();
    File::open(path)
        .map_err(|_| invalid())?
        .take(max + 1)
        .read_to_end(&mut raw)
        .map_err(|_| invalid())?;
    if raw.len() as u64 > max {
        return Err(invalid());
    }
    Ok(raw)
}
fn load(
    config: &AppConfig,
    p: &Profile,
) -> Result<(Vec<VerifiedRelease>, Option<VerifiedTrust>), AppError> {
    let Some(path) = config.marketplace_v2_catalog_path.as_deref() else {
        if config.marketplace_v2_runtime_enabled {
            return Err(invalid());
        }
        return Ok((vec![], None));
    };
    let epoch = config.marketplace_v2_epoch.ok_or_else(invalid)?;
    let key = config
        .marketplace_operator_public_key
        .as_deref()
        .ok_or_else(invalid)?;
    let root = decode_ed25519_public_key(
        config
            .marketplace_trust_root_public_key
            .as_deref()
            .ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    let operator = decode_ed25519_public_key(key).map_err(|_| invalid())?;
    let c: Catalog =
        serde_json::from_slice(&bounded_file(path, MAX_FILE)?).map_err(|_| invalid())?;
    let membership = hash(&catalog_membership(&c)?);
    let signed: SignedPolicy = serde_json::from_slice(&bounded_file(
        config
            .marketplace_trust_policy_path
            .as_deref()
            .ok_or_else(invalid)?,
        1048576,
    )?)
    .map_err(|_| invalid())?;
    let trust = verify_policy(
        signed.policy,
        &signature(&signed.signature)?,
        &root,
        &operator,
        epoch,
        &membership,
        chrono::Utc::now(),
    )?;
    let legacy = load_catalog(config)?;
    if legacy.iter().any(|r| {
        decode_ed25519_public_key(&r.publisher_public_key)
            .ok()
            .as_ref()
            == Some(&root)
    }) {
        return Err(invalid());
    }
    let mut items = verified_items(c, &legacy, key, epoch, p)?;
    items.retain(|r| {
        trust.permits(
            r.metadata["release_digest"].as_str().unwrap_or_default(),
            r.metadata["envelope"]["key_id"]
                .as_str()
                .unwrap_or_default(),
        )
    });
    if let Some(policy) = &trust.policy().commercial {
        // The root pins the license authority, and it must not double as a publisher.
        if legacy.iter().any(|r| {
            decode_ed25519_public_key(&r.publisher_public_key)
                .ok()
                .is_some_and(|k| hash(k.as_bytes()) == policy.authority_key_id)
        }) {
            return Err(invalid());
        }
        for item in &mut items {
            if policy.requires(item.metadata["release_digest"].as_str().unwrap_or_default()) {
                item.metadata["commercial_policy"] =
                    serde_json::to_value(policy).map_err(|_| invalid())?;
            }
        }
    }
    Ok((items, Some(trust)))
}
pub(super) async fn load_verified(
    state: &AppState,
    profile: Profile,
) -> Result<Vec<VerifiedRelease>, ExtensionError> {
    // The bounded slot spans verification and admission, including when a
    // caller holds its existing project transaction. No plugin-controlled
    // global lock is held across the scoped command.
    let slot = VERIFY_SLOTS.try_acquire().map_err(|_| {
        ExtensionError::Domain(AppError::service_unavailable(
            "PLUGIN_CATALOG_BUSY",
            "catalog verification capacity exhausted",
        ))
    })?;
    let config = state.config.clone();
    let (result, slot) = tokio::task::spawn_blocking(move || (load(&config, &profile), slot))
        .await
        .map_err(|_| ExtensionError::Domain(invalid()))?;
    let (items, trust) = result.map_err(ExtensionError::Domain)?;
    if let Some(trust) = trust {
        let pool = state.db_pool.as_ref().ok_or_else(|| {
            ExtensionError::Domain(AppError::service_unavailable(
                "PLUGIN_TRUST_UNAVAILABLE",
                "global trust checkpoint unavailable",
            ))
        })?;
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            ovg_storage::extensions::trust::admit_policy(pool, &trust),
        )
        .await
        .map_err(|_| {
            ExtensionError::Domain(AppError::service_unavailable(
                "PLUGIN_TRUST_BUSY",
                "global trust checkpoint unavailable",
            ))
        })?
        .map_err(ExtensionError::Domain)?;
    }
    drop(slot);
    Ok(items)
}
async fn catalog(
    State(state): State<AppState>,
    Query(profile): Query<Profile>,
) -> Result<Response, ExtensionError> {
    let platform = ovg_module_sdk::host_profile::Platform::parse(&profile.host)
        .map_err(ExtensionError::Domain)?;
    let availability = ovg_module_sdk::host_profile::availability(
        platform,
        &profile.api,
        state.config.marketplace_v2_runtime_enabled,
    );
    let runtime_enabled = availability.online_execution;
    let items: Vec<Value> = load_verified(&state, profile)
        .await?
        .into_iter()
        .map(|r| {
            let mut m = r.metadata;
            let required = m
                .as_object_mut()
                .and_then(|v| v.remove("commercial_policy"))
                .is_some();
            m["license_required"] = json!(required);
            m
        })
        .collect();
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"items":items,"profile":"metadata_only","web_read_only_enabled":runtime_enabled,"host_availability":availability})),
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
        assert_eq!(rows.len(), 4);
        for row in rows {
            let doc = ovg_module_sdk::declarative::Document::parse(&row.document).unwrap();
            assert!(!doc.actions.is_empty());
            assert_eq!(row.metadata["runtime_available"], false);
        }
    }
    #[test]
    fn configured_catalog_requires_trust_and_filters_revoked_release_and_key() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/spec075/python-signed-catalog.json"
        ))
        .unwrap();
        let dir = std::env::temp_dir().join(format!("ovg075-trust-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let v1 = dir.join("v1.json");
        let v2 = dir.join("v2.json");
        let trust = dir.join("trust.json");
        std::fs::write(&v1, serde_json::to_vec(&fixture["v1"]).unwrap()).unwrap();
        std::fs::write(&v2, serde_json::to_vec(&fixture["v2"]).unwrap()).unwrap();
        let root = SigningKey::from_bytes(&[77; 32]);
        let mut cfg = AppConfig::default();
        cfg.marketplace_catalog_path = Some(v1.to_string_lossy().into_owned());
        cfg.marketplace_v2_catalog_path = Some(v2.to_string_lossy().into_owned());
        cfg.marketplace_v2_epoch = Some(2);
        cfg.marketplace_operator_public_key =
            Some(fixture["operator_public_key"].as_str().unwrap().into());
        let p = Profile {
            host: "web".into(),
            api: "1.0.0".into(),
        };
        assert!(load(&cfg, &p).is_err());
        cfg.marketplace_trust_root_public_key =
            Some(STANDARD.encode(root.verifying_key().as_bytes()));
        cfg.marketplace_trust_policy_path = Some(trust.to_string_lossy().into_owned());
        let c: Catalog = serde_json::from_value(fixture["v2"].clone()).unwrap();
        let key =
            decode_ed25519_public_key(cfg.marketplace_operator_public_key.as_deref().unwrap())
                .unwrap();
        let now = chrono::Utc::now();
        let mut policy = TrustPolicy {
            schema: "openvietgis.trust.policy.v1".into(),
            sequence: 1,
            catalog_epoch: 2,
            catalog_membership_digest: hash(&catalog_membership(&c).unwrap()),
            operator_key_id: hash(key.as_bytes()),
            issued_at: (now - chrono::Duration::seconds(1)).to_rfc3339(),
            expires_at: (now + chrono::Duration::days(1)).to_rfc3339(),
            revoked_release_digests: vec![hash(&canonical(&c.releases[0].envelope).unwrap())],
            revoked_publisher_keys: vec![],
            revoked_operator_keys: vec![],
            commercial: None,
        };
        let save = |p: &TrustPolicy| {
            std::fs::write(&trust,serde_json::to_vec(&json!({"policy":p,"signature":STANDARD.encode(root.sign(&canonical(p).unwrap()).to_bytes())})).unwrap()).unwrap()
        };
        save(&policy);
        let (rows, proof) = load(&cfg, &p).unwrap();
        assert_eq!(rows.len(), 3);
        let old = proof.unwrap();
        let license_key = SigningKey::from_bytes(&[104; 32]);
        policy.sequence = 2;
        policy.commercial = Some(ovg_module_sdk::entitlement::CommercialPolicy {
            audience: "fixture-installation".into(),
            authority_key_id: hash(license_key.verifying_key().as_bytes()),
            licensed_release_digests: vec![hash(&canonical(&c.releases[1].envelope).unwrap())],
            revocation_floors: vec![],
        });
        save(&policy);
        let (licensed, proof) = load(&cfg, &p).unwrap();
        assert_eq!(
            licensed
                .iter()
                .filter(|r| r.metadata.get("commercial_policy").is_some())
                .count(),
            1
        );
        let proof = proof.unwrap();
        assert!(proof
            .ensure_successor(old.policy(), old.digest(), old.root_key_id())
            .is_ok());
        let mut stripped = policy.clone();
        stripped.sequence = 3;
        stripped.commercial = None;
        save(&stripped);
        let (_, next) = load(&cfg, &p).unwrap();
        assert!(next
            .unwrap()
            .ensure_successor(proof.policy(), proof.digest(), proof.root_key_id())
            .is_err());
        policy.sequence = 3;
        policy.revoked_publisher_keys = vec![c.releases[0].envelope.key_id.clone()];
        save(&policy);
        assert!(load(&cfg, &p).unwrap().0.is_empty());
        policy.catalog_membership_digest = "a".repeat(64);
        save(&policy);
        assert!(load(&cfg, &p).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn incompatible_release_is_still_verified_before_checkpoint() {
        let (c, existing, key, mut p) = fixture();
        p.host = "android".into();
        assert!(verified_items(c, &existing, &key, 2, &p)
            .unwrap()
            .is_empty());
        let (mut c, existing, key, mut p) = fixture();
        p.host = "android".into();
        c.releases[0].contribution = STANDARD.encode(b"tampered");
        assert!(verified_items(c, &existing, &key, 2, &p).is_err());
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
