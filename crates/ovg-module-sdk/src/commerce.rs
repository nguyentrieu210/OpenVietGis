//! SPEC 075 commercial domain primitives. Provider-neutral sandbox logic;
//! not a payment processor, checkout endpoint, or authorization for GIS data.
//! Production callers must verify external signatures and persist events,
//! state, ledger and entitlement outbox in one serializable transaction.
use crate::release_v2::canonical;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;
fn invalid(msg: &str) -> AppError {
    AppError::validation("INVALID_PLUGIN_PAYMENT", msg.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub order_id: Uuid,
    pub organization_id: Uuid,
    pub extension_key: String,
    pub currency: String,
    pub gross_minor: i64,
    pub tax_minor: i64,
    pub publisher_bps: u16,
    pub access_until: DateTime<Utc>,
}
impl Quote {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.order_id.is_nil()
            || self.organization_id.is_nil()
            || self.extension_key.is_empty()
            || self.extension_key.len() > 100
            || !self
                .extension_key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
            || self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
            || !(1..=9_007_199_254_740_991).contains(&self.gross_minor)
            || !(0..=self.gross_minor).contains(&self.tax_minor)
            || self.publisher_bps > 10_000
        {
            return Err(invalid("invalid order quote"));
        }
        Ok(())
    }
    fn allocations(&self) -> [i64; 3] {
        let net = self.gross_minor - self.tax_minor;
        let publisher = (i128::from(net) * i128::from(self.publisher_bps) / 10_000) as i64;
        [publisher, net - publisher, self.tax_minor]
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Capture,
    Refund,
    Chargeback,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxEvent {
    pub provider_event_id: String,
    pub order_id: Uuid,
    pub organization_id: Uuid,
    pub kind: EventKind,
    pub currency: String,
    pub amount_minor: i64,
    pub occurred_at: DateTime<Utc>,
}
#[derive(Serialize)]
struct SignedSandbox<'a> {
    domain: &'static str,
    event: &'a SandboxEvent,
}
/// Unforgeable proof constructed only by the pinned sandbox verification path.
/// A production provider adapter requires a separate reviewed verification contract.
pub struct VerifiedSandbox(SandboxEvent);
impl VerifiedSandbox {
    pub fn event(&self) -> &SandboxEvent {
        &self.0
    }
}
pub fn verify_sandbox(
    event: SandboxEvent,
    key: &VerifyingKey,
    signature: &Signature,
    now: DateTime<Utc>,
) -> Result<VerifiedSandbox, AppError> {
    if event.provider_event_id.is_empty()
        || event.provider_event_id.len() > 128
        || !event
            .provider_event_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        || event.occurred_at > now + chrono::Duration::minutes(5)
    {
        return Err(invalid("invalid sandbox event"));
    }
    let message = canonical(&SignedSandbox {
        domain: "openvietgis.payment.sandbox.v1",
        event: &event,
    })?;
    key.verify_strict(&message, signature)
        .map_err(|_| invalid("sandbox signature"))?;
    Ok(VerifiedSandbox(event))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Account {
    ProviderClearing,
    PublisherPayable,
    PlatformRevenue,
    TaxPayable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posting {
    pub account: Account,
    pub debit_minor: i64,
    pub credit_minor: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerReceipt {
    pub event_id: String,
    pub postings: Vec<Posting>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitlementState {
    Pending,
    Active,
    Expired,
    Revoked,
}
#[derive(Debug, Clone)]
pub struct OrderLedger {
    quote: Quote,
    captured: bool,
    reversed_minor: i64,
    events: BTreeMap<String, ([u8; 32], LedgerReceipt)>,
}
impl OrderLedger {
    pub fn new(quote: Quote) -> Result<Self, AppError> {
        quote.validate()?;
        Ok(Self {
            quote,
            captured: false,
            reversed_minor: 0,
            events: BTreeMap::new(),
        })
    }
    pub fn currency(&self) -> &str {
        &self.quote.currency
    }
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
    pub fn entitlement(&self, now: DateTime<Utc>) -> EntitlementState {
        if !self.captured {
            EntitlementState::Pending
        } else if self.reversed_minor == self.quote.gross_minor {
            EntitlementState::Revoked
        } else if now >= self.quote.access_until {
            EntitlementState::Expired
        } else {
            EntitlementState::Active
        }
    }
    pub fn outstanding_minor(&self) -> i64 {
        if self.captured {
            self.quote.gross_minor - self.reversed_minor
        } else {
            0
        }
    }
    /// Cumulative allocation makes repeated partial refund rounding converge
    /// exactly to the original postings. Tax reversal is sandbox policy only.
    fn reversed_allocation(&self, amount: i64) -> [i64; 3] {
        let [publisher, platform, tax] = self.quote.allocations();
        let ratio = |x: i64| {
            (i128::from(x) * i128::from(amount) / i128::from(self.quote.gross_minor)) as i64
        };
        // Apportion two cumulative balances with a residual, so every cent
        // balances and no individual account becomes negative or exceeds its cap.
        let p = ratio(publisher);
        let t = ratio(tax);
        let rem = amount - p - t;
        if rem <= platform {
            [p, rem, t]
        } else {
            let delta = rem - platform;
            let pextra = delta.min(publisher - p);
            [p + pextra, platform, t + delta - pextra]
        }
    }
    pub fn apply(&mut self, verified: &VerifiedSandbox) -> Result<LedgerReceipt, AppError> {
        let e = &verified.0;
        if e.order_id != self.quote.order_id
            || e.organization_id != self.quote.organization_id
            || e.currency != self.quote.currency
            || e.amount_minor <= 0
            || e.amount_minor > self.quote.gross_minor
        {
            return Err(invalid("event does not match order"));
        }
        let digest: [u8; 32] = Sha256::digest(canonical(e)?).into();
        if let Some((previous, receipt)) = self.events.get(&e.provider_event_id) {
            if *previous != digest {
                return Err(AppError::conflict(
                    "PAYMENT_EVENT_CONFLICT",
                    "event ID reused with different payload",
                ));
            }
            return Ok(receipt.clone());
        }
        if self.events.len() >= 1024 {
            return Err(invalid("order event limit"));
        }
        let mut postings = Vec::new();
        let mut post = |account: Account, amount: i64, debit: bool| {
            if amount > 0 {
                postings.push(Posting {
                    account,
                    debit_minor: if debit { amount } else { 0 },
                    credit_minor: if debit { 0 } else { amount },
                });
            }
        };
        let accounts = [
            Account::PublisherPayable,
            Account::PlatformRevenue,
            Account::TaxPayable,
        ];
        let (captured, reversed) = match e.kind {
            EventKind::Capture => {
                if self.captured
                    || e.amount_minor != self.quote.gross_minor
                    || e.occurred_at >= self.quote.access_until
                {
                    return Err(invalid("invalid capture transition"));
                }
                post(Account::ProviderClearing, e.amount_minor, true);
                for (account, amount) in accounts.into_iter().zip(self.quote.allocations()) {
                    post(account, amount, false);
                }
                (true, 0)
            }
            EventKind::Refund | EventKind::Chargeback => {
                if !self.captured || e.amount_minor > self.outstanding_minor() {
                    return Err(invalid("invalid reversal transition"));
                }
                if e.kind == EventKind::Chargeback && e.amount_minor != self.outstanding_minor() {
                    return Err(invalid("partial chargeback unsupported in sandbox"));
                }
                let next = self.reversed_minor + e.amount_minor;
                let before = self.reversed_allocation(self.reversed_minor);
                let after = self.reversed_allocation(next);
                for ((account, b), a) in accounts.into_iter().zip(before).zip(after) {
                    // Correct cumulative apportionment shifts can reverse a
                    // prior rounding cent; book the opposite posting explicitly.
                    post(account, (a - b).abs(), a >= b);
                }
                post(Account::ProviderClearing, e.amount_minor, false);
                (true, next)
            }
        };
        let debits: i128 = postings.iter().map(|p| i128::from(p.debit_minor)).sum();
        let credits: i128 = postings.iter().map(|p| i128::from(p.credit_minor)).sum();
        if debits != credits || debits == 0 {
            return Err(AppError::internal("unbalanced plugin ledger"));
        }
        let receipt = LedgerReceipt {
            event_id: e.provider_event_id.clone(),
            postings,
        };
        self.captured = captured;
        self.reversed_minor = reversed;
        self.events
            .insert(e.provider_event_id.clone(), (digest, receipt.clone()));
        Ok(receipt)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn fixture() -> (OrderLedger, SigningKey, DateTime<Utc>) {
        let now = DateTime::parse_from_rfc3339("2026-11-29T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let order = OrderLedger::new(Quote {
            order_id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            extension_key: "partner_demo".into(),
            currency: "VND".into(),
            gross_minor: 101,
            tax_minor: 7,
            publisher_bps: 7000,
            access_until: now + chrono::Duration::days(30),
        })
        .unwrap();
        (order, SigningKey::from_bytes(&[7; 32]), now)
    }
    fn event(
        o: &OrderLedger,
        key: &SigningKey,
        now: DateTime<Utc>,
        id: &str,
        kind: EventKind,
        amount: i64,
    ) -> VerifiedSandbox {
        let e = SandboxEvent {
            provider_event_id: id.into(),
            order_id: o.quote.order_id,
            organization_id: o.quote.organization_id,
            kind,
            currency: o.quote.currency.clone(),
            amount_minor: amount,
            occurred_at: now,
        };
        let sig = key.sign(
            &canonical(&SignedSandbox {
                domain: "openvietgis.payment.sandbox.v1",
                event: &e,
            })
            .unwrap(),
        );
        verify_sandbox(e, &key.verifying_key(), &sig, now).unwrap()
    }
    #[test]
    fn capture_refund_replay_and_exact_balance() {
        let (mut o, k, t) = fixture();
        let before = event(&o, &k, t, "refund-before", EventKind::Refund, 1);
        assert!(o.apply(&before).is_err());
        let capture = event(&o, &k, t, "capture", EventKind::Capture, 101);
        let receipt = o.apply(&capture).unwrap();
        assert_eq!(o.apply(&capture).unwrap(), receipt);
        let conflict = event(&o, &k, t, "capture", EventKind::Refund, 101);
        assert!(o.apply(&conflict).is_err());
        assert_eq!(o.entitlement(t), EntitlementState::Active);
        assert_eq!(
            o.entitlement(t + chrono::Duration::days(31)),
            EntitlementState::Expired
        );
        let mut receipts = vec![receipt];
        for i in 0..101 {
            let e = event(&o, &k, t, &format!("refund-{i}"), EventKind::Refund, 1);
            receipts.push(o.apply(&e).unwrap());
        }
        assert_eq!(o.entitlement(t), EntitlementState::Revoked);
        assert_eq!(o.outstanding_minor(), 0);
        for account in [
            Account::ProviderClearing,
            Account::PublisherPayable,
            Account::PlatformRevenue,
            Account::TaxPayable,
        ] {
            let total: i128 = receipts
                .iter()
                .flat_map(|r| &r.postings)
                .filter(|p| p.account == account)
                .map(|p| i128::from(p.debit_minor) - i128::from(p.credit_minor))
                .sum();
            assert_eq!(total, 0);
        }
        let extra = event(&o, &k, t, "extra", EventKind::Refund, 1);
        assert!(o.apply(&extra).is_err());
    }
    #[test]
    fn rejects_bad_signatures_and_scope() {
        let (mut o, k, t) = fixture();
        let mut e = event(&o, &k, t, "cap", EventKind::Capture, 101).0;
        let sig = k.sign(
            &canonical(&SignedSandbox {
                domain: "openvietgis.payment.sandbox.v1",
                event: &e,
            })
            .unwrap(),
        );
        e.amount_minor = 100;
        assert!(verify_sandbox(e, &k.verifying_key(), &sig, t).is_err());
        let e = event(&o, &k, t, "cap", EventKind::Capture, 101);
        let (mut other, _, _) = fixture();
        assert!(other.apply(&e).is_err());
        let wrong = event(&o, &k, t, "wrong", EventKind::Capture, 99);
        assert!(o.apply(&wrong).is_err());
        assert_eq!(o.entitlement(t), EntitlementState::Pending);
    }
}
