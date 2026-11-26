//! OpenVietGIS Rust modular monolith HTTP server library.

pub(crate) mod cad_bridge;
pub mod cartography;
pub(crate) mod catalog;
pub mod config;
pub mod connectors;
pub(crate) mod editing;
pub mod error;
pub(crate) mod extensions;
pub(crate) mod field_offline;
pub(crate) mod house_numbering;
pub(crate) mod identity;
pub(crate) mod map_render;
pub mod metrics;
pub mod middleware;
pub(crate) mod ops;
pub(crate) mod processing;
pub(crate) mod processing_models;
pub(crate) mod queries;
pub mod raster_policy;
pub mod routes;
pub(crate) mod spatial_replay;
pub mod state;
pub(crate) mod tenancy;
pub(crate) mod three_d;
pub(crate) mod transfers;

use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::middleware as axum_mw;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub use config::AppConfig;
pub use error::ApiError;
pub use state::AppState;

/// Initialize structured `tracing` logging (JSON or compact human-readable).
pub fn init_tracing(json_logs: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,openvietgis_api_rust=debug,tower_http=info"));

    let registry = tracing_subscriber::registry().with(filter);
    if json_logs {
        let _ = registry
            .with(tracing_subscriber::fmt::layer().json())
            .try_init();
    } else {
        let _ = registry
            .with(tracing_subscriber::fmt::layer().compact())
            .try_init();
    }
}

/// Construct the full Axum `Router` with CORS, tracing, request ID correlation, and error contracts.
pub fn build_router(state: AppState) -> Router {
    let allowed_origins: Vec<HeaderValue> = state
        .config
        .cors_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static("x-openvietgis-csrf"),
            HeaderName::from_static("x-request-id"),
        ]);

    Router::new()
        .nest("/api/v1", routes::api_v1_router())
        .fallback(routes::fallback_not_found)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::request_id_and_metrics,
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    async fn json_body(response: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn health_live_ready_metrics_and_error_envelope_contracts() {
        let state = AppState::new(AppConfig::default(), None).unwrap();
        let app = build_router(state);
        let expected_modules = usize::from(cfg!(feature = "house-numbering"));

        // 1. GET /api/v1/health/live -> 200 {"status": "ok"} + x-request-id propagation
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health/live")
                    .header("x-request-id", "test-req-42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("x-request-id").unwrap(), "test-req-42");
        assert_eq!(json_body(res).await["status"], "ok");

        // 2. GET /api/v1/health/ready without DB -> 503 {"status": "not_ready", "database": "unavailable"}
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_body(res).await;
        assert_eq!(body["status"], "not_ready");
        assert_eq!(body["database"], "unavailable");

        // 3. GET /api/v1/health -> 200 summary
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_body(res).await;
        assert_eq!(body["runtime"], "rust");
        assert_eq!(body["modules_registered"], expected_modules);

        // 4. GET /api/v1/ops/metrics -> Prometheus text format
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/ops/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.starts_with("text/plain; version=0.0.4"));
        let text = String::from_utf8(
            axum::body::to_bytes(res.into_body(), 1024 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("ovg_http_requests_total"));
        assert!(text.contains(&format!("ovg_registered_modules_total {expected_modules}")));

        // 5. GET /api/v1/modules -> house_numbering registered via SDK
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/modules")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_body(res).await;
        let modules = body["modules"].as_array().unwrap();
        assert_eq!(modules.len(), expected_modules);
        if cfg!(feature = "house-numbering") {
            assert_eq!(modules[0]["extension_key"], "house_numbering");
        }

        // 6. Unknown route -> 404 structured ErrorEnvelope with request_id
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/does-not-exist")
                    .header("x-request-id", "req-404")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = json_body(res).await;
        assert_eq!(body["code"], "NOT_FOUND");
        assert_eq!(body["request_id"], "req-404");
    }
}
