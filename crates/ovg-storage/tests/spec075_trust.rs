//! Actual checkpoint concurrency and persistence test against a disposable DB.
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use ovg_module_sdk::{
    release_v2::canonical,
    trust::{fingerprint, verify_policy, TrustPolicy, VerifiedTrust},
};
use ovg_storage::{extensions::trust::admit_policy, DbPool, DbPoolConfig};
fn signed(p: &TrustPolicy, root: &SigningKey, op: &SigningKey) -> VerifiedTrust {
    verify_policy(
        p.clone(),
        &root.sign(&canonical(p).unwrap()),
        &root.verifying_key(),
        &op.verifying_key(),
        p.catalog_epoch,
        &p.catalog_membership_digest,
        Utc::now(),
    )
    .unwrap()
}
#[tokio::test]
#[ignore = "requires fresh disposable global trust fixture"]
async fn global_checkpoint_survives_restart_and_denies_rollback() {
    let url = std::env::var("OVG075_TRUST_FIXTURE_URL").expect("disposable fixture");
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let root = SigningKey::from_bytes(&[71; 32]);
    let op = SigningKey::from_bytes(&[72; 32]);
    let next = SigningKey::from_bytes(&[73; 32]);
    let now = Utc::now();
    let mut p = TrustPolicy {
        schema: "openvietgis.trust.policy.v1".into(),
        sequence: 1,
        catalog_epoch: 2,
        catalog_membership_digest: "a".repeat(64),
        operator_key_id: fingerprint(op.verifying_key().as_bytes()),
        issued_at: (now - chrono::Duration::seconds(1)).to_rfc3339(),
        expires_at: (now + chrono::Duration::days(1)).to_rfc3339(),
        revoked_release_digests: vec![],
        revoked_publisher_keys: vec![],
        revoked_operator_keys: vec![],
        commercial: None,
    };
    let old = signed(&p, &root, &op);
    let (a, b) = tokio::join!(admit_policy(&pool, &old), admit_policy(&pool, &old));
    a.unwrap();
    b.unwrap();
    p.sequence = 2;
    p.revoked_release_digests = vec!["b".repeat(64)];
    p.commercial = Some(ovg_module_sdk::entitlement::CommercialPolicy {
        audience: "fixture".into(),
        authority_key_id: fingerprint(
            SigningKey::from_bytes(&[113; 32])
                .verifying_key()
                .as_bytes(),
        ),
        licensed_release_digests: vec!["e".repeat(64)],
        revocation_floors: vec![ovg_module_sdk::entitlement::LicenseRevocationFloor {
            organization_id: uuid::Uuid::new_v4(),
            release_digest: "e".repeat(64),
            through_sequence: 2,
        }],
    });
    let revoked = signed(&p, &root, &op);
    admit_policy(&pool, &revoked).await.unwrap();
    assert!(admit_policy(&pool, &old).await.is_err());
    let mut bad = p.clone();
    bad.revoked_release_digests.clear();
    bad.sequence = 3;
    assert!(admit_policy(&pool, &signed(&bad, &root, &op))
        .await
        .is_err());
    bad = p.clone();
    bad.sequence = 3;
    bad.commercial.as_mut().unwrap().revocation_floors[0].through_sequence = 1;
    assert!(admit_policy(&pool, &signed(&bad, &root, &op))
        .await
        .is_err());
    bad.commercial.as_mut().unwrap().revocation_floors.clear();
    assert!(admit_policy(&pool, &signed(&bad, &root, &op))
        .await
        .is_err());
    bad = p.clone();
    bad.catalog_membership_digest = "c".repeat(64);
    assert!(admit_policy(&pool, &signed(&bad, &root, &op))
        .await
        .is_err());
    p.sequence = 3;
    p.catalog_epoch = 3;
    p.catalog_membership_digest = "d".repeat(64);
    p.revoked_operator_keys = vec![p.operator_key_id.clone()];
    p.operator_key_id = fingerprint(next.verifying_key().as_bytes());
    let rotated = signed(&p, &root, &next);
    admit_policy(&pool, &rotated).await.unwrap();
    pool.inner().close().await;
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    assert!(admit_policy(&pool, &revoked).await.is_err());
    admit_policy(&pool, &rotated).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM extensions.plugin_trust_checkpoints")
        .fetch_one(pool.inner())
        .await
        .unwrap();
    assert_eq!(rows, 3);
    assert!(
        sqlx::query("UPDATE extensions.plugin_trust_checkpoints SET catalog_epoch=1")
            .execute(pool.inner())
            .await
            .is_err()
    );
}
