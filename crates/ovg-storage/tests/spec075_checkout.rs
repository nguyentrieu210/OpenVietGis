//! Run only with the existing disposable SPEC075 commerce fixture + 0055.
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use ovg_module_sdk::{
    commerce::{EventKind, Quote, SandboxEvent},
    payment_offer::{verify_offer, PriceOffer},
    release_v2::canonical,
};
use ovg_storage::{extensions::OrgExtensionScope, DbPool, DbPoolConfig};
use serde_json::json;
use uuid::Uuid;
fn id(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap()
}
#[tokio::test]
#[ignore = "requires disposable commerce fixture"]
async fn signed_checkout_atomic_acceptance_and_expiry() {
    let url = std::env::var("OVG075_COMMERCE_FIXTURE_URL").expect("disposable database");
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let org = id("11000000-0000-0000-0000-000000000001");
    let other = id("11000000-0000-0000-0000-000000000002");
    let user = id("51000000-0000-0000-0000-000000000001");
    let session = id("71000000-0000-0000-0000-000000000001");
    let price = SigningKey::from_bytes(&[111; 32]);
    let provider = SigningKey::from_bytes(&[112; 32]);
    let now = Utc::now() - Duration::seconds(1);
    let mut offer = PriceOffer {
        schema: "openvietgis.checkout.sandbox.offer.v1".into(),
        audience: "fixture".into(),
        release_digest: "a".repeat(64),
        publisher_id: "fixture".into(),
        quote: Quote {
            order_id: Uuid::new_v4(),
            organization_id: org,
            extension_key: "partner_fixture_context".into(),
            currency: "VND".into(),
            gross_minor: 101,
            tax_minor: 7,
            publisher_bps: 7000,
            access_until: now + Duration::days(30),
        },
        issued_at: now,
        expires_at: now + Duration::minutes(10),
    };
    let sig = price.sign(&canonical(&offer).unwrap());
    let proof = verify_offer(
        offer.clone(),
        &sig,
        &price.verifying_key(),
        "fixture",
        Utc::now(),
    )
    .unwrap();
    let open = || OrgExtensionScope::begin_human(&pool, org, user, session);
    let run = || async {
        let mut tx = open().await.unwrap();
        let v = tx
            .accept_sandbox_offer(&proof, &sig, &price.verifying_key(), Utc::now())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        v
    };
    let (a, b) = tokio::join!(run(), run());
    assert_ne!(a["replayed"], b["replayed"]);
    assert_eq!(a["offer_digest"], b["offer_digest"]);
    let mut tx = open().await.unwrap();
    assert!(tx
        .create_sandbox_order(&offer.quote, &offer.release_digest)
        .await
        .is_err());
    drop(tx);
    offer.quote.gross_minor = 100;
    let modified_sig = price.sign(&canonical(&offer).unwrap());
    let modified = verify_offer(
        offer.clone(),
        &modified_sig,
        &price.verifying_key(),
        "fixture",
        Utc::now(),
    )
    .unwrap();
    let mut tx = open().await.unwrap();
    assert!(tx
        .accept_sandbox_offer(&modified, &modified_sig, &price.verifying_key(), Utc::now())
        .await
        .is_err());
    drop(tx);
    offer.quote.gross_minor = 101;
    let event = |event_id: &str, occurred_at, amount_minor, kind| SandboxEvent {
        provider_event_id: event_id.into(),
        order_id: offer.quote.order_id,
        organization_id: org,
        kind,
        currency: "VND".into(),
        amount_minor,
        occurred_at,
    };
    let sign = |e: &SandboxEvent| {
        provider.sign(
            &canonical(&json!({"domain":"openvietgis.payment.sandbox.v1","event":e})).unwrap(),
        )
    };
    let expired = event("expired", offer.expires_at, 101, EventKind::Capture);
    let mut tx = open().await.unwrap();
    assert!(tx
        .record_sandbox_event(
            expired.clone(),
            &sign(&expired),
            &provider.verifying_key(),
            offer.expires_at
        )
        .await
        .is_err());
    drop(tx);
    let premature = event("premature", now, 101, EventKind::Capture);
    let mut tx = open().await.unwrap();
    assert!(tx
        .record_sandbox_event(
            premature.clone(),
            &sign(&premature),
            &provider.verifying_key(),
            Utc::now()
        )
        .await
        .is_err());
    drop(tx);
    let capture = event("capture-signed", Utc::now(), 101, EventKind::Capture);
    let mut tx = open().await.unwrap();
    let r = tx
        .record_sandbox_event(
            capture.clone(),
            &sign(&capture),
            &provider.verifying_key(),
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(r["outstanding_minor"], 101);
    tx.commit().await.unwrap();
    pool.inner().close().await;
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let snap = tx
        .sandbox_snapshot(offer.quote.order_id, &provider.verifying_key(), Utc::now())
        .await
        .unwrap();
    assert_eq!(snap["checkout_offer_digest"], proof.digest());
    assert_eq!(snap["outstanding_minor"], 101);
    let report = tx
        .sandbox_settlement(offer.quote.order_id, &provider.verifying_key(), Utc::now())
        .await
        .unwrap();
    assert_eq!(report.clearing_minor, 101);
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, other, user, session)
        .await
        .unwrap();
    assert!(tx
        .accept_sandbox_offer(&proof, &sig, &price.verifying_key(), Utc::now())
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
        .accept_sandbox_offer(&proof, &sig, &price.verifying_key(), Utc::now())
        .await
        .is_err());
    drop(tx);
}
