use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use openvietgis_api_rust::{routes, AppConfig, AppState};
use serde_json::Value;
use tower::ServiceExt;
#[tokio::test]
async fn runtime_default_disabled_and_catalog_does_not_advertise_execution() {
    let app = Router::new()
        .nest("/api/v1", routes::api_v1_router())
        .with_state(AppState::new(AppConfig::default(), None).unwrap());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/marketplace/v2/catalog?host=web&api=1.0.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(value["items"], serde_json::json!([]));
    assert_eq!(value["web_read_only_enabled"], false);
    assert_eq!(value["host_availability"]["reason"], "RUNTIME_DISABLED");
    for (platform, reason) in [
        ("android", "NATIVE_HUMAN_TRANSPORT_NOT_QUALIFIED"),
        ("ios", "NATIVE_HUMAN_TRANSPORT_NOT_QUALIFIED"),
        ("windows", "DESKTOP_CLOUD_TRANSPORT_NOT_QUALIFIED"),
        ("macos", "DESKTOP_CLOUD_TRANSPORT_NOT_QUALIFIED"),
        ("linux", "DESKTOP_CLOUD_TRANSPORT_NOT_QUALIFIED"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/marketplace/v2/catalog?host={platform}&api=1.0.0"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let v: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(v["host_availability"]["online_execution"], false);
        assert_eq!(v["host_availability"]["offline_execution"], false);
        assert_eq!(v["host_availability"]["reason"], reason);
    }
    for (method, suffix) in [
        ("GET", ""),
        ("POST", "/invoke"),
        ("POST", "/grant"),
        ("POST", "/processing/confirm"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!(
                        "/api/v1/organizations/o/projects/p/extensions/m/v2-runtime{suffix}"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn sandbox_commerce_disabled_without_dedicated_key() {
    let app = Router::new()
        .nest("/api/v1", routes::api_v1_router())
        .with_state(AppState::new(AppConfig::default(), None).unwrap());
    let base =
        "/api/v1/organizations/11000000-0000-0000-0000-000000000001/marketplace/sandbox/orders";
    for (method, path) in [
        ("POST", base.to_owned()),
        ("POST", "/api/v1/organizations/11000000-0000-0000-0000-000000000001/marketplace/sandbox/offers/preview".into()),
        ("GET", format!("{base}/order")),
        ("POST", format!("{base}/order/events")),
        ("GET", format!("{base}/order/settlement")),
        ("POST", format!("{base}/order/reconcile")),
        ("POST", format!("{base}/order/reconcile-signed")),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn graph_routes_disabled_by_default() {
    let app = Router::new()
        .nest("/api/v1", routes::api_v1_router())
        .with_state(AppState::new(AppConfig::default(), None).unwrap());
    for path in [
        "/organizations/o/marketplace/v2/graph/preview",
        "/organizations/o/marketplace/v2/graph/install",
        "/organizations/o/projects/p/extensions/v2-graph/preview",
        "/organizations/o/projects/p/extensions/v2-graph/apply",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1{path}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
