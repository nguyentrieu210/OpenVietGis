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
use ovg_module_sdk::entitlement::CommercialPolicy;
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
use std::sync::Arc;

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

#[derive(Clone)]
pub(super) struct VerifiedRelease {
    pub metadata: Value,
    pub document: Vec<u8>,
    pub manifest: Value,
    pub commercial_policy: Option<Arc<CommercialPolicy>>,
}
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/marketplace/v2/catalog", get(catalog))
        .route("/marketplace/v2/search", get(search))
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
fn verified_all(
    c: Catalog,
    existing: &[CatalogRelease],
    operator: &str,
    epoch: u64,
) -> Result<Vec<VerifiedRelease>, AppError> {
    if c.schema != "openvietgis.marketplace.v2"
        || c.epoch != epoch
        || epoch == 0
        || epoch > 9_007_199_254_740_991
        || c.releases.len() > 1000
    {
        return Err(invalid());
    }
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
        // A signature is not runtime qualification. Only expose metadata;
        // contribution and assets remain server-side until broker/host integration.
        items.push(VerifiedRelease { metadata: json!({"envelope":e,"release_digest":digest,"catalog_epoch":epoch,"runtime_available":false,
            "publisher_signature":r.publisher_signature,"operator_signature":r.operator_signature}), document: contribution, manifest:legacy.manifest.clone(), commercial_policy:None });
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
fn profile_api(profile: &Profile) -> Result<Version, AppError> {
    if !matches!(
        profile.host.as_str(),
        "web" | "android" | "ios" | "windows" | "macos" | "linux" | "server"
    ) || profile.api.len() > 64
    {
        return Err(invalid());
    }
    Version::parse(&profile.api).map_err(|_| invalid())
}
fn compatible(row: &VerifiedRelease, profile: &Profile, api: &Version) -> bool {
    let e = &row.metadata["envelope"];
    e["platforms"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&profile.host)))
        && e["host_api_range"]
            .as_str()
            .and_then(|r| VersionReq::parse(r).ok())
            .is_some_and(|r| r.matches(api))
}
#[cfg(test)]
fn verified_items(
    c: Catalog,
    existing: &[CatalogRelease],
    operator: &str,
    epoch: u64,
    profile: &Profile,
) -> Result<Vec<VerifiedRelease>, AppError> {
    let api = profile_api(profile)?;
    Ok(verified_all(c, existing, operator, epoch)?
        .into_iter()
        .filter(|r| compatible(r, profile, &api))
        .collect())
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
fn load_all(config: &AppConfig) -> Result<(Vec<VerifiedRelease>, Option<VerifiedTrust>), AppError> {
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
    let mut items = verified_all(c, &legacy, key, epoch)?;
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
        let policy = Arc::new(policy.clone());
        for item in &mut items {
            if policy.requires(item.metadata["release_digest"].as_str().unwrap_or_default()) {
                item.commercial_policy = Some(Arc::clone(&policy));
            }
        }
    }
    Ok((items, Some(trust)))
}
#[cfg(test)]
fn load(
    config: &AppConfig,
    p: &Profile,
) -> Result<(Vec<VerifiedRelease>, Option<VerifiedTrust>), AppError> {
    let api = profile_api(p)?;
    let (items, trust) = load_all(config)?;
    Ok((
        items
            .into_iter()
            .filter(|r| compatible(r, p, &api))
            .collect(),
        trust,
    ))
}
/// Files are local operator-owned inputs. Unix inode/ctime/mtime/size stamps
/// detect replacement and edits. Other targets bypass cache rather than
/// relying on weaker file timestamps.
#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    path: String,
    size: u64,
    identity: (u64, u64, i64, i64, i64, i64),
}
#[cfg(unix)]
fn stamp(path: &str) -> Result<FileStamp, AppError> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).map_err(|_| invalid())?;
    if !m.is_file() {
        return Err(invalid());
    }
    Ok(FileStamp {
        path: path.into(),
        size: m.len(),
        identity: (
            m.dev(),
            m.ino(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        ),
    })
}
#[derive(PartialEq, Eq)]
struct SnapshotKey {
    files: Vec<FileStamp>,
    epoch: u64,
    operator: String,
    root: String,
}
#[cfg(unix)]
fn snapshot_key(config: &AppConfig) -> Result<Option<SnapshotKey>, AppError> {
    if config.marketplace_v2_catalog_path.is_none() {
        return Ok(None);
    }
    let files = [
        &config.marketplace_catalog_path,
        &config.marketplace_v2_catalog_path,
        &config.marketplace_trust_policy_path,
    ]
    .into_iter()
    .map(|p| stamp(p.as_deref().ok_or_else(invalid)?))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(SnapshotKey {
        files,
        epoch: config.marketplace_v2_epoch.ok_or_else(invalid)?,
        operator: config
            .marketplace_operator_public_key
            .clone()
            .ok_or_else(invalid)?,
        root: config
            .marketplace_trust_root_public_key
            .clone()
            .ok_or_else(invalid)?,
    }))
}
#[cfg(not(unix))]
fn snapshot_key(_config: &AppConfig) -> Result<Option<SnapshotKey>, AppError> {
    Ok(None)
}
struct Snapshot {
    items: Vec<VerifiedRelease>,
    search: Vec<String>,
    trust: Option<VerifiedTrust>,
}
impl Snapshot {
    fn new(items: Vec<VerifiedRelease>, trust: Option<VerifiedTrust>) -> Self {
        let search = items
            .iter()
            .map(|r| {
                ["extension_key", "display_name", "description", "version"]
                    .into_iter()
                    .filter_map(|f| r.manifest[f].as_str())
                    .chain(r.metadata["envelope"]["publisher_id"].as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
            })
            .collect();
        Self {
            items,
            search,
            trust,
        }
    }
    fn current(&self) -> Result<(), AppError> {
        if let Some(t) = &self.trust {
            let now = chrono::Utc::now();
            let p = t.policy();
            let start =
                chrono::DateTime::parse_from_rfc3339(&p.issued_at).map_err(|_| invalid())?;
            let end = chrono::DateTime::parse_from_rfc3339(&p.expires_at).map_err(|_| invalid())?;
            if now < start || now >= end {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Default)]
pub(crate) struct SnapshotCache(Arc<tokio::sync::Mutex<Option<(SnapshotKey, Arc<Snapshot>)>>>);
impl SnapshotCache {
    async fn get(&self, config: Arc<AppConfig>) -> Result<Arc<Snapshot>, AppError> {
        // Single-flight cold verification. Cancelled requests do not release
        // the guard while a blocking verification is still running.
        let mut guard = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.0.clone().lock_owned(),
        )
        .await
        .map_err(|_| {
            AppError::service_unavailable("PLUGIN_CATALOG_BUSY", "catalog snapshot pending")
        })?;
        tokio::task::spawn_blocking(move || {
            let key = snapshot_key(&config)?;
            if let (Some(k), Some((prior, s))) = (&key, guard.as_ref()) {
                if k == prior {
                    s.current()?;
                    return Ok(Arc::clone(s));
                }
            }
            *guard = None;
            let _slot = VERIFY_SLOTS.try_acquire().map_err(|_| {
                AppError::service_unavailable(
                    "PLUGIN_CATALOG_BUSY",
                    "catalog verification capacity exhausted",
                )
            })?;
            let (items, trust) = load_all(&config)?;
            if key != snapshot_key(&config)? {
                return Err(invalid());
            }
            let s = Arc::new(Snapshot::new(items, trust));
            s.current()?;
            if let Some(k) = key {
                *guard = Some((k, Arc::clone(&s)));
            }
            Ok(s)
        })
        .await
        .map_err(|_| invalid())?
    }
}
async fn admitted_snapshot(state: &AppState) -> Result<Arc<Snapshot>, ExtensionError> {
    let s = state
        .plugin_catalog_snapshot
        .get(state.config.clone())
        .await
        .map_err(ExtensionError::Domain)?;
    if let Some(t) = &s.trust {
        let pool = state.db_pool.as_ref().ok_or_else(|| {
            ExtensionError::Domain(AppError::service_unavailable(
                "PLUGIN_TRUST_UNAVAILABLE",
                "global trust checkpoint unavailable",
            ))
        })?;
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            ovg_storage::extensions::trust::admit_policy(pool, t),
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
    s.current().map_err(ExtensionError::Domain)?;
    Ok(s)
}
pub(super) async fn load_verified(
    state: &AppState,
    profile: Profile,
) -> Result<Vec<VerifiedRelease>, ExtensionError> {
    let api = profile_api(&profile).map_err(ExtensionError::Domain)?;
    let snapshot = admitted_snapshot(state).await?;
    Ok(snapshot
        .items
        .iter()
        .filter(|r| compatible(r, &profile, &api))
        .cloned()
        .collect())
}
fn public_metadata(r: &VerifiedRelease) -> Value {
    let mut m = r.metadata.clone();
    m["license_required"] = json!(r.commercial_policy.is_some());
    m
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    host: String,
    api: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
    snapshot: Option<String>,
}
async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Response, ExtensionError> {
    let limit = q.limit.unwrap_or(50);
    if q.q.chars().count() > 200 || limit == 0 || limit > 100 || q.offset > 1000 {
        return Err(ExtensionError::InvalidRequest);
    }
    let profile = Profile {
        host: q.host,
        api: q.api,
    };
    let api = profile_api(&profile).map_err(ExtensionError::Domain)?;
    let s = admitted_snapshot(&state).await?;
    let id = s.trust.as_ref().map(|t| t.digest()).unwrap_or("disabled");
    if q.snapshot.as_deref().is_some_and(|v| v != id) {
        return Err(ExtensionError::Domain(AppError::conflict(
            "CATALOG_SNAPSHOT_CHANGED",
            "catalog changed; restart search",
        )));
    }
    let result = search_page(&s, &profile, &api, &q.q, q.offset, limit, id);
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
fn search_page(
    s: &Snapshot,
    profile: &Profile,
    api: &Version,
    query: &str,
    offset: usize,
    limit: usize,
    id: &str,
) -> Value {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut total = 0usize;
    let mut items = Vec::new();
    for (row, text) in s.items.iter().zip(&s.search) {
        if !compatible(row, profile, api) || !terms.iter().all(|t| text.contains(t)) {
            continue;
        }
        if total >= offset && items.len() < limit {
            items.push(public_metadata(row));
        }
        total += 1;
    }
    let next = (offset + items.len() < total).then_some(offset + items.len());
    json!({"items":items,"total":total,"next_offset":next,"snapshot":id})
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
    let api = profile_api(&profile).map_err(ExtensionError::Domain)?;
    let snapshot = admitted_snapshot(&state).await?;
    let items: Vec<Value> = snapshot
        .items
        .iter()
        .filter(|r| compatible(r, &profile, &api))
        .map(public_metadata)
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
    #[tokio::test]
    async fn configured_catalog_requires_trust_and_filters_revoked_release_and_key() {
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
        let cache = SnapshotCache::default();
        let config = Arc::new(cfg.clone());
        let cached = cache.get(config.clone()).await.unwrap();
        let warm = cache.get(config.clone()).await.unwrap();
        #[cfg(unix)]
        assert!(Arc::ptr_eq(&cached, &warm));
        assert_eq!(warm.items.len(), 3);
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
        let refreshed = cache.get(config.clone()).await.unwrap();
        assert!(!Arc::ptr_eq(&warm, &refreshed));
        assert_eq!(
            refreshed
                .items
                .iter()
                .filter(|r| r.commercial_policy.is_some())
                .count(),
            1
        );
        let (licensed, proof) = load(&cfg, &p).unwrap();
        assert_eq!(
            licensed
                .iter()
                .filter(|r| r.commercial_policy.is_some())
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
        assert!(cache.get(config.clone()).await.is_err());
        std::fs::write(&trust, b"invalid policy").unwrap();
        assert!(cache.get(config).await.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bounded_search_pages_over_1000_synthetic_metadata_rows() {
        // Search projection only; these synthetic metadata rows are NOT signed
        // catalog members and are never inserted into a live cache.
        let (c, existing, key, p) = fixture();
        let seed = verified_items(c, &existing, &key, 2, &p)
            .unwrap()
            .pop()
            .unwrap();
        let rows = (0..1000)
            .map(|i| {
                let mut row = seed.clone();
                let k = format!("partner_fixture_{i:04}");
                row.metadata["envelope"]["extension_key"] = json!(k);
                row.manifest["extension_key"] = json!(k);
                row.manifest["display_name"] = json!(format!("Load {i:04}"));
                row
            })
            .collect();
        let snapshot = Snapshot::new(rows, None);
        let api = profile_api(&p).unwrap();
        let first = search_page(&snapshot, &p, &api, "", 0, 100, "fixture");
        assert_eq!(first["total"], 1000);
        assert_eq!(first["next_offset"], 100);
        let last = search_page(&snapshot, &p, &api, "", 900, 100, "fixture");
        assert_eq!(last["items"].as_array().unwrap().len(), 100);
        assert_eq!(
            last["items"][0]["envelope"]["extension_key"],
            "partner_fixture_0900"
        );
        assert!(last["next_offset"].is_null());
        let filtered = search_page(&snapshot, &p, &api, "LOAD 0123", 0, 25, "fixture");
        assert_eq!(filtered["total"], 1);
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
