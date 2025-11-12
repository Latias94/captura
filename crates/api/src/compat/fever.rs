use axum::{
    extract::{Query, State},
    response::Response,
};

use crate::AppState;

mod handlers;
mod types;

pub(crate) use types::FeverQuery;

// 路由由 main.rs 直接挂载到 endpoint，这里仅做薄转调
pub(crate) async fn endpoint(State(st): State<AppState>, Query(q): Query<FeverQuery>) -> Response {
    handlers::endpoint(&st, &q).await
}
