//! Request ID correlation and HTTP metrics middleware.

use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;
use uuid::Uuid;

use crate::state::AppState;

pub static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Request extension holding the active correlation request ID.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

pub async fn request_id_and_metrics(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let req_id = req
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(RequestId(req_id.clone()));
    let started = Instant::now();
    let mut response = next.run(req).await;
    state
        .metrics
        .record_request(response.status().as_u16(), started.elapsed());

    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) && !response.headers().contains_key(header::CACHE_CONTROL)
    {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
    }

    if let Ok(val) = req_id.parse() {
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER.clone(), val);
    }
    response
}
