//! Only run against an isolated fresh commerce fixture and exact 0061 DDL.
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use ovg_module_sdk::{
    entitlement::{verify_grant, CommercialPolicy, GrantKind, LicenseGrant, LicenseState},
    release_v2::canonical,
    trust::fingerprint,
};
use ovg_storage::{extensions::OrgExtensionScope, DbPool, DbPoolConfig};
use uuid::Uuid;
fn id(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap()
}
#[tokio::test]
#[ignore = "requires disposable SPEC075 entitlement fixture"]
async fn immutable_certificate_lifecycle_and_tenant_denial() {
    let url = std::env::var("OVG075_LICENSE_FIXTURE_URL").expect("disposable DB URL");
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let org = id("11000000-0000-0000-0000-000000000001");
    let other = id("11000000-0000-0000-0000-000000000002");
    let user = id("51000000-0000-0000-0000-000000000001");
    let session = id("71000000-0000-0000-0000-000000000001");
    let k = SigningKey::from_bytes(&[103; 32]);
    let key = k.verifying_key();
    let p = CommercialPolicy {
        audience: "fixture".into(),
        authority_key_id: fingerprint(key.as_bytes()),
        licensed_release_digests: vec!["a".repeat(64), "b".repeat(64)],
    };
    let now = Utc::now() - Duration::seconds(1);
    let mut g = LicenseGrant {
        schema: "openvietgis.license.grant.v1".into(),
        audience: p.audience.clone(),
        authority_key_id: p.authority_key_id.clone(),
        grant_id: Uuid::new_v4(),
        organization_id: org,
        extension_key: "partner_fixture_context".into(),
        release_digest: "a".repeat(64),
        sequence: 1,
        kind: GrantKind::Trial,
        issued_at: now,
        valid_from: now,
        valid_until: now + Duration::days(14),
        grace_until: now + Duration::days(14),
    };
    let sign = |g: &LicenseGrant| {
        let sig = k.sign(&canonical(g).unwrap());
        (
            verify_grant(g.clone(), &sig, &key, &p, org, Utc::now()).unwrap(),
            sig,
        )
    };
    let (trial, sig) = sign(&g);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert_eq!(
        tx.license_status(&g.release_digest, &g.extension_key, &p, &key)
            .await
            .unwrap(),
        LicenseState::Missing
    );
    assert_eq!(
        tx.import_license(&trial, &sig).await.unwrap()["state"],
        "trial"
    );
    assert_eq!(
        tx.import_license(&trial, &sig).await.unwrap()["replayed"],
        true
    );
    tx.commit().await.unwrap();
    let mut repeat = g.clone();
    repeat.grant_id = Uuid::new_v4();
    repeat.release_digest = "b".repeat(64);
    let (r, s) = sign(&repeat);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert!(tx.import_license(&r, &s).await.is_err());
    drop(tx);
    g.grant_id = Uuid::new_v4();
    g.sequence = 2;
    g.kind = GrantKind::Subscription;
    g.valid_until = now + Duration::days(30);
    g.grace_until = g.valid_until + Duration::days(7);
    let (sub, s) = sign(&g);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert_eq!(
        tx.import_license(&sub, &s).await.unwrap()["state"],
        "active"
    );
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert!(tx.import_license(&trial, &sig).await.is_err());
    drop(tx);
    g.grant_id = Uuid::new_v4();
    g.sequence = 3;
    g.kind = GrantKind::Revoked;
    g.valid_until = now;
    g.grace_until = now;
    let (rev, s) = sign(&g);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert_eq!(
        tx.import_license(&rev, &s).await.unwrap()["state"],
        "revoked"
    );
    tx.commit().await.unwrap();
    pool.inner().close().await;
    let pool = DbPool::connect(&url, DbPoolConfig::default())
        .await
        .unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert_eq!(
        tx.license_status(&g.release_digest, &g.extension_key, &p, &key)
            .await
            .unwrap(),
        LicenseState::Revoked
    );
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, other, user, session)
        .await
        .unwrap();
    assert_eq!(
        tx.license_status(&g.release_digest, &g.extension_key, &p, &key)
            .await
            .unwrap(),
        LicenseState::Missing
    );
    assert!(tx.import_license(&rev, &s).await.is_err());
    drop(tx);
    let mut tx = OrgExtensionScope::begin_human(
        &pool,
        org,
        id("51000000-0000-0000-0000-000000000002"),
        id("71000000-0000-0000-0000-000000000002"),
    )
    .await
    .unwrap();
    assert!(tx.import_license(&rev, &s).await.is_err());
    drop(tx);
}
