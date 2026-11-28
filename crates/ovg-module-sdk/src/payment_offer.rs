//! Signed sandbox price offer contract. The browser cannot choose the price.
//! This verifier is not a checkout session or live provider adapter.
use crate::{commerce::Quote, release_v2::canonical, trust::fingerprint};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceOffer {
    pub schema: String,
    pub audience: String,
    pub release_digest: String,
    pub publisher_id: String,
    pub quote: Quote,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
pub struct VerifiedOffer {
    offer: PriceOffer,
    digest: String,
}
impl VerifiedOffer {
    pub fn offer(&self) -> &PriceOffer {
        &self.offer
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
pub fn verify_offer(
    o: PriceOffer,
    sig: &Signature,
    key: &VerifyingKey,
    audience: &str,
    now: DateTime<Utc>,
) -> Result<VerifiedOffer, AppError> {
    let deny = || AppError::validation("INVALID_PRICE_OFFER", "invalid signed sandbox price offer");
    o.quote.validate()?;
    if key.is_weak()
        || o.schema != "openvietgis.checkout.sandbox.offer.v1"
        || o.audience != audience
        || audience.is_empty()
        || audience.len() > 128
        || !crate::entitlement::valid_hash(&o.release_digest)
        || o.publisher_id.is_empty()
        || o.publisher_id.len() > 32
        || !o
            .publisher_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || !matches!(o.quote.currency.as_str(), "VND" | "USD" | "EUR")
        || o.issued_at > now
        || o.expires_at <= now
        || o.expires_at <= o.issued_at
        || o.expires_at - o.issued_at > Duration::minutes(30)
        || o.quote.access_until <= o.expires_at
        || o.quote.access_until - o.issued_at > Duration::days(366)
    {
        return Err(deny());
    }
    let data = canonical(&o)?;
    key.verify_strict(&data, sig).map_err(|_| deny())?;
    Ok(VerifiedOffer {
        offer: o,
        digest: fingerprint(&data),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use uuid::Uuid;
    #[test]
    fn signed_price_and_scope() {
        let k = SigningKey::from_bytes(&[105; 32]);
        let now = Utc::now();
        let mut o = PriceOffer {
            schema: "openvietgis.checkout.sandbox.offer.v1".into(),
            audience: "fixture".into(),
            release_digest: "a".repeat(64),
            publisher_id: "fixture".into(),
            quote: Quote {
                order_id: Uuid::new_v4(),
                organization_id: Uuid::new_v4(),
                extension_key: "partner_fixture_context".into(),
                currency: "VND".into(),
                gross_minor: 100,
                tax_minor: 10,
                publisher_bps: 7000,
                access_until: now + Duration::days(30),
            },
            issued_at: now,
            expires_at: now + Duration::minutes(10),
        };
        let sig = k.sign(&canonical(&o).unwrap());
        let proof = verify_offer(o.clone(), &sig, &k.verifying_key(), "fixture", now).unwrap();
        assert_eq!(proof.offer().quote.gross_minor, 100);
        assert_eq!(proof.digest().len(), 64);
        assert!(verify_offer(o.clone(), &sig, &k.verifying_key(), "other", now).is_err());
        assert!(
            verify_offer(o.clone(), &sig, &k.verifying_key(), "fixture", o.expires_at).is_err()
        );
        o.quote.gross_minor = 1;
        assert!(verify_offer(o.clone(), &sig, &k.verifying_key(), "fixture", now).is_err());
    }
}
