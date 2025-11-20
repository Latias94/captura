pub mod auth;
pub mod auth_endpoints;
pub mod categories;
pub mod compat;
pub mod entries;
pub mod entry_options;
pub mod error;
pub mod favicon;
pub mod feed_options;
pub mod feeds;
pub mod hub;
pub mod integrations;
pub mod jobs;
pub mod labels;
pub mod media;
pub mod oidc;
pub mod opml;
pub mod records;
pub mod rules;
pub mod smart_views;
pub mod state;
pub mod users;
pub mod util;
pub mod views;
pub mod webhooks;

pub use captura_types::IdResp;
pub use state::{AppConfig, AppState};

use axum::body::Body as AxumBody;
use axum::{
    extract::State,
    routing::{get, post, put},
    Router,
};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

/// Type alias used by integration tests to avoid tying them to the exact
/// Router generic parameters.
pub type RouterServiceType = axum::routing::RouterIntoService<AxumBody, ()>;

/// Build the full application Router, including:
/// - `/api/v1` primary API (auth/feeds/entries/jobs/...)
/// - `/v1` Miniflux compatibility layer
/// - `/fever` / Reader compatibility layer
/// - WebUI SSR routes
pub fn build_router(app_state: AppState) -> Router {
    let api_v1 = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        // users & auth
        .route("/users", post(crate::users::create_user))
        .route("/users/{id}/fever-key", post(crate::users::set_fever_key))
        .route("/me", get(crate::users::me))
        .route("/me/prefs", put(crate::users::update_prefs))
        .route("/auth/login", post(crate::auth_endpoints::auth_login))
        .route(
            "/auth/proxy/token",
            get(crate::auth_endpoints::auth_proxy_token),
        )
        .route("/auth/oidc/start", get(crate::oidc::start))
        .route("/auth/oidc/callback", get(crate::oidc::callback))
        .route("/auth/oidc/providers", get(crate::oidc::oidc_providers))
        .route("/auth/oidc/{name}/start", get(crate::oidc::start_named))
        .route(
            "/auth/oidc/{name}/callback",
            get(crate::oidc::callback_named),
        )
        // feeds & entries
        .route(
            "/feeds",
            post(crate::feeds::create_feed).get(crate::feeds::list_feeds),
        )
        .route(
            "/feeds/{id}",
            get(crate::feeds::get_feed)
                .patch(crate::feeds::update_feed)
                .delete(crate::feeds::delete_feed),
        )
        .route("/feeds/{id}/rss", get(crate::feeds::rss_feed))
        .route("/feeds/{id}/refresh", post(crate::feeds::refresh_feed))
        .route(
            "/feeds/{id}/enqueue-refresh",
            post(crate::feeds::enqueue_feed_refresh),
        )
        .route("/feeds/bulk-view", post(crate::feeds::bulk_update_view))
        .route("/feeds/{id}/favicon/refresh", post(crate::favicon::refresh))
        .route("/favicons/{id}", get(crate::favicon::get))
        .route(
            "/categories",
            get(crate::categories::list_categories).post(crate::categories::create_category),
        )
        .route(
            "/categories/{id}",
            get(crate::categories::get_category)
                .put(crate::categories::update_category)
                .delete(crate::categories::delete_category),
        )
        .route("/feeds/counters", get(crate::feeds::feeds_counters))
        .route(
            "/categories/counters",
            get(crate::categories::category_counters),
        )
        .route(
            "/labels",
            get(crate::labels::list_labels).post(crate::labels::create_label),
        )
        .route(
            "/labels/{id}",
            put(crate::labels::update_label).delete(crate::labels::delete_label),
        )
        .route("/entries", get(crate::entries::list_entries))
        .route("/entries/bulk-status", post(crate::entries::bulk_status))
        .route(
            "/entries/mark-all-read",
            post(crate::entries::mark_all_read),
        )
        .route("/entries/{id}", get(crate::entries::get_entry))
        .route("/entries/{id}/content", get(crate::entries::entry_content))
        .route("/entries/{id}/read", post(crate::entries::mark_read))
        .route("/entries/{id}/star", post(crate::entries::mark_star))
        .route("/entries/{id}/save", post(crate::entries::save_entry))
        .route(
            "/entries/{id}/tags",
            post(crate::entries::add_tags).delete(crate::entries::remove_tags),
        )
        .route(
            "/smart-views",
            get(crate::smart_views::list_smart_views).post(crate::smart_views::create_smart_view),
        )
        .route(
            "/smart-views/{id}",
            get(crate::smart_views::get_smart_view)
                .put(crate::smart_views::update_smart_view)
                .delete(crate::smart_views::delete_smart_view),
        )
        .route(
            "/smart-views/{id}/entries",
            get(crate::smart_views::list_smart_view_entries),
        )
        .route("/views", get(crate::views::list_views))
        .route("/views/summary", get(crate::views::view_summary))
        .route("/timelines", get(crate::views::list_timelines))
        .route("/opml/export", get(crate::opml::export))
        .route("/opml/import", post(crate::opml::import))
        .route("/opml/validate", post(crate::opml::validate))
        .route("/export/full", get(crate::opml::export_full))
        .route("/import/full", post(crate::opml::import_full))
        // jobs
        .route("/jobs", get(crate::jobs::list_jobs))
        .route("/jobs/run-once", post(crate::jobs::run_jobs_once))
        .route(
            "/jobs/enqueue-due-feeds",
            post(crate::jobs::enqueue_due_feeds),
        )
        // media proxy
        .route("/media", get(crate::media::proxy))
        // webhooks
        .route(
            "/webhooks",
            get(crate::webhooks::list).post(crate::webhooks::create),
        )
        .route(
            "/webhooks/{id}",
            get(crate::webhooks::get).delete(crate::webhooks::delete),
        )
        // integrations
        .route(
            "/integrations",
            get(crate::integrations::list).post(crate::integrations::create),
        )
        .route(
            "/integrations/{id}",
            get(crate::integrations::get)
                .put(crate::integrations::update)
                .delete(crate::integrations::delete),
        )
        .route(
            "/integrations/jobs",
            get(crate::jobs::list_integration_jobs),
        )
        // rule & hub execution records / stats
        .route("/rules/stats", get(crate::records::list_rule_stats))
        .route(
            "/hub/routes/stats",
            get(crate::records::list_hub_route_stats),
        )
        // rules
        .route(
            "/rules",
            get(crate::rules::list_rules).post(crate::rules::create_rule),
        )
        .route("/rules/lint", post(crate::rules::lint_rule))
        .route(
            "/rules/{id}",
            get(crate::rules::get_rule)
                .put(crate::rules::update_rule)
                .delete(crate::rules::delete_rule),
        )
        .route("/rules/try", post(crate::rules::try_rule))
        .route("/rules/templates", get(crate::rules::list_templates))
        .route("/rules/templates/{id}", get(crate::rules::get_template))
        .route(
            "/feeds/from-template",
            post(crate::rules::create_feed_from_template),
        )
        .route("/feeds/validate-hub", post(crate::hub::validate_hub))
        .route("/hub/routes", get(crate::hub::list_routes))
        .route("/hub/routes/{namespace}/{name}", get(crate::hub::get_route))
        .route("/hub/preview", post(crate::hub::preview_hub));

    let compat_root = compat::fever::router().merge(compat::reader::router());

    let mut app = Router::new()
        .route("/healthz", get(|| async { "OK" }))
        .route("/liveness", get(|| async { "OK" }))
        .route(
            "/healthcheck",
            get(|State(st): State<AppState>| async move {
                let _ = st.db.execute_unprepared("SELECT 1").await;
                "OK"
            }),
        )
        .route(
            "/readyz",
            get(|State(st): State<AppState>| async move {
                let _ = st.db.execute_unprepared("SELECT 1").await;
                "OK"
            }),
        )
        .route(
            "/readiness",
            get(|State(st): State<AppState>| async move {
                let _ = st.db.execute_unprepared("SELECT 1").await;
                "OK"
            }),
        )
        .merge(compat_root)
        // Web UI (SSR): mounted at root and /ui/static/*
        .merge(captura_webui::router())
        .nest("/api/v1", api_v1)
        .nest("/v1", crate::compat::miniflux::router())
        .with_state(app_state.clone());

    if app_state.cfg.security_headers_enabled {
        let rp = axum::http::HeaderValue::from_str(&app_state.cfg.referrer_policy)
            .unwrap_or(axum::http::HeaderValue::from_static("no-referrer"));
        app = app.layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("referrer-policy"),
            rp,
        ));
        app = app.layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-content-type-options"),
            axum::http::HeaderValue::from_static("nosniff"),
        ));
        app = app.layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-frame-options"),
            axum::http::HeaderValue::from_static("DENY"),
        ));
        if let Some(csp) = &app_state.cfg.content_security_policy {
            if !csp.is_empty() {
                let v = axum::http::HeaderValue::from_str(csp).unwrap_or_else(|_| {
                    axum::http::HeaderValue::from_static(
                        "default-src 'self'; frame-ancestors 'none';",
                    )
                });
                app = app.layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::HeaderName::from_static("content-security-policy"),
                    v,
                ));
            } else {
                let v = axum::http::HeaderValue::from_static(
                    "default-src 'self'; frame-ancestors 'none';",
                );
                app = app.layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::HeaderName::from_static("content-security-policy"),
                    v,
                ));
            }
        } else {
            let v =
                axum::http::HeaderValue::from_static("default-src 'self'; frame-ancestors 'none';");
            app = app.layer(SetResponseHeaderLayer::overriding(
                axum::http::header::HeaderName::from_static("content-security-policy"),
                v,
            ));
        }
    }

    // Attach an HTTP trace layer for structured request/response logging.
    app.layer(TraceLayer::new_for_http())
}

/// Build a Miniflux compatibility router with minimal routes (for tests)
pub fn miniflux_router_with_state(db: DatabaseConnection) -> axum::Router<AppState> {
    compat::miniflux::router().with_state(AppState::new(db))
}

/// Return a oneshot-ready Service for Miniflux compatibility tests
pub fn miniflux_service_with_state(
    db: DatabaseConnection,
) -> axum::routing::RouterIntoService<axum::body::Body, ()> {
    let st = AppState::new(db);
    compat::miniflux::router()
        .with_state::<()>(st)
        .into_service()
}

/// Provide a minimal test router (healthcheck + subset of compatibility endpoints)
pub fn test_router(app_state: AppState) -> Router<AppState> {
    let compat_root = compat::fever::router().merge(compat::reader::router());

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
        .route("/opml/validate", post(crate::opml::validate))
        .route("/users/{id}/fever-key", post(crate::users::set_fever_key));

    Router::new()
        .route("/healthz", get(liveness))
        .merge(compat_root)
        .nest("/api/v1", v1)
        .with_state(app_state)
}

/// Minimal router used only for HTTP smoke tests (stateless)
pub fn test_min_router() -> Router {
    async fn liveness() -> &'static str {
        "OK"
    }
    Router::new().route("/healthz", get(liveness))
}

/// Return a oneshot-ready Service, injecting state and erasing it to `()`
pub fn test_router_service(app_state: AppState) -> axum::routing::RouterIntoService<AxumBody, ()> {
    let st = app_state.clone();
    test_router(app_state).with_state::<()>(st).into_service()
}
