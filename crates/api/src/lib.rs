pub mod auth;
pub mod auth_endpoints;
pub mod compat;
pub mod entries;
pub mod entry_options;
pub mod error;
pub mod feed_options;
pub mod feeds;
pub mod search;
pub mod state;
pub mod users;
pub mod util;

pub use state::AppState;

use axum::body::Body as AxumBody;
use axum::{
    routing::{get, post},
    Router,
};
use sea_orm::DatabaseConnection;
use serde::Serialize;

/// 构造带有最小路由的 Miniflux 兼容层 Router（用于测试）
pub fn miniflux_router_with_state(db: DatabaseConnection) -> axum::Router<AppState> {
    compat::miniflux::router().with_state(AppState::new(db))
}

/// 返回可直接用于 oneshot 的 Service（测试便利）
pub fn miniflux_service_with_state(
    db: DatabaseConnection,
) -> axum::routing::RouterIntoService<axum::body::Body, ()> {
    let st = AppState::new(db);
    compat::miniflux::router()
        .with_state::<()>(st)
        .into_service()
}

// 在 lib 目标下也提供通用 IdResp，避免其他模块通过 crate::IdResp 引用失败
#[derive(Serialize)]
pub struct IdResp {
    pub id: i64,
}

/// 提供最小可用的测试路由（健康检查 + 兼容层端点子集）
pub fn test_router(app_state: AppState) -> Router<AppState> {
    let compat_root = Router::new()
        .route(
            "/fever",
            get(crate::compat::fever::endpoint).post(crate::compat::fever::endpoint),
        )
        .route(
            "/reader/api/0/subscription/list",
            get(crate::compat::reader::subscription_list),
        )
        .route(
            "/reader/api/0/stream/contents/user/-/state/com.google/reading-list",
            get(crate::compat::reader::stream_contents),
        )
        .route(
            "/reader/api/0/edit-tag",
            post(crate::compat::reader::edit_tag),
        )
        .route(
            "/reader/api/0/mark-all-as-read",
            post(crate::compat::reader::mark_all_read),
        )
        .route(
            "/reader/api/0/unread-count",
            get(crate::compat::reader::unread_count),
        )
        .route(
            "/reader/api/0/subscription/quickadd",
            post(crate::compat::reader::subscription_quickadd),
        )
        .route(
            "/reader/api/0/subscription/edit",
            post(crate::compat::reader::subscription_edit),
        )
        .route(
            "/reader/api/0/stream/items/ids",
            get(crate::compat::reader::items_ids),
        )
        .route(
            "/reader/api/0/stream/items/contents",
            get(crate::compat::reader::items_contents),
        );

    async fn liveness() -> &'static str {
        "OK"
    }
    let v1 = Router::new()
        .route("/users", post(crate::users::create_user))
        .route("/auth/login", post(crate::auth_endpoints::auth_login))
        .route(
            "/feeds",
            post(crate::feeds::create_feed).get(crate::feeds::list_feeds),
        )
        .route("/entries", get(crate::entries::list_entries))
        .route("/users/{id}/fever-key", post(crate::users::set_fever_key));

    Router::new()
        .route("/healthz", get(liveness))
        .merge(compat_root)
        .nest("/api/v1", v1)
        .with_state(app_state)
}

/// 仅用于 HTTP 烟囱测试的最小 Router（无状态）
pub fn test_min_router() -> Router {
    async fn liveness() -> &'static str {
        "OK"
    }
    Router::new().route("/healthz", get(liveness))
}

/// 返回可直接 oneshot 的 Service（将状态注入路由后擦除为 `()`）
pub fn test_router_service(app_state: AppState) -> axum::routing::RouterIntoService<AxumBody, ()> {
    let st = app_state.clone();
    test_router(app_state).with_state::<()>(st).into_service()
}
