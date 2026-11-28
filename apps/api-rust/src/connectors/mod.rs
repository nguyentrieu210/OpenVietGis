//! E12 candidate policy primitives. No outbound I/O or authorization granted.
pub mod bindings;
pub mod catalog;
pub mod http;
pub mod profile;
pub mod provider_policy;
mod transfer;
pub(crate) mod transfer_worker;

pub(crate) fn router() -> axum::Router<crate::AppState> {
    http::router()
}
