use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            code: self.code.to_string(),
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

pub fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        e.to_string(),
    )
}
pub fn not_found(msg: &str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "not_found", msg)
}
pub fn bad_request<S: Into<String>>(msg: S) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "bad_request", msg)
}
pub fn unauthorized(msg: &str) -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", msg)
}
pub fn forbidden(msg: &str) -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "forbidden", msg)
}
pub fn too_many_requests<S: Into<String>>(msg: S) -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "too_many_requests",
        msg.into(),
    )
}
