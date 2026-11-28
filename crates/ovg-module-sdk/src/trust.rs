//! Offline trust-authority policy, distinct from the online catalog operator.
//! A verified policy is required before admitting a configured V2 catalog.
use crate::release_v2::canonical;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn fingerprint(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn denied() -> AppError {
    AppError::forbidden("untrusted or stale marketplace policy")
}
const MAX_SAFE: u64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustPolicy {
    pub schema: String,
    pub sequence: u64,
    pub catalog_epoch: u64,
    pub catalog_membership_digest: String,
    pub operator_key_id: String,
    pub issued_at: String,
    pub expires_at: String,
    pub revoked_release_digests: Vec<String>,
    pub revoked_publisher_keys: Vec<String>,
    pub revoked_operator_keys: Vec<String>,
}
fn valid_list(values: &[String]) -> bool {
    values.len() <= 4096
        && values.iter().all(|v| valid_hash(v))
        && values.windows(2).all(|w| w[0] < w[1])
}
pub struct VerifiedTrust {
    policy: TrustPolicy,
    digest: String,
    root_key_id: String,
}
impl VerifiedTrust {
    pub fn policy(&self) -> &TrustPolicy {
        &self.policy
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn root_key_id(&self) -> &str {
        &self.root_key_id
    }
    pub fn permits(&self, release: &str, publisher_key: &str) -> bool {
        self.policy
            .revoked_release_digests
            .binary_search_by(|v| v.as_str().cmp(release))
            .is_err()
            && self
                .policy
                .revoked_publisher_keys
                .binary_search_by(|v| v.as_str().cmp(publisher_key))
                .is_err()
    }
    pub fn ensure_successor(
        &self,
        prior: &TrustPolicy,
        prior_digest: &str,
        prior_root: &str,
    ) -> Result<(), AppError> {
        let p = &self.policy;
        let includes =
            |old: &[String], new: &[String]| old.iter().all(|v| new.binary_search(v).is_ok());
        if self.root_key_id != prior_root
            || p.sequence < prior.sequence
            || p.catalog_epoch < prior.catalog_epoch
            || (p.sequence == prior.sequence && self.digest != prior_digest)
            || (p.catalog_epoch == prior.catalog_epoch
                && p.catalog_membership_digest != prior.catalog_membership_digest)
            || !includes(&prior.revoked_release_digests, &p.revoked_release_digests)
            || !includes(&prior.revoked_publisher_keys, &p.revoked_publisher_keys)
            || !includes(&prior.revoked_operator_keys, &p.revoked_operator_keys)
            || (p.operator_key_id != prior.operator_key_id
                && (p.catalog_epoch <= prior.catalog_epoch
                    || p.revoked_operator_keys
                        .binary_search(&prior.operator_key_id)
                        .is_err()))
        {
            return Err(denied());
        }
        Ok(())
    }
}
pub fn verify_policy(
    policy: TrustPolicy,
    sig: &Signature,
    root: &VerifyingKey,
    operator: &VerifyingKey,
    epoch: u64,
    membership: &str,
    now: DateTime<Utc>,
) -> Result<VerifiedTrust, AppError> {
    let p = &policy;
    let issued = DateTime::parse_from_rfc3339(&p.issued_at)
        .map_err(|_| denied())?
        .with_timezone(&Utc);
    let expires = DateTime::parse_from_rfc3339(&p.expires_at)
        .map_err(|_| denied())?
        .with_timezone(&Utc);
    if root == operator
        || root.is_weak()
        || operator.is_weak()
        || p.schema != "openvietgis.trust.policy.v1"
        || !(1..=MAX_SAFE).contains(&p.sequence)
        || !(1..=MAX_SAFE).contains(&p.catalog_epoch)
        || p.catalog_epoch != epoch
        || p.catalog_membership_digest != membership
        || !valid_hash(membership)
        || p.operator_key_id != fingerprint(operator.as_bytes())
        || p.issued_at.len() > 64
        || p.expires_at.len() > 64
        || issued > now
        || expires <= now
        || expires <= issued
        || expires.signed_duration_since(issued) > Duration::days(30)
        || !valid_list(&p.revoked_release_digests)
        || !valid_list(&p.revoked_publisher_keys)
        || !valid_list(&p.revoked_operator_keys)
        || p.revoked_operator_keys
            .binary_search(&p.operator_key_id)
            .is_ok()
    {
        return Err(denied());
    }
    let data = canonical(p)?;
    root.verify_strict(&data, sig).map_err(|_| denied())?;
    Ok(VerifiedTrust {
        policy,
        digest: fingerprint(&data),
        root_key_id: fingerprint(root.as_bytes()),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn monotonic_policy_revocation_and_operator_rotation() {
        let root = SigningKey::from_bytes(&[61; 32]);
        let operator = SigningKey::from_bytes(&[62; 32]);
        let next = SigningKey::from_bytes(&[63; 32]);
        let now = DateTime::parse_from_rfc3339("2026-11-29T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut p = TrustPolicy {
            schema: "openvietgis.trust.policy.v1".into(),
            sequence: 1,
            catalog_epoch: 2,
            catalog_membership_digest: "a".repeat(64),
            operator_key_id: fingerprint(operator.verifying_key().as_bytes()),
            issued_at: "2026-11-29T00:00:00Z".into(),
            expires_at: "2026-12-01T00:00:00Z".into(),
            revoked_release_digests: vec![],
            revoked_publisher_keys: vec![],
            revoked_operator_keys: vec![],
        };
        let verify = |p: &TrustPolicy, op: &SigningKey| {
            verify_policy(
                p.clone(),
                &root.sign(&canonical(p).unwrap()),
                &root.verifying_key(),
                &op.verifying_key(),
                p.catalog_epoch,
                &p.catalog_membership_digest,
                now,
            )
        };
        let old = verify(&p, &operator).unwrap();
        p.sequence = 2;
        p.revoked_release_digests = vec!["b".repeat(64)];
        let revoked = verify(&p, &operator).unwrap();
        assert!(!revoked.permits(&"b".repeat(64), &"c".repeat(64)));
        assert!(revoked
            .ensure_successor(old.policy(), old.digest(), old.root_key_id())
            .is_ok());
        assert!(old
            .ensure_successor(revoked.policy(), revoked.digest(), revoked.root_key_id())
            .is_err());
        p.sequence = 3;
        p.revoked_release_digests.clear();
        assert!(verify(&p, &operator)
            .unwrap()
            .ensure_successor(revoked.policy(), revoked.digest(), revoked.root_key_id())
            .is_err());
        p.revoked_release_digests = vec!["b".repeat(64)];
        p.catalog_epoch = 3;
        p.catalog_membership_digest = "d".repeat(64);
        p.operator_key_id = fingerprint(next.verifying_key().as_bytes());
        assert!(verify(&p, &next)
            .unwrap()
            .ensure_successor(revoked.policy(), revoked.digest(), revoked.root_key_id())
            .is_err());
        p.revoked_operator_keys = vec![fingerprint(operator.verifying_key().as_bytes())];
        let rotated = verify(&p, &next).unwrap();
        assert!(rotated
            .ensure_successor(revoked.policy(), revoked.digest(), revoked.root_key_id())
            .is_ok());
        assert!(verify(&p, &operator).is_err());
        p.sequence = 4;
        p.catalog_membership_digest = "e".repeat(64);
        assert!(verify(&p, &next)
            .unwrap()
            .ensure_successor(rotated.policy(), rotated.digest(), rotated.root_key_id())
            .is_err());
    }
}
