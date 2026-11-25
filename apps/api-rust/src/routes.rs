//! Core health, ops metrics, CRS registry, module discovery, and fallback routes.

use axum::extract::{Extension, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::{all_crs_profiles, AppError, CRS_REGISTRY_VERSION};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::middleware::RequestId;
use crate::state::AppState;

pub fn api_v1_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_summary))
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/ops/health/live", get(ops_health_live))
        .route("/ops/health/ready", get(crate::ops::readiness))
        .route("/ops/metrics", get(ops_metrics))
        .route("/crs", get(list_crs))
        .route("/modules", get(list_modules))
        .merge(crate::identity::router())
        .merge(crate::catalog::router())
        .merge(crate::queries::router())
}

async fn health_summary(State(state): State<AppState>) -> Json<Value> {
    let db_status = match &state.db_pool {
        Some(pool) => match pool.health_check().await {
            Ok(h) if h.database_ready => {
                state.metrics.set_db_ready(true);
                json!({
                    "status": "ready",
                    "postgis_version": h.postgis_version,
                })
            }
            _ => {
                state.metrics.set_db_ready(false);
                json!({ "status": "unavailable" })
            }
        },
        None => json!({ "status": "unconfigured" }),
    };

    Json(json!({
        "status": "ok",
        "service": "openvietgis-api-rust",
        "runtime": "rust",
        "version": state.config.service_version,
        "database": db_status,
        "modules_registered": state.modules.len(),
    }))
}

/// Exact parity with Python `GET /api/v1/health/live`.
async fn health_live() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Exact parity with Python `GET /api/v1/health/ready`.
async fn health_ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let Some(pool) = &state.db_pool else {
        state.metrics.set_db_ready(false);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "database": "unavailable",
            })),
        );
    };

    match pool.health_check().await {
        Ok(health) if health.database_ready => {
            state.metrics.set_db_ready(true);
            (
                StatusCode::OK,
                Json(json!({
                    "status": "ready",
                    "database": "ready",
                })),
            )
        }
        _ => {
            state.metrics.set_db_ready(false);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "not_ready",
                    "database": "unavailable",
                })),
            )
        }
    }
}

/// Exact parity with Python `GET /api/v1/ops/health/live`.
async fn ops_health_live() -> Json<Value> {
    Json(json!({
        "status": "alive",
        "service": "openvietgis-api",
    }))
}

/// Prometheus text exposition endpoint matching Python `GET /api/v1/ops/metrics`.
async fn ops_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state
        .metrics
        .render_prometheus(state.modules.len(), state.jobs.in_flight_count());
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

async fn list_crs() -> Json<Value> {
    Json(json!({
        "registry_version": CRS_REGISTRY_VERSION,
        "profiles": all_crs_profiles(),
    }))
}

async fn list_modules(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "modules": state.modules.list_manifests(),
    }))
}

pub async fn fallback_not_found(req_id: Option<Extension<RequestId>>) -> ApiError {
    ApiError::new(
        AppError::not_found("route not found"),
        req_id.map(|Extension(r)| r.0),
    )
}
