//! Verify mounted method/path coverage without executing any business handler.
#![cfg(feature = "house-numbering")]

use std::collections::BTreeSet;
use std::path::PathBuf;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::Router;
use openvietgis_api_rust::{routes, AppConfig, AppState};
use serde_json::Value;
use tower::ServiceExt;

async fn mounted(_request: Request, _next: Next) -> StatusCode {
    StatusCode::IM_A_TEAPOT
}

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
    for engine in 1..=18 {
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
                expected.insert((method.to_ascii_uppercase(), route.to_owned()));
            }
        }
    }
    // The baseline WebSocket is also an HTTP GET upgrade route.
    expected.insert((
        "GET".to_owned(),
        "/api/v1/organizations/{organization_id}/projects/{project_id}/collaboration/presence"
            .to_owned(),
    ));
    expected
}

async fn probe(app: &Router, method: &str, path: &str) -> StatusCode {
    let method = if method == "WEBSOCKET" || method == "WS" {
        Method::GET
    } else {
        Method::from_bytes(method.as_bytes()).expect("valid HTTP method")
    };
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .expect("valid sample request"),
        )
        .await
        .expect("infallible router")
        .status()
}

#[tokio::test]
async fn frozen_api_inventory_is_mounted_on_full_profile() {
    let state = AppState::new(AppConfig::default(), None).expect("database-free state");
    // The route layer returns before extractors, authorization, or mutations.
    // It applies only to existing methods, not the unmatched-path fallback.
    let app = Router::new()
        .nest(
            "/api/v1",
            routes::api_v1_router().route_layer(middleware::from_fn(mounted)),
        )
        .with_state(state);
    assert_eq!(
        probe(&app, "GET", "/api/v1/health/live").await,
        StatusCode::IM_A_TEAPOT
    );
    assert_eq!(
        probe(&app, "POST", "/api/v1/health/live").await,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        probe(&app, "GET", "/api/v1/spec066-unknown-route").await,
        StatusCode::NOT_FOUND
    );

    let expected = baseline_operations();
    assert_eq!(
        expected.len(),
        364,
        "unexpected baseline inventory drift; reconcile the frozen contracts"
    );
    let mut missing = Vec::new();
    for (method, route) in &expected {
        let status = probe(&app, method, &sample_path(route)).await;
        if status != StatusCode::IM_A_TEAPOT {
            missing.push(format!("{method} {route}: {status}"));
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
