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
    for (method, suffix) in [("GET", ""), ("POST", "/invoke"), ("POST", "/grant")] {
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
