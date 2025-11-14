use axum::http::StatusCode;
use axum::response::{ErrorResponse, Result as AxumResult};
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;

/// Miniflux 兼容错误响应体：`{"error_message": "..."}`
#[derive(Serialize)]
pub struct MfErrorBody {
    pub error_message: String,
}

/// Miniflux 兼容层统一使用的 Result 类型。
/// 错误类型为 `axum::response::ErrorResponse`，便于与 axum 生态兼容。
pub type MfResult<T> = AxumResult<T>;

/// 将应用内部的 `ApiError` 映射为 Miniflux 风格的 `ErrorResponse`。
pub fn from_api_error(e: ApiError) -> ErrorResponse {
    mf_error(e.status, e.message)
}

/// 构造 Miniflux 风格错误：状态码 + `{"error_message": ...}` JSON。
pub fn mf_error(status: StatusCode, message: impl Into<String>) -> ErrorResponse {
    let body = Json(MfErrorBody {
        error_message: message.into(),
    });
    ErrorResponse::from((status, body))
}

/// 内部错误（500）。
pub fn internal<E: std::fmt::Display>(e: E) -> ErrorResponse {
    mf_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Bad request（400）。
pub fn bad_request(msg: &str) -> ErrorResponse {
    mf_error(StatusCode::BAD_REQUEST, msg.to_string())
}

/// Not found（404）。
pub fn not_found(resource: &str) -> ErrorResponse {
    mf_error(StatusCode::NOT_FOUND, resource.to_string())
}

/// Forbidden（403）。
pub fn forbidden(msg: &str) -> ErrorResponse {
    mf_error(StatusCode::FORBIDDEN, msg.to_string())
}
