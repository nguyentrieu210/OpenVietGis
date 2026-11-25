//! Typed application errors mapping 1-to-1 to OpenVietGIS HTTP error contracts.
//!
//! Customer-safe error messages are preserved across the API boundary; internal
//! SQL or infrastructure error messages must never be forwarded to clients.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Canonical HTTP status codes produced by the OpenVietGIS API.
pub type HttpStatusCode = u16;

/// JSON error envelope returned by OpenVietGIS HTTP endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorEnvelope {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            request_id: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

/// Unified domain & platform error enum mirroring Python `errors.py` and `platform_errors.py`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{message}")]
    BadRequest {
        code: &'static str,
        message: String,
        details: Option<Value>,
    },

    #[error("{message}")]
    Unauthorized { code: &'static str, message: String },

    #[error("{message}")]
    Forbidden { code: &'static str, message: String },

    #[error("{message}")]
    NotFound { code: &'static str, message: String },

    #[error("{message}")]
    Conflict {
        code: &'static str,
        message: String,
        details: Option<Value>,
    },

    #[error("{message}")]
    Validation {
        code: &'static str,
        message: String,
        details: Option<Value>,
    },

    #[error("{message}")]
    RateLimited {
        code: &'static str,
        message: String,
        retry_after_secs: Option<u64>,
    },

    #[error("{message}")]
    ServiceUnavailable { code: &'static str, message: String },

    /// Internal error; `internal_detail` is logged server-side and NEVER exposed to HTTP clients.
    #[error("internal server error")]
    Internal { internal_detail: String },
}

impl AppError {
    #[must_use]
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            code: "BAD_REQUEST",
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized {
            code: "UNAUTHORIZED",
            message: message.into(),
        }
    }

    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden {
            code: "FORBIDDEN",
            message: message.into(),
        }
    }

    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            code: "NOT_FOUND",
            message: message.into(),
        }
    }

    #[must_use]
    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn stale_layer_revision() -> Self {
        Self::conflict(
            "STALE_LAYER_REVISION",
            "Layer revision changed; read current metadata and retry.",
        )
    }

    #[must_use]
    pub fn stale_map_revision() -> Self {
        Self::conflict(
            "STALE_MAP_REVISION",
            "Map revision changed; reload the committed configuration.",
        )
    }

    #[must_use]
    pub fn data_locked(message: impl Into<String>) -> Self {
        Self::conflict("DATA_LOCKED", message)
    }

    #[must_use]
    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            code,
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn unsupported_crs(code: u32) -> Self {
        Self::Validation {
            code: "UNSUPPORTED_CRS",
            message: "Select a readable supported CRS.".to_string(),
            details: Some(serde_json::json!({ "epsg_code": code })),
        }
    }

    #[must_use]
    pub fn unsupported_transformation(source_epsg: u32, target_epsg: u32) -> Self {
        Self::Validation {
            code: "UNSUPPORTED_TRANSFORMATION",
            message: "No approved direct operation exists.".to_string(),
            details: Some(serde_json::json!({
                "source_epsg": source_epsg,
                "target_epsg": target_epsg,
            })),
        }
    }

    #[must_use]
    pub fn transformation_out_of_area(message: impl Into<String>) -> Self {
        Self::Validation {
            code: "TRANSFORMATION_OUT_OF_AREA",
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn invalid_cursor() -> Self {
        Self::validation(
            "INVALID_CURSOR",
            "Cursor is not part of the current source.",
        )
    }

    #[must_use]
    pub fn rate_limited(retry_after_secs: Option<u64>) -> Self {
        Self::RateLimited {
            code: "RATE_LIMITED",
            message: "Too many requests; retry later.".to_string(),
            retry_after_secs,
        }
    }

    #[must_use]
    pub fn service_unavailable(code: &'static str, message: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::Internal {
            internal_detail: detail.into(),
        }
    }

    /// HTTP status code matching OpenVietGIS API contracts.
    #[must_use]
    pub const fn status_code(&self) -> HttpStatusCode {
        match self {
            Self::BadRequest { .. } => 400,
            Self::Unauthorized { .. } => 401,
            Self::Forbidden { .. } => 403,
            Self::NotFound { .. } => 404,
            Self::Conflict { .. } => 409,
            Self::Validation { .. } => 422,
            Self::RateLimited { .. } => 429,
            Self::ServiceUnavailable { .. } => 503,
            Self::Internal { .. } => 500,
        }
    }

    /// Stable machine-readable error code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::BadRequest { code, .. }
            | Self::Unauthorized { code, .. }
            | Self::Forbidden { code, .. }
            | Self::NotFound { code, .. }
            | Self::Conflict { code, .. }
            | Self::Validation { code, .. }
            | Self::RateLimited { code, .. }
            | Self::ServiceUnavailable { code, .. } => code,
            Self::Internal { .. } => "INTERNAL_ERROR",
        }
    }

    /// Convert to a customer-safe JSON error envelope.
    #[must_use]
    pub fn to_envelope(&self, request_id: Option<String>) -> ErrorEnvelope {
        let mut envelope = match self {
            Self::BadRequest {
                code,
                message,
                details,
            }
            | Self::Conflict {
                code,
                message,
                details,
            }
            | Self::Validation {
                code,
                message,
                details,
            } => {
                let mut env = ErrorEnvelope::new(*code, message.clone());
                if let Some(d) = details {
                    env = env.with_details(d.clone());
                }
                env
            }
            Self::Unauthorized { code, message }
            | Self::Forbidden { code, message }
            | Self::NotFound { code, message }
            | Self::ServiceUnavailable { code, message } => {
                ErrorEnvelope::new(*code, message.clone())
            }
            Self::RateLimited {
                code,
                message,
                retry_after_secs,
            } => {
                let mut env = ErrorEnvelope::new(*code, message.clone());
                if let Some(secs) = retry_after_secs {
                    env = env.with_details(serde_json::json!({ "retry_after_secs": secs }));
                }
                env
            }
            Self::Internal { .. } => {
                ErrorEnvelope::new("INTERNAL_ERROR", "An unexpected internal error occurred.")
            }
        };
        if let Some(req_id) = request_id {
            envelope = envelope.with_request_id(req_id);
        }
        envelope
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_standard_status_codes() {
        assert_eq!(AppError::bad_request("bad").status_code(), 400);
        assert_eq!(AppError::unauthorized("auth").status_code(), 401);
        assert_eq!(AppError::forbidden("denied").status_code(), 403);
        assert_eq!(AppError::not_found("missing").status_code(), 404);
        assert_eq!(AppError::stale_layer_revision().status_code(), 409);
        assert_eq!(AppError::unsupported_crs(99999).status_code(), 422);
        assert_eq!(AppError::rate_limited(Some(10)).status_code(), 429);
        assert_eq!(AppError::internal("sql leak").status_code(), 500);
        assert_eq!(
            AppError::service_unavailable("DB_UNAVAILABLE", "down").status_code(),
            503
        );
    }

    #[test]
    fn internal_error_never_leaks_internal_detail() {
        let err = AppError::internal("password=supersecret sqlstate=42P01");
        let env = err.to_envelope(Some("req-123".to_string()));
        assert_eq!(env.code, "INTERNAL_ERROR");
        assert!(!env.message.contains("supersecret"));
        assert!(!env.message.contains("42P01"));
        assert_eq!(env.request_id.as_deref(), Some("req-123"));
        assert!(env.details.is_none());
    }
}
