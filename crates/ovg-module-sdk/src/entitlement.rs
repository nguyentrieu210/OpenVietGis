//! Authority-signed commercial grants, independent of permission grants and sandbox payments.
use crate::{release_v2::canonical, trust::fingerprint};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommercialPolicy {
    pub audience: String,
    pub authority_key_id: String,
    pub licensed_release_digests: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocation_floors: Vec<LicenseRevocationFloor>,
}
/// Central revocation of certificates at or below an inclusive sequence,
/// scoped to one tenant and one exact release. A later certificate can renew.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseRevocationFloor {
    pub organization_id: Uuid,
    pub release_digest: String,
    pub through_sequence: u64,
}
impl LicenseRevocationFloor {
    fn key(&self) -> (Uuid, &str) {
        (self.organization_id, self.release_digest.as_str())
    }
}
pub fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl CommercialPolicy {
    pub fn validate(&self) -> bool {
        !self.audience.is_empty()
            && self.audience.len() <= 128
            && self
                .audience
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            && valid_hash(&self.authority_key_id)
            && !self.licensed_release_digests.is_empty()
            && self.licensed_release_digests.len() <= 4096
            && self.licensed_release_digests.iter().all(|v| valid_hash(v))
            && self
                .licensed_release_digests
                .windows(2)
                .all(|w| w[0] < w[1])
            && self.revocation_floors.len() <= 512
            && self.revocation_floors.iter().all(|f| {
                !f.organization_id.is_nil()
                    && self.requires(&f.release_digest)
                    && (1..=9_007_199_254_740_991).contains(&f.through_sequence)
            })
            && self
                .revocation_floors
                .windows(2)
                .all(|w| w[0].key() < w[1].key())
    }
    pub fn requires(&self, digest: &str) -> bool {
        self.licensed_release_digests
            .binary_search_by(|v| v.as_str().cmp(digest))
            .is_ok()
    }
    pub fn revocation_floor(&self, org: Uuid, digest: &str) -> u64 {
        self.revocation_floors
            .binary_search_by(|f| f.key().cmp(&(org, digest)))
            .ok()
            .map(|i| self.revocation_floors[i].through_sequence)
            .unwrap_or(0)
    }
    pub fn revokes(&self, grant: &LicenseGrant) -> bool {
        grant.sequence <= self.revocation_floor(grant.organization_id, &grant.release_digest)
    }
    pub fn preserves(&self, old: &Self) -> bool {
        self.audience == old.audience
            && old
                .licensed_release_digests
                .iter()
                .all(|v| self.requires(v))
            && old.revocation_floors.iter().all(|f| {
                self.revocation_floor(f.organization_id, &f.release_digest) >= f.through_sequence
            })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantKind {
    Trial,
    Subscription,
    Revoked,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseState {
    Pending,
    Trial,
    Active,
    Grace,
    Expired,
    Revoked,
    Missing,
}
impl LicenseState {
    pub fn allowed(self) -> bool {
        matches!(self, Self::Trial | Self::Active | Self::Grace)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseGrant {
    pub schema: String,
    pub audience: String,
    pub authority_key_id: String,
    pub grant_id: Uuid,
    pub organization_id: Uuid,
    pub extension_key: String,
    pub release_digest: String,
    pub sequence: u64,
    pub kind: GrantKind,
    pub issued_at: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub grace_until: DateTime<Utc>,
}
pub struct VerifiedLicense {
    grant: LicenseGrant,
    digest: String,
}
impl VerifiedLicense {
    pub fn grant(&self) -> &LicenseGrant {
        &self.grant
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
impl LicenseGrant {
    pub fn state(&self, now: DateTime<Utc>) -> LicenseState {
        if self.kind == GrantKind::Revoked {
            return LicenseState::Revoked;
        }
        if now < self.valid_from {
            return LicenseState::Pending;
        }
        if now < self.valid_until {
            return if self.kind == GrantKind::Trial {
                LicenseState::Trial
            } else {
                LicenseState::Active
            };
        }
        if self.kind == GrantKind::Subscription && now < self.grace_until {
            return LicenseState::Grace;
        }
        LicenseState::Expired
    }
}
pub fn verify_grant(
    g: LicenseGrant,
    sig: &Signature,
    key: &VerifyingKey,
    policy: &CommercialPolicy,
    org: Uuid,
    now: DateTime<Utc>,
) -> Result<VerifiedLicense, AppError> {
    let deny = || AppError::forbidden("invalid commercial license");
    if !policy.validate()
        || key.is_weak()
        || g.schema != "openvietgis.license.grant.v1"
        || g.audience != policy.audience
        || g.authority_key_id != policy.authority_key_id
        || fingerprint(key.as_bytes()) != g.authority_key_id
        || g.organization_id != org
        || org.is_nil()
        || g.grant_id.is_nil()
        || !policy.requires(&g.release_digest)
        || !g.extension_key.starts_with("partner_")
        || g.extension_key.len() > 100
        || !g
            .extension_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        || !(1..=9_007_199_254_740_991).contains(&g.sequence)
        || g.issued_at > now
        || g.valid_from < g.issued_at
    {
        return Err(deny());
    }
    let term = g.valid_until.signed_duration_since(g.valid_from);
    let grace = g.grace_until.signed_duration_since(g.valid_until);
    if match g.kind {
        GrantKind::Trial => {
            term <= Duration::zero() || term > Duration::days(30) || grace != Duration::zero()
        }
        GrantKind::Subscription => {
            term <= Duration::zero()
                || term > Duration::days(366)
                || grace < Duration::zero()
                || grace > Duration::days(7)
        }
        GrantKind::Revoked => {
            g.valid_from != g.issued_at || term != Duration::zero() || grace != Duration::zero()
        }
    } {
        return Err(deny());
    }
    // Bound scheduled starts; do not let a certificate reserve arbitrary future access.
    if g.valid_from.signed_duration_since(g.issued_at) > Duration::days(30) {
        return Err(deny());
    }
    let data = canonical(&g)?;
    key.verify_strict(&data, sig).map_err(|_| deny())?;
    Ok(VerifiedLicense {
        grant: g,
        digest: fingerprint(&data),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn signed_license_boundaries() {
        let k = SigningKey::from_bytes(&[101; 32]);
        let now = DateTime::parse_from_rfc3339("2026-11-29T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let p = CommercialPolicy {
            audience: "fixture-installation".into(),
            authority_key_id: fingerprint(k.verifying_key().as_bytes()),
            licensed_release_digests: vec!["a".repeat(64)],
            revocation_floors: vec![],
        };
        let mut g = LicenseGrant {
            schema: "openvietgis.license.grant.v1".into(),
            audience: p.audience.clone(),
            authority_key_id: p.authority_key_id.clone(),
            grant_id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            extension_key: "partner_fixture_context".into(),
            release_digest: "a".repeat(64),
            sequence: 1,
            kind: GrantKind::Subscription,
            issued_at: now,
            valid_from: now,
            valid_until: now + Duration::days(30),
            grace_until: now + Duration::days(37),
        };
        let mut centrally_revoked = p.clone();
        centrally_revoked
            .revocation_floors
            .push(LicenseRevocationFloor {
                organization_id: g.organization_id,
                release_digest: g.release_digest.clone(),
                through_sequence: 1,
            });
        assert!(centrally_revoked.validate());
        assert!(centrally_revoked.revokes(&g));
        assert!(centrally_revoked.preserves(&p));
        assert!(!p.preserves(&centrally_revoked));
        let mut renewal = g.clone();
        renewal.sequence = 2;
        assert!(!centrally_revoked.revokes(&renewal));
        renewal.sequence = 1;
        renewal.organization_id = Uuid::new_v4();
        assert!(!centrally_revoked.revokes(&renewal));
        let mut invalid = centrally_revoked.clone();
        invalid
            .revocation_floors
            .push(invalid.revocation_floors[0].clone());
        assert!(!invalid.validate());
        let sign = |g: &LicenseGrant| k.sign(&canonical(g).unwrap());
        assert!(verify_grant(
            g.clone(),
            &sign(&g),
            &k.verifying_key(),
            &p,
            g.organization_id,
            now
        )
        .is_ok());
        assert!(verify_grant(
            g.clone(),
            &sign(&g),
            &k.verifying_key(),
            &p,
            Uuid::new_v4(),
            now
        )
        .is_err());
        assert_eq!(g.state(now - Duration::seconds(1)), LicenseState::Pending);
        assert_eq!(g.state(now), LicenseState::Active);
        assert_eq!(g.state(g.valid_until), LicenseState::Grace);
        assert_eq!(g.state(g.grace_until), LicenseState::Expired);
        g.kind = GrantKind::Trial;
        assert!(verify_grant(
            g.clone(),
            &sign(&g),
            &k.verifying_key(),
            &p,
            g.organization_id,
            now
        )
        .is_err());
        g.grace_until = g.valid_until;
        assert_eq!(g.state(now), LicenseState::Trial);
        assert!(verify_grant(
            g.clone(),
            &sign(&g),
            &k.verifying_key(),
            &p,
            g.organization_id,
            now
        )
        .is_ok());
        g.kind = GrantKind::Revoked;
        g.valid_until = now;
        g.grace_until = now;
        assert!(verify_grant(
            g.clone(),
            &sign(&g),
            &k.verifying_key(),
            &p,
            g.organization_id,
            now
        )
        .is_ok());
        assert_eq!(g.state(now), LicenseState::Revoked);
    }
}
