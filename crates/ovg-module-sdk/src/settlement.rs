//! Sandbox accounting projection and provider statement comparison.
//! No payout, banking instructions, or production payment authority.
use crate::commerce::{Account, LedgerReceipt};
use crate::release_v2::canonical;
use crate::trust::fingerprint;
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub currency: String,
    pub event_count: usize,
    pub event_digest: String,
    pub clearing_minor: i64,
    pub publisher_payable_minor: i64,
    pub platform_revenue_minor: i64,
    pub tax_payable_minor: i64,
}
fn invalid() -> AppError {
    AppError::validation(
        "INVALID_SETTLEMENT",
        "invalid sandbox accounting projection",
    )
}
impl Settlement {
    pub fn from_receipts(currency: &str, receipts: &[LedgerReceipt]) -> Result<Self, AppError> {
        if currency.len() != 3
            || !currency.bytes().all(|b| b.is_ascii_uppercase())
            || receipts.len() > 1024
        {
            return Err(invalid());
        }
        let mut ids = BTreeSet::new();
        let mut totals = [0i128; 4];
        for receipt in receipts {
            if receipt.event_id.is_empty()
                || !ids.insert(receipt.event_id.as_str())
                || receipt.postings.is_empty()
                || receipt.postings.len() > 8
            {
                return Err(invalid());
            }
            let mut balance = 0i128;
            for p in &receipt.postings {
                if p.debit_minor < 0
                    || p.credit_minor < 0
                    || (p.debit_minor == 0) == (p.credit_minor == 0)
                {
                    return Err(invalid());
                }
                let v = i128::from(p.debit_minor) - i128::from(p.credit_minor);
                let i = match p.account {
                    Account::ProviderClearing => 0,
                    Account::PublisherPayable => 1,
                    Account::PlatformRevenue => 2,
                    Account::TaxPayable => 3,
                };
                totals[i] += if i == 0 { v } else { -v };
                balance += v;
            }
            if balance != 0 {
                return Err(invalid());
            }
        }
        if totals.iter().any(|v| *v < 0 || *v > 9_007_199_254_740_991)
            || totals[0] != totals[1] + totals[2] + totals[3]
        {
            return Err(invalid());
        }
        Ok(Self {
            currency: currency.into(),
            event_count: receipts.len(),
            event_digest: fingerprint(&canonical(&receipts)?),
            clearing_minor: totals[0] as i64,
            publisher_payable_minor: totals[1] as i64,
            platform_revenue_minor: totals[2] as i64,
            tax_payable_minor: totals[3] as i64,
        })
    }
    /// External statement comparison is evidence only, never a financial mutation.
    pub fn reconcile(&self, statement: &Self) -> Reconciliation {
        let mismatches = [
            ("currency", self.currency != statement.currency),
            ("event_count", self.event_count != statement.event_count),
            ("event_digest", self.event_digest != statement.event_digest),
            (
                "clearing_minor",
                self.clearing_minor != statement.clearing_minor,
            ),
            (
                "publisher_payable_minor",
                self.publisher_payable_minor != statement.publisher_payable_minor,
            ),
            (
                "platform_revenue_minor",
                self.platform_revenue_minor != statement.platform_revenue_minor,
            ),
            (
                "tax_payable_minor",
                self.tax_payable_minor != statement.tax_payable_minor,
            ),
        ]
        .into_iter()
        .filter_map(|(name, diff)| diff.then_some(name))
        .collect::<Vec<_>>();
        Reconciliation {
            matched: mismatches.is_empty(),
            mismatches,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Reconciliation {
    pub matched: bool,
    pub mismatches: Vec<&'static str>,
}
/// Synthetic statement signed independently of the stored ledger. The
/// provider test key is pinned by the existing sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedStatement {
    pub schema: String,
    pub organization_id: uuid::Uuid,
    pub order_id: uuid::Uuid,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub report: Settlement,
}
pub fn verify_statement(
    s: &SignedStatement,
    sig: &ed25519_dalek::Signature,
    key: &ed25519_dalek::VerifyingKey,
    org: uuid::Uuid,
    order: uuid::Uuid,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    let deny = || AppError::forbidden("invalid signed sandbox statement");
    if key.is_weak()
        || s.schema != "openvietgis.settlement.sandbox.statement.v1"
        || s.organization_id != org
        || s.order_id != order
        || org.is_nil()
        || order.is_nil()
        || s.observed_at > now + chrono::Duration::seconds(30)
        || s.observed_at < now - chrono::Duration::minutes(5)
        || !crate::entitlement::valid_hash(&s.report.event_digest)
        || s.report.currency.len() != 3
        || !s.report.currency.bytes().all(|b| b.is_ascii_uppercase())
        || s.report.event_count > 1024
        || [
            s.report.clearing_minor,
            s.report.publisher_payable_minor,
            s.report.platform_revenue_minor,
            s.report.tax_payable_minor,
        ]
        .iter()
        .any(|v| !(0..=9_007_199_254_740_991).contains(v))
    {
        return Err(deny());
    }
    key.verify_strict(&canonical(s)?, sig).map_err(|_| deny())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commerce::Posting;
    fn p(account: Account, debit_minor: i64, credit_minor: i64) -> Posting {
        Posting {
            account,
            debit_minor,
            credit_minor,
        }
    }
    #[test]
    fn statement_signature_scope_and_freshness() {
        use ed25519_dalek::{Signer, SigningKey};
        let now = chrono::Utc::now();
        let key = SigningKey::from_bytes(&[106; 32]);
        let org = uuid::Uuid::new_v4();
        let order = uuid::Uuid::new_v4();
        let mut s = SignedStatement {
            schema: "openvietgis.settlement.sandbox.statement.v1".into(),
            organization_id: org,
            order_id: order,
            observed_at: now,
            report: Settlement::from_receipts("VND", &[]).unwrap(),
        };
        let sig = key.sign(&canonical(&s).unwrap());
        assert!(verify_statement(&s, &sig, &key.verifying_key(), org, order, now).is_ok());
        assert!(verify_statement(
            &s,
            &sig,
            &key.verifying_key(),
            uuid::Uuid::new_v4(),
            order,
            now
        )
        .is_err());
        assert!(verify_statement(
            &s,
            &sig,
            &key.verifying_key(),
            org,
            order,
            now + chrono::Duration::minutes(6)
        )
        .is_err());
        s.report.clearing_minor = 1;
        assert!(verify_statement(&s, &sig, &key.verifying_key(), org, order, now).is_err());
    }
    #[test]
    fn projection_and_reconciliation() {
        let capture = LedgerReceipt {
            event_id: "capture".into(),
            postings: vec![
                p(Account::ProviderClearing, 100, 0),
                p(Account::PublisherPayable, 0, 63),
                p(Account::PlatformRevenue, 0, 27),
                p(Account::TaxPayable, 0, 10),
            ],
        };
        let refund = LedgerReceipt {
            event_id: "refund".into(),
            postings: vec![
                p(Account::ProviderClearing, 0, 40),
                p(Account::PublisherPayable, 25, 0),
                p(Account::PlatformRevenue, 11, 0),
                p(Account::TaxPayable, 4, 0),
            ],
        };
        let r = Settlement::from_receipts("VND", &[capture.clone(), refund.clone()]).unwrap();
        assert_eq!(
            (
                r.clearing_minor,
                r.publisher_payable_minor,
                r.platform_revenue_minor,
                r.tax_payable_minor
            ),
            (60, 38, 16, 6)
        );
        assert!(r.reconcile(&r).matched);
        let mut wrong = r.clone();
        wrong.tax_payable_minor += 1;
        assert_eq!(r.reconcile(&wrong).mismatches, vec!["tax_payable_minor"]);
        assert!(Settlement::from_receipts("VND", &[capture.clone(), capture.clone()]).is_err());
        let mut bad = refund;
        bad.postings[0].credit_minor += 1;
        assert!(Settlement::from_receipts("VND", &[capture, bad]).is_err());
    }
}
