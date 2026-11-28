//! SPEC 075 additive release verification and dependency lock primitives.
//! No v1 manifest changes. The caller must obtain trusted keys and scope from
//! authenticated persistence, never from the plugin or its request body.
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BUNDLE: usize = 10 * 1024 * 1024;
const MAX_ENVELOPE: usize = 256 * 1024;
fn invalid(msg: &str) -> AppError {
    AppError::validation("INVALID_PLUGIN_RELEASE", msg.to_owned())
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && s != "."
        && s != ".."
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub media_type: String,
    pub size: usize,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContributionEnvelope {
    pub schema_version: u8,
    pub publisher_id: String,
    pub extension_key: String,
    pub version: String,
    pub manifest_digest: String,
    pub host_api_range: String,
    pub platforms: Vec<String>,
    pub permissions: Vec<String>,
    pub contribution_digest: String,
    pub assets: Vec<Asset>,
    pub issued_at: String,
    pub key_id: String,
}
/// V2 signing profile: sorted ASCII member names, compact JSON, typed integers,
/// strings and arrays only. Existing marketplace v1 canonical bytes unchanged.
pub fn canonical<T: Serialize>(v: &T) -> Result<Vec<u8>, AppError> {
    fn sort(v: serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(m) => {
                let ordered: BTreeMap<_, _> = m.into_iter().map(|(k, v)| (k, sort(v))).collect();
                serde_json::Value::Object(ordered.into_iter().collect())
            }
            serde_json::Value::Array(a) => {
                serde_json::Value::Array(a.into_iter().map(sort).collect())
            }
            v => v,
        }
    }
    let v = serde_json::to_value(v).map_err(|_| invalid("serialization failed"))?;
    serde_json::to_vec(&sort(v)).map_err(|_| invalid("serialization failed"))
}
impl ContributionEnvelope {
    pub fn parse(bytes: &[u8]) -> Result<Self, AppError> {
        if bytes.len() > MAX_ENVELOPE {
            return Err(invalid("envelope limit"));
        }
        let env: Self = serde_json::from_slice(bytes).map_err(|_| invalid("invalid envelope"))?;
        env.validate()?;
        Ok(env)
    }
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != 2
            || !identifier(&self.publisher_id)
            || !identifier(&self.extension_key)
            || !identifier(&self.key_id)
            || self.version.len() > 64
            || Version::parse(&self.version).is_err()
            || self.host_api_range.len() > 128
            || VersionReq::parse(&self.host_api_range).is_err()
            || !hash_valid(&self.manifest_digest)
            || !hash_valid(&self.contribution_digest)
            || self.issued_at.len() > 64
            || chrono::DateTime::parse_from_rfc3339(&self.issued_at).is_err()
            || self.platforms.is_empty()
            || self.platforms.len() > 7
            || self.permissions.len() > 64
            || self.assets.len() > 128
        {
            return Err(invalid("invalid envelope fields"));
        }
        let platforms: BTreeSet<_> = self.platforms.iter().collect();
        let perms: BTreeSet<_> = self.permissions.iter().collect();
        if platforms.len() != self.platforms.len()
            || perms.len() != self.permissions.len()
            || self.platforms.iter().any(|p| {
                !matches!(
                    p.as_str(),
                    "web" | "android" | "ios" | "windows" | "macos" | "linux" | "server"
                )
            })
            || self
                .permissions
                .iter()
                .any(|p| !identifier(p) || !p.starts_with(&format!("ext.{}.", self.extension_key)))
        {
            return Err(invalid("invalid platform or permission"));
        }
        let mut ids = BTreeSet::new();
        let mut total = 0usize;
        for a in &self.assets {
            total = total
                .checked_add(a.size)
                .ok_or_else(|| invalid("asset size overflow"))?;
            if !identifier(&a.id)
                || !ids.insert(&a.id)
                || !hash_valid(&a.sha256)
                || !matches!(
                    a.media_type.as_str(),
                    "application/json" | "image/png" | "image/jpeg" | "image/webp"
                )
                || total > MAX_BUNDLE
            {
                return Err(invalid("invalid or excessive assets"));
            }
        }
        if canonical(self)?.len() > MAX_ENVELOPE {
            return Err(invalid("envelope limit"));
        }
        Ok(())
    }
}
/// Trusted binding from the reviewed catalog, including the current catalog epoch.
/// Trust roots and revocation state must come from the existing operator, not user input.
pub struct ReleaseTrust<'a> {
    pub publisher_id: &'a str,
    pub extension_key: &'a str,
    pub version: &'a str,
    pub key_id: &'a str,
    pub publisher_key: &'a VerifyingKey,
    pub operator_key: &'a VerifyingKey,
    pub manifest_digest: &'a str,
    pub allowed_permissions: &'a BTreeSet<String>,
    pub minimum_epoch: u64,
    pub revoked: bool,
}
#[derive(Serialize)]
struct Review<'a> {
    schema: &'static str,
    envelope_digest: &'a str,
    epoch: u64,
}
pub struct Bundle<'a> {
    pub envelope: &'a ContributionEnvelope,
    pub publisher_signature: &'a Signature,
    pub operator_signature: &'a Signature,
    pub epoch: u64,
    pub contribution: &'a [u8],
    pub assets: &'a BTreeMap<String, Vec<u8>>,
}
/// Verify BOTH signatures, scope, digest, asset sizes and compatibility before
/// returning an immutable release digest. This does not install or execute code.
pub fn verify_bundle(
    bundle: &Bundle<'_>,
    trust: &ReleaseTrust<'_>,
    host: &str,
    api: &Version,
) -> Result<String, AppError> {
    let e = bundle.envelope;
    e.validate()?;
    if trust.revoked
        || trust.publisher_key == trust.operator_key
        || bundle.epoch == 0
        || bundle.epoch > 9_007_199_254_740_991
        || bundle.epoch < trust.minimum_epoch
        || trust.publisher_id != e.publisher_id
        || trust.extension_key != e.extension_key
        || trust.version != e.version
        || trust.key_id != e.key_id
        || trust.manifest_digest != e.manifest_digest
        || e.permissions
            .iter()
            .any(|p| !trust.allowed_permissions.contains(p))
        || !e.platforms.iter().any(|p| p == host)
        || !VersionReq::parse(&e.host_api_range)
            .map_err(|_| invalid("bad API range"))?
            .matches(api)
    {
        return Err(invalid("untrusted or incompatible release"));
    }
    let bytes = canonical(e)?;
    trust
        .publisher_key
        .verify_strict(&bytes, bundle.publisher_signature)
        .map_err(|_| invalid("publisher signature"))?;
    let d = digest(&bytes);
    let review = canonical(&Review {
        schema: "openvietgis.review.v2",
        envelope_digest: &d,
        epoch: bundle.epoch,
    })?;
    trust
        .operator_key
        .verify_strict(&review, bundle.operator_signature)
        .map_err(|_| invalid("operator signature"))?;
    if bundle.contribution.len() > MAX_ENVELOPE
        || digest(bundle.contribution) != e.contribution_digest
        || bundle.assets.len() != e.assets.len()
    {
        return Err(invalid("contribution or asset mismatch"));
    }
    for a in &e.assets {
        let bytes = bundle
            .assets
            .get(&a.id)
            .ok_or_else(|| invalid("missing asset"))?;
        if bytes.len() != a.size || digest(bytes) != a.sha256 {
            return Err(invalid("asset digest mismatch"));
        }
    }
    Ok(d)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub key: String,
    pub range: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub key: String,
    pub version: String,
    pub digest: String,
    pub dependencies: Vec<Dependency>,
}
/// Validate a proposed exact lock against all dependencies, with deterministic
/// topological order. Selection is separate; arbitrary extra pins are rejected.
pub fn validate_lock(root: &str, pins: &[Pin]) -> Result<Vec<String>, AppError> {
    if pins.is_empty() || pins.len() > 256 {
        return Err(invalid("lock size"));
    }
    let mut map = BTreeMap::new();
    for p in pins {
        if !identifier(&p.key)
            || !hash_valid(&p.digest)
            || p.version.len() > 64
            || Version::parse(&p.version).is_err()
            || p.dependencies.len() > 64
            || map.insert(p.key.as_str(), p).is_some()
        {
            return Err(invalid("invalid or duplicate pin"));
        }
    }
    fn visit<'a>(
        key: &'a str,
        map: &BTreeMap<&'a str, &'a Pin>,
        active: &mut BTreeSet<&'a str>,
        done: &mut BTreeSet<&'a str>,
        order: &mut Vec<String>,
    ) -> Result<(), AppError> {
        if done.contains(key) {
            return Ok(());
        }
        if !active.insert(key) {
            return Err(invalid("dependency cycle"));
        }
        let p = map.get(key).ok_or_else(|| invalid("missing dependency"))?;
        let mut deps: Vec<_> = p.dependencies.iter().collect();
        deps.sort_by(|a, b| a.key.cmp(&b.key));
        let mut seen = BTreeSet::new();
        for dep in deps {
            if !seen.insert(&dep.key) || dep.range.len() > 128 {
                return Err(invalid("duplicate or invalid dependency"));
            }
            let target = map
                .get(dep.key.as_str())
                .ok_or_else(|| invalid("missing dependency"))?;
            let range =
                VersionReq::parse(&dep.range).map_err(|_| invalid("invalid dependency range"))?;
            if !range
                .matches(&Version::parse(&target.version).map_err(|_| invalid("invalid version"))?)
            {
                return Err(invalid("dependency version mismatch"));
            }
            visit(dep.key.as_str(), map, active, done, order)?;
        }
        active.remove(key);
        done.insert(key);
        order.push(key.to_owned());
        Ok(())
    }
    let root = map
        .get_key_value(root)
        .ok_or_else(|| invalid("missing root"))?
        .0;
    let mut order = Vec::new();
    visit(
        root,
        &map,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut order,
    )?;
    if order.len() != pins.len() {
        return Err(invalid("unreachable lock pins"));
    }
    Ok(order)
}
/// Select one version per dependency from a caller-verified compatible catalog.
/// Exact root release is fixed; prefer highest compatible versions, backtrack
/// on conflicts/cycles. Strict search budgets fail closed rather than hang.
pub fn resolve_lock(root: &str, root_version: &str, catalog: &[Pin]) -> Result<Vec<Pin>, AppError> {
    if catalog.is_empty() || catalog.len() > 1024 {
        return Err(invalid("catalog size"));
    }
    let mut versions: BTreeMap<&str, Vec<(&Pin, Version)>> = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for p in catalog {
        if !identifier(&p.key)
            || !hash_valid(&p.digest)
            || p.version.len() > 64
            || p.dependencies.len() > 64
            || !identities.insert((&p.key, &p.version))
        {
            return Err(invalid("invalid catalog pin"));
        }
        let v = Version::parse(&p.version).map_err(|_| invalid("invalid catalog version"))?;
        let mut deps = BTreeSet::new();
        for d in &p.dependencies {
            if !identifier(&d.key)
                || d.range.len() > 128
                || !deps.insert(&d.key)
                || VersionReq::parse(&d.range).is_err()
            {
                return Err(invalid("invalid catalog dependency"));
            }
        }
        versions.entry(&p.key).or_default().push((p, v));
    }
    for candidates in versions.values_mut() {
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.digest.cmp(&b.0.digest)));
    }
    let start = versions
        .get(root)
        .and_then(|v| v.iter().find(|(p, _)| p.version == root_version))
        .map(|(p, _)| *p)
        .ok_or_else(|| invalid("root release not found"))?;
    let mut selected = BTreeMap::from([(start.key.as_str(), start)]);
    fn search<'a>(
        root: &str,
        available: &BTreeMap<&'a str, Vec<(&'a Pin, Version)>>,
        selected: &mut BTreeMap<&'a str, &'a Pin>,
        steps: &mut usize,
    ) -> Result<Option<Vec<Pin>>, AppError> {
        *steps += 1;
        if *steps > 10_000 {
            return Err(invalid("dependency search budget exhausted"));
        }
        if selected.len() > 256 {
            return Ok(None);
        }
        let mut required: BTreeMap<&'a str, Vec<VersionReq>> = BTreeMap::new();
        for pin in selected.values() {
            for d in &pin.dependencies {
                required.entry(&d.key).or_default().push(
                    VersionReq::parse(&d.range).map_err(|_| invalid("invalid dependency range"))?,
                );
            }
        }
        for (key, ranges) in &required {
            if let Some(p) = selected.get(key) {
                let v = Version::parse(&p.version).map_err(|_| invalid("invalid version"))?;
                if !ranges.iter().all(|r| r.matches(&v)) {
                    return Ok(None);
                }
            }
        }
        let Some((key, ranges)) = required
            .iter()
            .find(|(key, _)| !selected.contains_key(**key))
        else {
            let pins: Vec<Pin> = selected.values().map(|p| (*p).clone()).collect();
            let Ok(order) = validate_lock(root, &pins) else {
                return Ok(None);
            };
            let map: BTreeMap<_, _> = pins.into_iter().map(|p| (p.key.clone(), p)).collect();
            return Ok(Some(
                order.into_iter().map(|key| map[&key].clone()).collect(),
            ));
        };
        if let Some(candidates) = available.get(key) {
            for (pin, version) in candidates {
                if ranges.iter().all(|r| r.matches(version)) {
                    selected.insert(pin.key.as_str(), pin);
                    let result = search(root, available, selected, steps)?;
                    selected.remove(pin.key.as_str());
                    if result.is_some() {
                        return Ok(result);
                    }
                }
            }
        }
        Ok(None)
    }
    search(root, &versions, &mut selected, &mut 0)?
        .ok_or_else(|| invalid("no compatible dependency lock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn pin(key: &str, deps: &[&str]) -> Pin {
        Pin {
            key: key.into(),
            version: "1.2.0".into(),
            digest: "a".repeat(64),
            dependencies: deps
                .iter()
                .map(|k| Dependency {
                    key: (*k).into(),
                    range: "^1".into(),
                })
                .collect(),
        }
    }
    #[test]
    fn dependency_solver_backtracks_and_is_deterministic() {
        let root = pin("a", &["b", "c"]);
        let mut b_new = pin("b", &["d"]);
        b_new.version = "1.9.0".into();
        b_new.dependencies[0].range = "^2".into();
        let b_old = pin("b", &["d"]);
        let c = pin("c", &["d"]);
        let d1 = pin("d", &[]);
        let mut d2 = pin("d", &[]);
        d2.version = "2.0.0".into();
        let catalog = vec![root.clone(), b_new, b_old, c, d1, d2];
        let result = resolve_lock("a", "1.2.0", &catalog).unwrap();
        assert_eq!(
            result.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
            vec!["d", "b", "c", "a"]
        );
        assert_eq!(
            result.iter().find(|p| p.key == "b").unwrap().version,
            "1.2.0"
        );
        let mut reversed = catalog.clone();
        reversed.reverse();
        assert_eq!(
            canonical(&result).unwrap(),
            canonical(&resolve_lock("a", "1.2.0", &reversed).unwrap()).unwrap()
        );
        assert!(resolve_lock("a", "1.2.0", &[pin("a", &["b"]), pin("b", &["a"])]).is_err());
        assert!(resolve_lock("a", "1.2.0", &[root]).is_err());
    }
    #[test]
    fn dependency_order_and_rejections() {
        assert_eq!(
            validate_lock("a", &[pin("a", &["c", "b"]), pin("c", &[]), pin("b", &[])]).unwrap(),
            vec!["b", "c", "a"]
        );
        assert!(validate_lock("a", &[pin("a", &["b"]), pin("b", &["a"])]).is_err());
        assert!(validate_lock("a", &[pin("a", &["b"])]).is_err());
        assert!(validate_lock("a", &[pin("a", &[]), pin("b", &[])]).is_err());
        let mut b = pin("b", &[]);
        b.version = "2.0.0".into();
        assert!(validate_lock("a", &[pin("a", &["b"]), b]).is_err());
    }
    #[test]
    fn two_signatures_and_tamper() {
        let publisher = SigningKey::from_bytes(&[1; 32]);
        let operator = SigningKey::from_bytes(&[2; 32]);
        let payload = b"{\"slot\":\"form\"}";
        let e = ContributionEnvelope {
            schema_version: 2,
            publisher_id: "pub".into(),
            extension_key: "partner_pub_demo".into(),
            version: "1.0.0".into(),
            manifest_digest: "a".repeat(64),
            host_api_range: "^1".into(),
            platforms: vec!["web".into()],
            permissions: vec![],
            contribution_digest: digest(payload),
            assets: vec![],
            issued_at: "2026-11-29T00:00:00Z".into(),
            key_id: "key1".into(),
        };
        let bytes = canonical(&e).unwrap();
        let d = digest(&bytes);
        let ps = publisher.sign(&bytes);
        let os = operator.sign(
            &canonical(&Review {
                schema: "openvietgis.review.v2",
                envelope_digest: &d,
                epoch: 4,
            })
            .unwrap(),
        );
        let pk = publisher.verifying_key();
        let ok = operator.verifying_key();
        let permissions = BTreeSet::new();
        let mut trust = ReleaseTrust {
            publisher_id: "pub",
            extension_key: "partner_pub_demo",
            version: "1.0.0",
            key_id: "key1",
            publisher_key: &pk,
            operator_key: &ok,
            manifest_digest: &e.manifest_digest,
            allowed_permissions: &permissions,
            minimum_epoch: 4,
            revoked: false,
        };
        let assets = BTreeMap::new();
        let mut b = Bundle {
            envelope: &e,
            publisher_signature: &ps,
            operator_signature: &os,
            epoch: 4,
            contribution: payload,
            assets: &assets,
        };
        let api = Version::parse("1.0.0").unwrap();
        assert_eq!(verify_bundle(&b, &trust, "web", &api).unwrap(), d);
        assert!(verify_bundle(&b, &trust, "ios", &api).is_err());
        b.contribution = b"tamper";
        assert!(verify_bundle(&b, &trust, "web", &api).is_err());
        b.contribution = payload;
        b.epoch = 3;
        assert!(verify_bundle(&b, &trust, "web", &api).is_err());
        b.epoch = 4;
        trust.revoked = true;
        assert!(verify_bundle(&b, &trust, "web", &api).is_err());
    }
}
