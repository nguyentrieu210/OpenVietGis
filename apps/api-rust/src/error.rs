//! Axum `IntoResponse` adapter preserving exact OpenVietGIS JSON error envelope parity.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use ovg_core::AppError;

/// HTTP-layer error wrapper carrying an optional request correlation ID.
#[derive(Debug)]
pub struct ApiError {
    pub inner: AppError,
    pub request_id: Option<String>,
}

impl ApiError {
    #[must_use]
    pub fn new(inner: AppError, request_id: Option<String>) -> Self {
        Self { inner, request_id }
    }
}

impl From<AppError> for ApiError {
    fn from(inner: AppError) -> Self {
        Self {
            inner,
            request_id: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let AppError::Internal {
            ref internal_detail,
        } = self.inner
        {
            tracing::error!(
                request_id = self.request_id.as_deref().unwrap_or(""),
                detail = %internal_detail,
                "internal API error redacted from client response"
            );
        }

        let status = StatusCode::from_u16(self.inner.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let envelope = self.inner.to_envelope(self.request_id);
        (status, Json(envelope)).into_response()
    }
}
