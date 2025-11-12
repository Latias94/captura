use axum::http::StatusCode;
use axum::response::IntoResponse;

// Miniflux 兼容错误：{"error_message": "..."}
pub struct MfError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

impl From<crate::error::ApiError> for MfError {
    fn from(e: crate::error::ApiError) -> Self {
        MfError {
            status: e.status,
            message: e.message,
        }
    }
}

impl IntoResponse for MfError {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::json!({"error_message": self.message});
        (self.status, axum::Json(body)).into_response()
    }
}

pub type MfResult<T> = Result<T, MfError>;
