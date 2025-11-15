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

/// Fever single-endpoint adapter: `GET/POST /fever`
pub(crate) async fn endpoint(State(st): State<AppState>, Query(q): Query<FeverQuery>) -> Response {
    handlers::endpoint(&st, &q).await
}

/// Build the Fever compatibility-layer Router.
pub fn router() -> Router<AppState> {
    Router::new().route("/fever", get(endpoint).post(endpoint))
}
