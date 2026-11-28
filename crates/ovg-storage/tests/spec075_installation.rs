//! Only against fresh disposable graph fixture and exact migrations.
use ovg_storage::{
    catalog::{ProjectRead, ProjectReadPolicy},
    extensions::{
        installation::{project_pins, switch_graph, GraphRelease},
        OrgExtensionScope,
    },
    DbPool, DbPoolConfig,
};
use serde_json::{json, Value};
use uuid::Uuid;
fn id(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap()
}
fn graph(version: &str) -> Vec<GraphRelease> {
    let f: Value = serde_json::from_str(include_str!(
        "../../../fixtures/spec075/python-signed-catalog.json"
    ))
    .unwrap();
    ["partner_fixture_context", "partner_fixture_layers"]
        .into_iter()
        .map(|key| {
            let mut m = f["v1"]["payload"]["releases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["manifest"]["extension_key"] == key)
                .unwrap()["manifest"]
                .clone();
            m["version"] = json!(version);
            GraphRelease {
                manifest: m,
                release_digest: if version == "1.0.0" {
                    "a".repeat(64)
                } else {
                    "b".repeat(64)
                },
            }
        })
        .collect()
}
#[tokio::test]
#[ignore = "requires disposable SPEC075 graph fixture"]
async fn atomic_install_switch_rollback_and_cas() {
    let pool = DbPool::connect(
        &std::env::var("OVG075_GRAPH_FIXTURE_URL").expect("disposable URL"),
        DbPoolConfig::default(),
    )
    .await
    .unwrap();
    let org = id("11000000-0000-0000-0000-000000000001");
    let user = id("51000000-0000-0000-0000-000000000001");
    let session = id("71000000-0000-0000-0000-000000000001");
    let project = id("31000000-0000-0000-0000-000000000001");
    let keys = vec![
        "partner_fixture_context".into(),
        "partner_fixture_layers".into(),
    ];
    let first = graph("1.0.0");
    let second = graph("1.1.0");
    let req = Uuid::new_v4();
    let d1 = "1".repeat(64);
    let d2 = "2".repeat(64);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    let r = tx.graph_install(req, &d1, 2, &first).await.unwrap();
    assert_eq!(r["items"].as_array().unwrap().len(), 2);
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert_eq!(tx.graph_install(req, &d1, 2, &first).await.unwrap(), r);
    tx.commit().await.unwrap();
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert!(tx.graph_install(req, &d2, 2, &first).await.is_err());
    drop(tx);
    // Invalid second manifest must roll back a successfully inserted first manifest.
    let mut bad = second.clone();
    bad[1].manifest["display_name"] = json!("");
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert!(tx
        .graph_install(Uuid::new_v4(), &d2, 2, &bad)
        .await
        .is_err());
    drop(tx);
    let mut tx = OrgExtensionScope::begin_human(&pool, org, user, session)
        .await
        .unwrap();
    assert_eq!(tx.list_manifests().await.unwrap().len(), 2);
    tx.graph_install(Uuid::new_v4(), &d2, 2, &second)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let open = || {
        ProjectRead::begin(
            &pool,
            org,
            project,
            user,
            session,
            ProjectReadPolicy::UnrestrictedManage,
        )
    };
    let mut tx = open().await.unwrap();
    let r = switch_graph(
        &mut tx,
        Uuid::new_v4(),
        &d1,
        2,
        &first,
        &[],
        None,
        "fixture",
    )
    .await
    .unwrap();
    assert_eq!(r["after"].as_array().unwrap().len(), 2);
    tx.commit().await.unwrap();
    let mut tx = open().await.unwrap();
    let before = project_pins(&mut tx, &keys).await.unwrap();
    let upgrade = Uuid::new_v4();
    let result = switch_graph(&mut tx, upgrade, &d2, 3, &second, &before, None, "fixture")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = open().await.unwrap();
    assert_eq!(
        switch_graph(&mut tx, upgrade, &d2, 3, &second, &before, None, "fixture")
            .await
            .unwrap(),
        result
    );
    tx.commit().await.unwrap();
    let mut tx = open().await.unwrap();
    assert!(switch_graph(
        &mut tx,
        Uuid::new_v4(),
        &d1,
        3,
        &first,
        &before,
        None,
        "fixture"
    )
    .await
    .is_err());
    drop(tx);
    let mut tx = open().await.unwrap();
    let after = project_pins(&mut tx, &keys).await.unwrap();
    assert!(after.iter().all(|p| p.revision == 2));
    assert!(switch_graph(
        &mut tx,
        Uuid::new_v4(),
        &d1,
        3,
        &first,
        &after,
        None,
        "fixture"
    )
    .await
    .is_err());
    drop(tx);
    let mut tx = open().await.unwrap();
    let rollback = switch_graph(
        &mut tx,
        Uuid::new_v4(),
        &d1,
        3,
        &first,
        &after,
        Some(upgrade),
        "fixture",
    )
    .await
    .unwrap();
    assert_eq!(rollback["grants_created"], false);
    tx.commit().await.unwrap();
    let mut tx = open().await.unwrap();
    let final_pins = project_pins(&mut tx, &keys).await.unwrap();
    for (a, b) in final_pins.iter().zip(before) {
        assert_eq!(a.manifest_id, b.manifest_id);
        assert_eq!(a.revision, 3);
    }
    tx.commit().await.unwrap();
}
