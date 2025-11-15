use axum::http::StatusCode;
use axum::response::{ErrorResponse, Result as AxumResult};
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;

/// Miniflux-compatible error response body: `{"error_message": "..."}`
#[derive(Serialize)]
pub struct MfErrorBody {
    pub error_message: String,
}

/// Result type used by the Miniflux compatibility layer.
/// Error type is `axum::response::ErrorResponse`, which plays well with axum.
pub type MfResult<T> = AxumResult<T>;

/// Map an internal `ApiError` into a Miniflux-style `ErrorResponse`.
pub fn from_api_error(e: ApiError) -> ErrorResponse {
    mf_error(e.status, e.message)
}

/// Build a Miniflux-style error: status code + `{"error_message": ...}` JSON.
pub fn mf_error(status: StatusCode, message: impl Into<String>) -> ErrorResponse {
    let body = Json(MfErrorBody {
        error_message: message.into(),
    });
    ErrorResponse::from((status, body))
}

/// Internal error (500).
pub fn internal<E: std::fmt::Display>(e: E) -> ErrorResponse {
    mf_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Bad request (400).
pub fn bad_request(msg: &str) -> ErrorResponse {
    mf_error(StatusCode::BAD_REQUEST, msg.to_string())
}

/// Not found (404).
pub fn not_found(resource: &str) -> ErrorResponse {
    mf_error(StatusCode::NOT_FOUND, resource.to_string())
}

/// Forbidden (403).
pub fn forbidden(msg: &str) -> ErrorResponse {
    mf_error(StatusCode::FORBIDDEN, msg.to_string())
}
