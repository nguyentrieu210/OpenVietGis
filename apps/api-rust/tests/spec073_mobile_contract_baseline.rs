//! Baseline regression: mobile must not downgrade browser or service auth.
use axum::body::Body;
use http::{Request, StatusCode};
use openvietgis_api_rust::{build_router, AppConfig, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn spec073_human_identity_never_falls_back_from_bearer_to_cookie() {
    let app = build_router(AppState::new(AppConfig::default(), None).unwrap());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/me")
                .header("Authorization", "Bearer untrusted-mobile-test")
                .header("Cookie", "ovg_session=untrusted-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["detail"], "invalid service credential");
}

#[tokio::test]
async fn spec073_service_routes_fail_closed_for_invalid_organization_bearer() {
    let app = build_router(AppState::new(AppConfig::default(), None).unwrap());
    for path in [
        "/api/v1/organizations/not-a-uuid/extensions/manifests",
        "/api/v1/organizations/not-a-uuid/projects/not-a-uuid/integration-hub/managed/delta",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("Authorization", "Bearer untrusted-mobile-test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}
