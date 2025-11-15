use axum::{
    extract::{Query, State},
    response::Response,
    routing::{get, post},
    Router,
};

use crate::AppState;

mod handlers;
mod types;

pub(crate) use types::FeverQuery;

/// Fever 单端点适配器：`GET/POST /fever`
pub(crate) async fn endpoint(State(st): State<AppState>, Query(q): Query<FeverQuery>) -> Response {
    handlers::endpoint(&st, &q).await
}

/// 构建 Fever 兼容层 Router。
pub fn router() -> Router<AppState> {
    Router::new().route("/fever", get(endpoint).post(endpoint))
}
