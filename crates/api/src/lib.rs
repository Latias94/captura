pub mod auth;
pub mod compat;
pub mod error;
pub mod search;
pub mod state;

pub use state::AppState;

use sea_orm::DatabaseConnection;

/// 构造带有最小路由的 Miniflux 兼容层 Router（用于测试）
pub fn miniflux_router_with_state(db: DatabaseConnection) -> axum::Router<AppState> {
    compat::miniflux::router().with_state(AppState::new(db))
}

/// 返回可直接用于 oneshot 的 Service（测试便利）
pub fn miniflux_service_with_state(
    db: DatabaseConnection,
) -> axum::routing::RouterIntoService<axum::body::Body, AppState> {
    compat::miniflux::router()
        .with_state(AppState::new(db))
        .into_service()
}
