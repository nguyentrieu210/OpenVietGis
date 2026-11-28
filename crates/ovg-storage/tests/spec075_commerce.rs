//! Only run against a freshly created disposable database with commerce_fixture.sql
//! and exact migration 0055. Never run against shared/production databases.
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use ovg_module_sdk::{
    commerce::{EventKind, Quote, SandboxEvent},
    release_v2::canonical,
};
use ovg_storage::{extensions::OrgExtensionScope, DbPool, DbPoolConfig};
use serde_json::json;
use uuid::Uuid;
fn id(v: &str) -> Uuid {
    Uuid::parse_str(v).unwrap()
}
#[tokio::test]
#[ignore = "requires disposable SPEC075 commerce fixture"]
async fn persistent_replay_refund_and_tenant_denial() {
    let url = std::env::var("OVG075_COMMERCE_FIXTURE_URL").expect("disposable fixture URL");
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let org = id("11000000-0000-0000-0000-000000000001");
    let other = id("11000000-0000-0000-0000-000000000002");
    let user = id("51000000-0000-0000-0000-000000000001");
    let session = id("71000000-0000-0000-0000-000000000001");
    let now = Utc::now() - Duration::seconds(1);
    let order = Uuid::new_v4();
    let key = SigningKey::from_bytes(&[51; 32]);
    let pubkey = key.verifying_key();
    let quote = Quote {
        order_id: order,
        organization_id: org,
        extension_key: "partner_fixture_context".into(),
        currency: "VND".into(),
        gross_minor: 101,
        tax_minor: 7,
        publisher_bps: 7000,
        access_until: now + Duration::days(1),
    };
    let digest = "a".repeat(64);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    tx.create_sandbox_order(&quote, &digest).await.unwrap();
    tx.create_sandbox_order(&quote, &digest).await.unwrap();
    let empty = tx
        .sandbox_settlement(order, &pubkey, Utc::now())
        .await
        .unwrap();
    assert_eq!(empty.clearing_minor, 0);
    assert_eq!(empty.event_count, 0);
    tx.commit().await.unwrap();
    let make = |name: &str, kind, amount| {
        let e = SandboxEvent {
            provider_event_id: name.into(),
            order_id: order,
            organization_id: org,
            kind,
            currency: "VND".into(),
            amount_minor: amount,
            occurred_at: now,
        };
        let sig = key.sign(
            &canonical(&json!({"domain":"openvietgis.payment.sandbox.v1","event":e})).unwrap(),
        );
        (e, sig)
    };
    let (capture, sig) = make("capture", EventKind::Capture, 101);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let r = tx
        .record_sandbox_event(capture.clone(), &sig, &pubkey, Utc::now())
        .await
        .unwrap();
    assert_eq!(r["entitlement"], "active");
    assert_eq!(r["replayed"], false);
    tx.commit().await.unwrap();
    let retry = || async {
        let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
            .await
            .unwrap();
        let result = tx
            .record_sandbox_event(capture.clone(), &sig, &pubkey, Utc::now())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    let (a, b) = tokio::join!(retry(), retry());
    assert_eq!(a["replayed"], true);
    assert_eq!(b["replayed"], true);
    assert_eq!(a["receipt"], b["receipt"]);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let (changed, changed_sig) = make("capture", EventKind::Refund, 1);
    assert!(tx
        .record_sandbox_event(changed, &changed_sig, &pubkey, Utc::now())
        .await
        .is_err());
    drop(tx);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let (e, s) = make("refund", EventKind::Refund, 41);
    tx.record_sandbox_event(e, &s, &pubkey, Utc::now())
        .await
        .unwrap();
    let report = tx
        .sandbox_settlement(order, &pubkey, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.clearing_minor, 60);
    assert_eq!(report.publisher_payable_minor, 39);
    assert_eq!(report.platform_revenue_minor, 16);
    assert_eq!(report.tax_payable_minor, 5);
    assert!(report.reconcile(&report).matched);
    let mut changed = report.clone();
    changed.platform_revenue_minor += 1;
    assert!(!report.reconcile(&changed).matched);
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let (e, s) = make("chargeback", EventKind::Chargeback, 60);
    tx.record_sandbox_event(e, &s, &pubkey, Utc::now())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // New connection pool proves reconstruction from persisted signed events.
    pool.inner().close().await;
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let v = tx
        .sandbox_snapshot(order, &pubkey, Utc::now())
        .await
        .unwrap();
    assert_eq!(v["entitlement"], "revoked");
    assert_eq!(v["event_count"], 3);
    assert_eq!(v["outstanding_minor"], 0);
    let report = tx
        .sandbox_settlement(order, &pubkey, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.event_count, 3);
    assert_eq!(
        (
            report.clearing_minor,
            report.publisher_payable_minor,
            report.platform_revenue_minor,
            report.tax_payable_minor
        ),
        (0, 0, 0, 0)
    );
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, other, user, session)
        .await
        .unwrap();
    assert!(tx
        .sandbox_snapshot(order, &pubkey, Utc::now())
        .await
        .is_err());
    drop(tx);
    let mut tx = OrgExtensionScope::begin_human(
        &pool,
        org,
        id("51000000-0000-0000-0000-000000000002"),
        id("71000000-0000-0000-0000-000000000002"),
    )
    .await
    .unwrap();
    assert!(tx
        .sandbox_snapshot(order, &pubkey, Utc::now())
        .await
        .is_err());
    drop(tx);
}
