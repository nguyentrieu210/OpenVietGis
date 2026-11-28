//! Verify mounted method/path coverage without executing any business handler.
#![cfg(feature = "house-numbering")]

use std::collections::BTreeSet;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use openvietgis_api_rust::{routes, AppConfig, AppState};
use serde_json::Value;
use tower::ServiceExt;

fn sample_path(mut path: &str) -> String {
    let mut result = String::new();
    while let Some((prefix, rest)) = path.split_once('{') {
        let (parameter, suffix) = rest.split_once('}').expect("closed route parameter");
        assert!(!parameter.is_empty(), "empty route parameter");
        result.push_str(prefix);
        result.push('1');
        path = suffix;
    }
    result.push_str(path);
    result
}

fn baseline_operations() -> BTreeSet<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs/064-rust-platform/contracts/engines");
    let mut expected = BTreeSet::new();
    let engine_counts = [
        54, 22, 5, 4, 7, 12, 11, 14, 8, 30, 57, 26, 6, 27, 72, 0, 0, 8,
    ];
    for (index, expected_count) in engine_counts.into_iter().enumerate() {
        let engine = index + 1;
        let mut count = 0;
        let path = root.join(format!("e{engine:02}.json"));
        let inventory: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read engine inventory"))
                .expect("parse engine inventory");
        let operations = inventory
            .get("operations")
            .or_else(|| inventory.get("http_routes"))
            .and_then(Value::as_array)
            .expect("known engine HTTP inventory schema");
        for operation in operations {
            let route = operation["path"].as_str().expect("inventory route path");
            if !route.starts_with("/api/v1/") {
                continue;
            }
            let methods: Vec<&str> = if let Some(method) = operation["method"].as_str() {
                vec![method]
            } else {
                operation["methods"]
                    .as_array()
                    .expect("inventory methods")
                    .iter()
                    .map(|value| value.as_str().expect("HTTP method"))
                    .collect()
            };
            assert!(!methods.is_empty(), "no methods for {route}");
            for method in methods {
                count += 1;
                expected.insert((method.to_ascii_uppercase(), route.to_owned()));
            }
        }
        assert_eq!(count, expected_count, "frozen E{engine:02} inventory drift");
    }
    // The baseline WebSocket is also an HTTP GET upgrade route.
    expected.insert((
        "GET".to_owned(),
        "/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/presence"
            .to_owned(),
    ));
    expected
}

// A deliberately unsupported extension method reaches Axum's method fallback,
// which returns the registered methods in Allow. Unlike Router::route_layer,
// this does not wrap the fallback or execute any business handler.
async fn probe(app: &Router, path: &str) -> (StatusCode, BTreeSet<String>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::from_bytes(b"OVG-PROBE").expect("valid extension method"))
                .uri(path)
                .body(Body::empty())
                .expect("valid sample request"),
        )
        .await
        .expect("infallible router");
    let allowed = response
        .headers()
        .get_all(header::ALLOW)
        .iter()
        .flat_map(|value| value.to_str().expect("valid Allow header").split(','))
        .map(|method| method.trim().to_owned())
        .collect();
    (response.status(), allowed)
}

#[tokio::test]
async fn frozen_api_inventory_is_mounted_on_full_profile() {
    let state = AppState::new(AppConfig::default(), None).expect("database-free state");
    let app = Router::new()
        .nest("/api/v1", routes::api_v1_router())
        .with_state(state);

    let (status, allowed) = probe(&app, "/api/v1/health/live").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(allowed.contains("GET"));
    assert!(!allowed.contains("POST"));
    let (status, allowed) = probe(&app, "/api/v1/spec066-unknown-route").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(allowed.is_empty());

    let expected = baseline_operations();
    let mut missing = Vec::new();
    for (method, route) in &expected {
        let (status, allowed) = probe(&app, &sample_path(route)).await;
        if status != StatusCode::METHOD_NOT_ALLOWED || !allowed.contains(method) {
            missing.push(format!("{method} {route}: {status}, Allow={allowed:?}"));
        }
    }
    assert!(
        missing.is_empty(),
        "{} of {} mounted operations missing:\n{}",
        missing.len(),
        expected.len(),
        missing.join("\n")
    );
}
