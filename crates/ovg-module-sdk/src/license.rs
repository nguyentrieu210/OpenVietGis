//! Signed offline lease verification primitive, not an offline authorization
//! grant. Native runtime remains disabled until host trust/time/revocation gates.
use crate::release_v2::canonical;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    pub lease_id: Uuid,
    pub issuer: String,
    pub organization_id: Uuid,
    pub subject_id: Uuid,
    pub device_id: Uuid,
    pub project_id: Uuid,
    pub manifest_id: Uuid,
    pub release_digest: String,
    pub catalog_epoch: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
pub struct LeaseTrust<'a> {
    pub key: &'a VerifyingKey,
    pub issuer: &'a str,
    pub organization_id: Uuid,
    pub subject_id: Uuid,
    pub device_id: Uuid,
    pub project_id: Uuid,
    pub manifest_id: Uuid,
    pub release_digest: &'a str,
    pub minimum_epoch: u64,
    pub now: DateTime<Utc>,
    pub last_trusted_time: DateTime<Utc>,
    pub revoked: &'a BTreeSet<Uuid>,
}
#[derive(Serialize)]
struct Signed<'a> {
    domain: &'static str,
    lease: &'a Lease,
}
pub fn verify_lease(
    lease: &Lease,
    signature: &Signature,
    trust: &LeaseTrust<'_>,
) -> Result<(), AppError> {
    let deny = || AppError::forbidden("offline entitlement lease denied");
    if [
        lease.lease_id,
        lease.organization_id,
        lease.subject_id,
        lease.device_id,
        lease.project_id,
        lease.manifest_id,
    ]
    .iter()
    .any(Uuid::is_nil)
        || lease.issuer.is_empty()
        || lease.issuer.len() > 256
        || lease.issuer != trust.issuer
        || lease.organization_id != trust.organization_id
        || lease.subject_id != trust.subject_id
        || lease.device_id != trust.device_id
        || lease.project_id != trust.project_id
        || lease.manifest_id != trust.manifest_id
        || lease.release_digest != trust.release_digest
        || lease.release_digest.len() != 64
        || !lease
            .release_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || lease.catalog_epoch == 0
        || lease.catalog_epoch > 9_007_199_254_740_991
        || lease.catalog_epoch < trust.minimum_epoch
        || trust.now < trust.last_trusted_time
        || lease.issued_at > trust.now
        || lease.expires_at <= trust.now
        || lease.expires_at.signed_duration_since(lease.issued_at) > Duration::hours(24)
        || lease.expires_at <= lease.issued_at
        || trust.revoked.contains(&lease.lease_id)
    {
        return Err(deny());
    }
    trust
        .key
        .verify_strict(
            &canonical(&Signed {
                domain: "openvietgis.entitlement.lease.v1",
                lease,
            })?,
            signature,
        )
        .map_err(|_| deny())
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn lease_scope_expiry_revocation_and_rollback() {
        let key = SigningKey::from_bytes(&[31; 32]);
        let pubkey = key.verifying_key();
        let now = DateTime::parse_from_rfc3339("2026-11-29T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let l = Lease {
            lease_id: Uuid::new_v4(),
            issuer: "fixture".into(),
            organization_id: Uuid::new_v4(),
            subject_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            manifest_id: Uuid::new_v4(),
            release_digest: "a".repeat(64),
            catalog_epoch: 4,
            issued_at: now,
            expires_at: now + Duration::hours(24),
        };
        let sig = key.sign(
            &canonical(&Signed {
                domain: "openvietgis.entitlement.lease.v1",
                lease: &l,
            })
            .unwrap(),
        );
        let empty = BTreeSet::new();
        let revoked = BTreeSet::from([l.lease_id]);
        let mut t = LeaseTrust {
            key: &pubkey,
            issuer: "fixture",
            organization_id: l.organization_id,
            subject_id: l.subject_id,
            device_id: l.device_id,
            project_id: l.project_id,
            manifest_id: l.manifest_id,
            release_digest: &l.release_digest,
            minimum_epoch: 4,
            now,
            last_trusted_time: now,
            revoked: &empty,
        };
        assert!(verify_lease(&l, &sig, &t).is_ok());
        t.minimum_epoch = 5;
        assert!(verify_lease(&l, &sig, &t).is_err());
        t.minimum_epoch = 4;
        t.device_id = Uuid::new_v4();
        assert!(verify_lease(&l, &sig, &t).is_err());
        t.device_id = l.device_id;
        t.now = l.expires_at;
        assert!(verify_lease(&l, &sig, &t).is_err());
        t.now = now;
        t.last_trusted_time = now + Duration::seconds(1);
        assert!(verify_lease(&l, &sig, &t).is_err());
        t.last_trusted_time = now;
        t.revoked = &revoked;
        assert!(verify_lease(&l, &sig, &t).is_err());
        t.revoked = &empty;
        let mut bad = l.clone();
        bad.expires_at += Duration::minutes(1);
        assert!(verify_lease(&bad, &sig, &t).is_err());
    }
}
