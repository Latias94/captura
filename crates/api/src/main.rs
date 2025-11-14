//! Captura API service entrypoint (Axum-based).

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
// use axum::debug_handler;
use axum_extra::typed_header::TypedHeader;
// use captura_crawler::{self as crawler, CrawlOptions};
// pipeline used elsewhere (try_rule path uses parsing/execution), direct refresh handled by service
// use captura_service as service;
// rules parsing handled in rules module
use captura_scheduler as scheduler;
use captura_storage::connect as db_connect;
use captura_storage::entity::job;
use captura_storage::entity::prelude::Job;
use headers::authorization::Bearer;
use headers::Authorization;
// md5 compatibility removed
use migration::migrate;
// once_cell only used in old tests
// use scraper::{Html, Selector};
use sea_orm::ConnectionTrait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
// use sea_orm::sea_query::{Alias as SAlias, Expr as SExpr, Func as SFunc};
// use sea_orm::sea_query::OnConflict; // unused after service extraction
use serde::{Deserialize, Serialize};
// sha2 only used in old tests
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;
// use url::Url; // no longer used in main
// use axum::Form; // reader handlers moved to compat
mod error;
use crate::error::{internal, ApiResult};
// use axum::middleware;
// use axum::middleware::Next;
mod auth;
mod entry_options;
mod feed_options;
mod util;
use crate::auth::AuthUser;
use crate::util::validate_limit_offset;
mod categories;
mod entries;
mod feeds;
mod rules;
// compat no longer needed; remove legacy compatible routes
mod compat;
mod jobs;
mod state;
mod users;
pub use state::{AppConfig, AppState};
mod auth_endpoints;
use crate::auth_endpoints::{auth_login, auth_proxy_token};
mod favicon;
mod hub;
mod integrations;
mod media;
mod oidc;
mod opml;
mod search;
mod webhooks;
// testkit 已抽离为独立 crate: captura-testkit

// Re-export types for tests no longer needed; keep API modules self-contained
// OPML types for Miniflux wrappers (not used in main)
// use crate::opml::OutlineNode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_max_level(Level::INFO)
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://captura.db?mode=rwc".to_string());
    let db = db_connect(&db_url).await?;
    migrate(&db).await?;
    let app_state = AppState::new(db.clone());

    // Background scheduler (optional)
    if std::env::var("SCHEDULER_ENABLED")
        .ok()
        .map(|v| v.to_lowercase() != "false" && v != "0")
        .unwrap_or(true)
    {
        let db_enq = db.clone();
        let db_run = db.clone();
        let enqueue_every: u64 = std::env::var("SCHEDULER_ENQUEUE_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        let run_every: u64 = std::env::var("SCHEDULER_RUNONCE_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        let enqueue_batch: u64 = std::env::var("SCHEDULER_ENQUEUE_BATCH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);
        let run_batch: u64 = std::env::var("SCHEDULER_RUNONCE_BATCH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(enqueue_every)).await;
                match scheduler::enqueue_due_feeds(&db_enq, enqueue_batch).await {
                    Ok(n) => tracing::info!(enqueued = n, "scheduler: enqueue due feeds"),
                    Err(e) => tracing::warn!(error=%e, "scheduler: enqueue due feeds failed"),
                }
            }
        });
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(run_every)).await;
                match scheduler::run_once(&db_run, run_batch).await {
                    Ok(n) => tracing::info!(processed = n, "scheduler: run once"),
                    Err(e) => tracing::warn!(error=%e, "scheduler: run once failed"),
                }
            }
        });
    }

    // 路由组装与服务启动见下

    // AppState 已迁移至 state.rs，并在此处通过 `pub use` 进行再导出

    // moved to compat::miniflux::router

    // Miniflux/兼容层鉴权函数移至 auth.rs

    // Miniflux main implementation moved to compat::miniflux

    // v1 feed create/list handlers moved to feeds.rs

    // Error helpers moved to error.rs

    // create_feed moved

    // tests moved; no longer re-export handlers from main

    // AuthUser moved to auth.rs

    // v1 entries handlers moved to entries.rs

    // ----- Extended feeds & categories -----

    // legacy FeedsQuery removed (moved to crates/api/src/feeds.rs)

    // legacy FeedDto removed (moved to crates/api/src/feeds.rs)

    // legacy list_feeds removed (moved to crates/api/src/feeds.rs)

    // legacy get_feed removed (moved to crates/api/src/feeds.rs)

    // legacy UpdateFeedReq removed (moved to crates/api/src/feeds.rs)

    // legacy update_feed removed (moved to crates/api/src/feeds.rs)

    // legacy delete_feed removed (moved to crates/api/src/feeds.rs)

    // legacy ExtendedEntriesQuery removed (moved to crates/api/src/entries.rs)

    // legacy _list_entries_extended removed (moved to crates/api/src/entries.rs)

    // legacy mark_all_read removed (moved to crates/api/src/entries.rs)

    // legacy categories handlers removed (moved to crates/api/src/categories.rs)

    // OPML export/import
    // legacy opml_export removed; use crate::opml::export

    // opml_import 已迁至 crates/api/src/opml.rs

    // OPML helpers moved to crate::opml

    // ...

    // parse_opml_quickxml moved to crate::opml

    // Fever 兼容实现已迁移至 crate::compat::fever

    // Fever 兼容相关已移除

    // Favicon 主实现已迁移至 crate::favicon

    // set_fever_key moved to users.rs (re-exported above)

    // Build API routers
    let api_v1 = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        // users & auth
        .route("/users", post(crate::users::create_user))
        .route("/users/{id}/fever-key", post(crate::users::set_fever_key))
        .route("/auth/login", post(auth_login))
        .route("/auth/proxy/token", get(auth_proxy_token))
        .route("/auth/oidc/start", get(crate::oidc::start))
        .route("/auth/oidc/callback", get(crate::oidc::callback))
        .route("/auth/oidc/providers", get(oidc_providers))
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
        .route("/entries", get(crate::entries::list_entries))
        .route(
            "/entries/mark-all-read",
            post(crate::entries::mark_all_read),
        )
        .route("/entries/{id}/read", post(crate::entries::mark_read))
        .route("/entries/{id}/star", post(crate::entries::mark_star))
        .route("/opml/export", get(crate::opml::export))
        .route("/opml/import", post(crate::opml::import))
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
        // rules
        .route(
            "/rules",
            get(crate::rules::list_rules).post(crate::rules::create_rule),
        )
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
            "/rules/sync-from-fs",
            post(crate::rules::sync_rules_from_fs),
        )
        .route(
            "/feeds/from-template",
            post(crate::rules::create_feed_from_template),
        )
        .route("/feeds/validate-hub", post(crate::hub::validate_hub))
        .route("/hub/routes", get(crate::hub::list_routes))
        .route("/hub/routes/{namespace}/{name}", get(crate::hub::get_route))
        .route("/hub/preview", post(crate::hub::preview_hub));

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
    async fn readiness(State(st): State<AppState>) -> &'static str {
        let _ = st.db.execute_unprepared("SELECT 1").await;
        "OK"
    }
    // 保持与现有启动一致的装配（tests 将使用 router::build）
    let mut app = Router::new()
        .route("/healthz", get(liveness))
        .route("/liveness", get(liveness))
        .route("/healthcheck", get(readiness))
        .route("/readyz", get(readiness))
        .route("/readiness", get(readiness))
        .merge(compat_root)
        // Web UI (SSR): mounted at root and /ui/static/*
        .merge(captura_webui::router())
        .nest("/api/v1", api_v1)
        .nest("/v1", crate::compat::miniflux::router())
        .with_state(app_state.clone());

    if app_state.cfg.security_headers_enabled {
        use tower_http::set_header::SetResponseHeaderLayer;
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
        if let Some(ref csp) = app_state.cfg.content_security_policy {
            if !csp.is_empty() {
                let v = axum::http::HeaderValue::from_str(csp)
                    .unwrap_or(axum::http::HeaderValue::from_static("default-src 'none'"));
                app = app.layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::HeaderName::from_static("content-security-policy"),
                    v,
                ));
            }
        }
    }

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!(%addr, "listening");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

#[cfg(any())]
mod tests {
    use super::*;
    use axum::extract::{Path, State};
    // legacy tests removed; see crates/api/tests/*

    #[tokio::test]
    async fn reader_items_ids_limit_n() {
        use crate::compat::reader::types::ReaderItemsIdsQuery;
        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // 用户与 feed + 两条 entry
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "ids".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "ids".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let user = AuthUser::from_bearer(&db, &login.0.token).await.unwrap();
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let feed_am = feed::ActiveModel {
            user_id: Set(user.user_id),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://ex".into())),
            feed_url: Set("https://ex/rss".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let f = feed_am.insert(&db).await.unwrap();
        for i in 0..2 {
            let am = entry::ActiveModel {
                feed_id: Set(f.id),
                guid: Set(Some(format!("g{}", i))),
                url: Set(Some(format!("https://ex/{}", i))),
                title: Set(Some(format!("t{}", i))),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            let _ = am.insert(&db).await.unwrap();
        }
        let q = ReaderItemsIdsQuery {
            n: Some(1),
            s: None,
            c: None,
            xt: None,
            q: None,
        };
        let out = crate::compat::reader::handlers::items_ids(&st, user.user_id, &q)
            .await
            .unwrap();
        assert!(out.item_refs.len() <= 1);
    }

    #[tokio::test]
    async fn reader_items_contents_minimal_fields() {
        use crate::compat::reader::types::ReaderItemsContentsQuery;
        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // 用户与 feed + 一条 entry
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "cnts".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "cnts".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let user = AuthUser::from_bearer(&db, &login.0.token).await.unwrap();
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let feed_am = feed::ActiveModel {
            user_id: Set(user.user_id),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("Feed".into())),
            site_url: Set(Some("https://site".into())),
            feed_url: Set("https://site/rss".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let f = feed_am.insert(&db).await.unwrap();
        let e_am = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            url: Set(Some("https://site/p/1".into())),
            title: Set(Some("Hello".into())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let _ = e_am.insert(&db).await.unwrap();
        let q = ReaderItemsContentsQuery {
            n: Some(10),
            s: None,
            c: None,
            q: None,
            xt: None,
        };
        let out = crate::compat::reader::handlers::items_contents(&st, user.user_id, &q)
            .await
            .unwrap();
        assert_eq!(out.items.len(), 1);
        let it = &out.items[0];
        assert_eq!(it.origin.stream_id, format!("feed/{}", "https://site/rss"));
        assert!(it.alternate.iter().any(|l| l.href == "https://site/p/1"));
        assert!(it.categories.iter().any(|c| c.ends_with("reading-list")));
    }

    #[tokio::test]
    async fn fever_groups_feeds_basic() {
        use serde_json::json;
        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // 用户 + fever key + 一个分类和 feed
        let create = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "fgf".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "fgf".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;
        let _ = set_fever_key(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(create.0.id),
            Json(SetFeverKeyReq {
                api_password: "pp".into(),
            }),
        )
        .await
        .unwrap();
        let key = format!("{:x}", Md5::digest(b"fgf:pp"));
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let cat = category::ActiveModel {
            user_id: Set(create.0.id),
            name: Set("news".into()),
            created_at: Set(now),
            ..Default::default()
        };
        let cat = cat.insert(&db).await.unwrap();
        let feed_am = feed::ActiveModel {
            user_id: Set(create.0.id),
            category_id: Set(Some(cat.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("F".into())),
            site_url: Set(Some("https://s".into())),
            feed_url: Set("https://s/rss".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let _ = feed_am.insert(&db).await.unwrap();
        let resp = crate::compat::fever::endpoint(
            State(st.clone()),
            Query(crate::compat::fever::FeverQuery {
                api: None,
                api_key: Some(key),
                groups: Some(1),
                feeds: Some(1),
                favicons: None,
                items: None,
                since_id: None,
                limit: None,
                unread_item_ids: None,
                saved_item_ids: None,
                mark: None,
                r#as: None,
                id: None,
                before: None,
            }),
        )
        .await;
        let v = json_from_response(resp).await;
        assert!(v.get("groups").is_some());
        assert!(v.get("feeds").is_some());
    }

    // HTTP 级路由集成测试计划迁移到 crates/api/tests/，此处暂以 handlers 级单测为主

    #[tokio::test]
    async fn auth_login_disabled() {
        let db = setup_db().await;
        let mut st = AppState::new(db);
        st.cfg.disable_local_auth = true;
        let err = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .err()
        .unwrap();
        let (status, code) = resp_to_status_and_code(err.into_response());
        assert_eq!(status, 403);
        assert_eq!(code, "forbidden");
    }

    #[tokio::test]
    async fn proxy_token_mint_and_auth() {
        let db = setup_db().await;
        let mut st = AppState::new(db.clone());
        st.cfg.auth_proxy_header = Some("X-Forwarded-User".into());
        st.cfg.auth_proxy_user_creation = true;
        // prepare headers
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::HeaderName::from_static("x-forwarded-user"),
            axum::http::HeaderValue::from_static("bob"),
        );
        let resp = auth_proxy_token(State(st.clone()), headers).await.unwrap();
        let token = resp.0.token;
        // token should authenticate
        let auth = super::AuthUser::from_bearer(&db, &token).await.unwrap();
        assert!(auth.user_id > 0);
    }

    #[tokio::test]
    async fn login_rate_limit_blocks_after_threshold() {
        let db = setup_db().await;
        let mut st = AppState::new(db.clone());
        st.cfg.login_max_attempts = 1; // allow one failure within window
        st.cfg.login_window_secs = 60;
        // Ensure user exists with known password
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "rl".into(),
                password: "good".into(),
            }),
        )
        .await
        .unwrap();
        // 1st attempt wrong password -> 401
        let e1 = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "rl".into(),
                password: "bad".into(),
                name: None,
            }),
        )
        .await
        .err()
        .unwrap();
        let (s1, _) = resp_to_status_and_code(e1.into_response());
        assert_eq!(s1, 401);
        // 2nd attempt wrong password -> 429 too_many_requests
        let e2 = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "rl".into(),
                password: "stillbad".into(),
                name: None,
            }),
        )
        .await
        .err()
        .unwrap();
        let (s2, code2) = resp_to_status_and_code(e2.into_response());
        assert_eq!(s2, 429);
        assert_eq!(code2, "too_many_requests");
    }

    #[tokio::test]
    async fn auth_token_expiry_and_last_used_update() {
        let db = setup_db().await;
        let st = AppState::new(db);
        // 创建用户
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        // 登录颁发 token
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "p".into(),
                name: Some("t".into()),
            }),
        )
        .await
        .unwrap();
        let token_plain = login.0.token.clone();

        // 1) 正常鉴权应通过且刷新 last_used_at
        let before = Token::find()
            .filter(token::Column::TokenPlain.eq(token_plain.clone()))
            .one(&st.db)
            .await
            .unwrap()
            .and_then(|m| m.last_used_at);
        // 稍作等待，确保时间可比较
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _ = AuthUser::from_bearer(&st.db, &token_plain).await.unwrap();
        let after = Token::find()
            .filter(token::Column::TokenPlain.eq(token_plain.clone()))
            .one(&st.db)
            .await
            .unwrap()
            .and_then(|m| m.last_used_at);
        assert!(after.is_some());
        if let (Some(b), Some(a)) = (before, after) {
            assert!(a >= b);
        }

        // 2) 人为设置过期，再次鉴权应失败
        if let Some(model) = Token::find()
            .filter(token::Column::TokenPlain.eq(token_plain.clone()))
            .one(&st.db)
            .await
            .unwrap()
        {
            let mut am: token::ActiveModel = model.into();
            let past = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap())
                - chrono::Duration::seconds(10);
            am.expires_at = Set(Some(past));
            am.update(&st.db).await.unwrap();
        }
        let err = AuthUser::from_bearer(&st.db, &token_plain)
            .await
            .err()
            .unwrap();
        let (status, code) = resp_to_status_and_code(err.into_response());
        assert_eq!(status, 401);
        assert_eq!(code, "unauthorized");
    }

    // 迁移由 testkit::setup_db 完成

    fn resp_to_status_and_code(resp: Response) -> (u16, String) {
        use axum::http::StatusCode;
        use http_body_util::BodyExt;
        let status = resp.status().as_u16();
        let body = futures::executor::block_on(async {
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            String::from_utf8(bytes.to_vec()).unwrap()
        });
        let code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("code")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        (status, code)
    }

    #[tokio::test]
    async fn create_user_duplicate() {
        let db = setup_db().await;
        let st = AppState::new(db);
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let err = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u2".into(),
                password: "p".into(),
            }),
        )
        .await
        .err()
        .unwrap();
        let resp = err.into_response();
        let (status, code) = resp_to_status_and_code(resp);
        assert_eq!(status, 403);
        assert_eq!(code, "forbidden");
    }

    #[tokio::test]
    async fn auth_invalid_password() {
        let db = setup_db().await;
        let st = AppState::new(db);
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let err = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "wrong".into(),
                name: None,
            }),
        )
        .await
        .err()
        .unwrap();
        let (status, code) = resp_to_status_and_code(err.into_response());
        assert_eq!(status, 401);
        assert_eq!(code, "unauthorized");
    }

    #[tokio::test]
    async fn create_feed_invalid_url() {
        let db = setup_db().await;
        let st = AppState::new(db);
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;
        let body = CreateFeedReq {
            category_id: None,
            r#type: "rss".into(),
            title: None,
            site_url: None,
            feed_url: "not a url".into(),
            rule_id: None,
            rule_params_json: None,
            user_agent: None,
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: None,
            disable_http2: None,
            allow_invalid_certs: None,
            request_timeout_ms: Some(1000),
            username: None,
            password: None,
            disabled: None,
        };
        let err = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(body),
        )
        .await
        .err()
        .unwrap();
        let (status, code) = resp_to_status_and_code(err.into_response());
        assert_eq!(status, 400);
        assert_eq!(code, "bad_request");
    }

    #[tokio::test]
    async fn list_entries_filters() {
        let db = setup_db().await;
        let st = AppState::new(db);
        // user + token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;
        // category + feed + entries
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let cat = category::ActiveModel {
            user_id: Set(1),
            name: Set("c".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&st.db)
        .await
        .unwrap();
        let feed_am = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(Some(cat.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(None),
            feed_url: Set("https://x".into()),
            rule_id: Set(None),
            user_agent: Set(None),
            headers_json: Set(None),
            cookies: Set(None),
            proxy_url: Set(None),
            fetch_via_proxy: Set(false),
            disable_http2: Set(false),
            allow_invalid_certs: Set(false),
            request_timeout_ms: Set(None),
            checked_at: Set(None),
            next_run_at: Set(None),
            etag: Set(None),
            last_modified: Set(None),
            last_status: Set(None),
            error_count: Set(0),
            disabled: Set(false),
            scraper_rules: Set(None),
            rewrite_rules: Set(None),
            blocklist_rules: Set(None),
            keeplist_rules: Set(None),
            url_rewrite_rules: Set(None),
            block_filter_entry_rules: Set(None),
            keep_filter_entry_rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let f = feed_am.insert(&st.db).await.unwrap();
        let mut e1: entry::ActiveModel = Default::default();
        e1.feed_id = Set(f.id);
        e1.guid = Set(Some("g1".into()));
        e1.title = Set(Some("hello".into()));
        e1.created_at = Set(now);
        e1.updated_at = Set(now);
        e1.is_read = Set(false);
        e1.is_starred = Set(false);
        let e1 = e1.insert(&st.db).await.unwrap();
        let mut e2: entry::ActiveModel = Default::default();
        e2.feed_id = Set(f.id);
        e2.guid = Set(Some("g2".into()));
        e2.title = Set(Some("world".into()));
        e2.created_at = Set(now);
        e2.updated_at = Set(now);
        e2.is_read = Set(true);
        e2.is_starred = Set(true);
        let _e2 = e2.insert(&st.db).await.unwrap();

        // unread only
        let q = EntriesQuery {
            feed_id: Some(f.id),
            category_id: None,
            status: Some(StatusFilter::Unread),
            limit: Some(50),
            offset: Some(0),
            q: None,
            sort_by: None,
            order: None,
        };
        let res = list_entries(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Query(q),
        )
        .await
        .unwrap();
        assert_eq!(res.0.len(), 1);
        assert_eq!(res.0[0].title.as_deref(), Some("hello"));

        // starred only
        let q = EntriesQuery {
            feed_id: Some(f.id),
            category_id: None,
            status: Some(StatusFilter::Starred),
            limit: Some(50),
            offset: Some(0),
            q: None,
            sort_by: None,
            order: None,
        };
        let res = list_entries(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Query(q),
        )
        .await
        .unwrap();
        assert_eq!(res.0.len(), 1);
        assert_eq!(res.0[0].title.as_deref(), Some("world"));
    }

    #[tokio::test]
    async fn create_feed_with_basic_auth_fields() {
        use captura_storage::entity::prelude::*;
        use sea_orm::EntityTrait;

        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // user + token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "basic".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "basic".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;

        let body = CreateFeedReq {
            category_id: None,
            r#type: "rss".into(),
            title: Some("ba".into()),
            site_url: None,
            feed_url: "https://example.com/ba".into(),
            rule_id: None,
            rule_params_json: None,
            user_agent: Some("captura-tests/0.1".into()),
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: Some(false),
            disable_http2: Some(false),
            allow_invalid_certs: Some(false),
            request_timeout_ms: Some(1000),
            username: Some("authu".into()),
            password: Some("authp".into()),
            disabled: Some(false),
        };
        let created = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(body),
        )
        .await
        .unwrap();
        let fid = created.0.id;
        let f = Feed::find_by_id(fid).one(&st.db).await.unwrap().unwrap();
        assert_eq!(f.username.as_deref(), Some("authu"));
        assert_eq!(f.password.as_deref(), Some("authp"));
    }

    #[tokio::test]
    async fn update_feed_basic_auth_fields() {
        use captura_storage::entity::prelude::*;
        use sea_orm::EntityTrait;

        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // user + token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "basic2".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "basic2".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;

        // create
        let created = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(CreateFeedReq {
                category_id: None,
                r#type: "rss".into(),
                title: Some("ba2".into()),
                site_url: None,
                feed_url: "https://example.com/ba2".into(),
                rule_id: None,
                rule_params_json: None,
                user_agent: Some("captura-tests/0.1".into()),
                headers_json: None,
                cookies: None,
                proxy_url: None,
                fetch_via_proxy: Some(false),
                disable_http2: Some(false),
                allow_invalid_certs: Some(false),
                request_timeout_ms: Some(1000),
                username: Some("u1".into()),
                password: Some("p1".into()),
                disabled: Some(false),
            }),
        )
        .await
        .unwrap();
        let fid = created.0.id;

        // update
        let _ = crate::feeds::update_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(fid),
            Json(UpdateFeedReq {
                title: None,
                category_id: None,
                disabled: None,
                user_agent: None,
                headers_json: None,
                cookies: None,
                proxy_url: None,
                fetch_via_proxy: None,
                disable_http2: None,
                allow_invalid_certs: None,
                request_timeout_ms: None,
                integrations_json: None,
                rule_params_json: None,
                username: Some("u2".into()),
                password: Some("p2".into()),
            }),
        )
        .await
        .unwrap();

        // verify
        let f = Feed::find_by_id(fid).one(&st.db).await.unwrap().unwrap();
        assert_eq!(f.username.as_deref(), Some("u2"));
        assert_eq!(f.password.as_deref(), Some("p2"));
    }

    #[tokio::test]
    async fn update_feed_clear_cookie_proxy_on_empty() {
        use captura_storage::entity::prelude::*;
        use sea_orm::EntityTrait;

        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // user + token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "cookies".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "cookies".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;

        // create with cookie/proxy
        let created = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(CreateFeedReq {
                category_id: None,
                r#type: "rss".into(),
                title: Some("ck".into()),
                site_url: None,
                feed_url: "https://example.com/ck".into(),
                rule_id: None,
                rule_params_json: None,
                user_agent: None,
                headers_json: None,
                cookies: Some("a=b".into()),
                proxy_url: Some("http://proxy".into()),
                fetch_via_proxy: Some(false),
                disable_http2: Some(false),
                allow_invalid_certs: Some(false),
                request_timeout_ms: Some(1000),
                username: None,
                password: None,
                disabled: Some(false),
            }),
        )
        .await
        .unwrap();
        let fid = created.0.id;

        // update to empty values -> clear
        let _ = crate::feeds::update_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(fid),
            Json(UpdateFeedReq {
                title: None,
                category_id: None,
                disabled: None,
                user_agent: None,
                headers_json: None,
                cookies: Some("".into()),
                proxy_url: Some("".into()),
                fetch_via_proxy: None,
                disable_http2: None,
                allow_invalid_certs: None,
                request_timeout_ms: None,
                integrations_json: None,
                rule_params_json: None,
                username: None,
                password: None,
            }),
        )
        .await
        .unwrap();

        let f = Feed::find_by_id(fid).one(&st.db).await.unwrap().unwrap();
        assert!(f.cookies.is_none());
        assert!(f.proxy_url.is_none());
    }

    #[tokio::test]
    async fn update_feed_clear_user_agent_on_empty() {
        use captura_storage::entity::prelude::*;
        use sea_orm::EntityTrait;

        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // user + token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "uag".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "uag".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;

        // create with user_agent
        let created = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(CreateFeedReq {
                category_id: None,
                r#type: "rss".into(),
                title: Some("ua".into()),
                site_url: None,
                feed_url: "https://example.com/ua".into(),
                rule_id: None,
                rule_params_json: None,
                user_agent: Some("UA".into()),
                headers_json: None,
                cookies: None,
                proxy_url: None,
                fetch_via_proxy: Some(false),
                disable_http2: Some(false),
                allow_invalid_certs: Some(false),
                request_timeout_ms: Some(1000),
                username: None,
                password: None,
                disabled: Some(false),
            }),
        )
        .await
        .unwrap();
        let fid = created.0.id;

        // update to empty -> clear
        let _ = crate::feeds::update_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(fid),
            Json(UpdateFeedReq {
                title: None,
                category_id: None,
                disabled: None,
                user_agent: Some("".into()),
                headers_json: None,
                cookies: None,
                proxy_url: None,
                fetch_via_proxy: None,
                disable_http2: None,
                allow_invalid_certs: None,
                request_timeout_ms: None,
                integrations_json: None,
                rule_params_json: None,
                username: None,
                password: None,
            }),
        )
        .await
        .unwrap();

        let f = Feed::find_by_id(fid).one(&st.db).await.unwrap().unwrap();
        assert!(f.user_agent.is_none());
    }

    #[tokio::test]
    async fn create_feed_clear_ua_cookie_proxy_on_empty() {
        use captura_storage::entity::prelude::*;
        use sea_orm::EntityTrait;

        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // user + token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "create-empty".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "create-empty".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;

        let created = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(CreateFeedReq {
                category_id: None,
                r#type: "rss".into(),
                title: Some("cku".into()),
                site_url: None,
                feed_url: "https://example.com/create-empty".into(),
                rule_id: None,
                rule_params_json: None,
                user_agent: Some("".into()),
                headers_json: None,
                cookies: Some("".into()),
                proxy_url: Some("".into()),
                fetch_via_proxy: Some(false),
                disable_http2: Some(false),
                allow_invalid_certs: Some(false),
                request_timeout_ms: Some(1000),
                username: None,
                password: None,
                disabled: Some(false),
            }),
        )
        .await
        .unwrap();
        let fid = created.0.id;

        let f = Feed::find_by_id(fid).one(&st.db).await.unwrap().unwrap();
        assert!(f.user_agent.is_none());
        assert!(f.cookies.is_none());
        assert!(f.proxy_url.is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_feed_live() {
        if std::env::var("CAPTURA_TEST_LIVE")
            .ok()
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
            == false
        {
            eprintln!("skip live test");
            return;
        }
        let db = setup_db().await;
        let st = AppState::new(db);
        // bootstrap user and token
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token.clone();
        // create a real feed
        let body = CreateFeedReq {
            category_id: None,
            r#type: "atom".into(),
            title: None,
            site_url: None,
            feed_url: "https://blog.rust-lang.org/feed.xml".into(),
            rule_id: None,
            rule_params_json: None,
            user_agent: Some("captura-tests/0.1".into()),
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: Some(false),
            disable_http2: Some(false),
            allow_invalid_certs: Some(false),
            request_timeout_ms: Some(15000),
            username: None,
            password: None,
            disabled: Some(false),
        };
        let created = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(body),
        )
        .await
        .unwrap();
        let fid = created.0.id;
        // refresh via handler
        let resp = refresh_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(fid),
        )
        .await
        .unwrap();
        let inserted = resp.0.get("inserted").and_then(|v| v.as_i64()).unwrap_or(0);
        assert!(inserted >= 0);
    }

    async fn json_from_response(resp: Response) -> Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn fever_auth_and_sections() {
        let db = setup_db().await;
        let st = AppState::new(db);

        // 1) Create user
        let create = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "alice".into(),
                password: "secret".into(),
            }),
        )
        .await
        .unwrap();
        let user_id = create.0.id;

        // 2) Login to get token
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "alice".into(),
                password: "secret".into(),
                name: Some("test".into()),
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;

        // 3) Set Fever key
        let _ = set_fever_key(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(user_id),
            Json(SetFeverKeyReq {
                api_password: "feverpass".into(),
            }),
        )
        .await
        .unwrap();

        // Compute api_key = md5("username:api_password") in lowercase hex
        let key = format!("{:x}", Md5::digest(b"alice:feverpass"));

        // 4) Probe auth
        let base = crate::compat::fever::endpoint(
            State(st.clone()),
            Query(crate::compat::fever::FeverQuery {
                api: Some(1),
                api_key: Some(key.clone()),
                groups: None,
                feeds: None,
                favicons: None,
                items: None,
                since_id: None,
                limit: None,
                unread_item_ids: None,
                saved_item_ids: None,
                mark: None,
                r#as: None,
                id: None,
                before: None,
            }),
        )
        .await;
        let base_json = json_from_response(base).await;
        assert_eq!(base_json["auth"], 1);

        // 5) Insert a category, feed, and one entry
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let cat = category::ActiveModel {
            user_id: Set(user_id),
            name: Set("news".into()),
            created_at: Set(now),
            ..Default::default()
        };
        let cat = cat.insert(&st.db).await.unwrap();

        let feed_am = feed::ActiveModel {
            user_id: Set(user_id),
            category_id: Set(Some(cat.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("Example".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            rule_id: Set(None),
            user_agent: Set(None),
            headers_json: Set(None),
            cookies: Set(None),
            proxy_url: Set(None),
            fetch_via_proxy: Set(false),
            disable_http2: Set(false),
            allow_invalid_certs: Set(false),
            request_timeout_ms: Set(Some(5000)),
            checked_at: Set(None),
            next_run_at: Set(None),
            etag: Set(None),
            last_modified: Set(None),
            last_status: Set(None),
            error_count: Set(0),
            disabled: Set(false),
            scraper_rules: Set(None),
            rewrite_rules: Set(None),
            blocklist_rules: Set(None),
            keeplist_rules: Set(None),
            url_rewrite_rules: Set(None),
            block_filter_entry_rules: Set(None),
            keep_filter_entry_rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let feed = feed_am.insert(&st.db).await.unwrap();

        let entry_am = entry::ActiveModel {
            feed_id: Set(feed.id),
            guid: Set(Some("g1".into())),
            url: Set(Some("https://example.com/1".into())),
            title: Set(Some("Hello".into())),
            summary: Set(Some("S".into())),
            content_html: Set(Some("<p>Hi</p>".into())),
            author: Set(Some("A".into())),
            published_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            hash: Set(None),
            is_read: Set(false),
            is_starred: Set(false),
            extras_json: Set(None),
            ..Default::default()
        };
        let e = entry_am.insert(&st.db).await.unwrap();

        // 6) groups & feeds
        let resp = crate::compat::fever::endpoint(
            State(st.clone()),
            Query(crate::compat::fever::FeverQuery {
                api: None,
                api_key: Some(key.clone()),
                groups: Some(1),
                feeds: Some(1),
                favicons: None,
                items: None,
                since_id: None,
                limit: None,
                unread_item_ids: None,
                saved_item_ids: None,
                mark: None,
                r#as: None,
                id: None,
                before: None,
            }),
        )
        .await;
        let j = json_from_response(resp).await;
        assert!(j.get("groups").is_some());
        assert!(j.get("feeds_groups").is_some());
        assert!(j.get("feeds").is_some());

        // 7) items & unread ids
        let resp = crate::compat::fever::endpoint(
            State(st.clone()),
            Query(crate::compat::fever::FeverQuery {
                api: None,
                api_key: Some(key.clone()),
                groups: None,
                feeds: None,
                favicons: None,
                items: Some(1),
                since_id: Some(0),
                limit: Some(50),
                unread_item_ids: Some(1),
                saved_item_ids: None,
                mark: None,
                r#as: None,
                id: None,
                before: None,
            }),
        )
        .await;
        let j = json_from_response(resp).await;
        assert!(j.get("items").is_some());
        assert_eq!(j["total_items"].as_u64().unwrap_or(0), 1);
        let unread = j["unread_item_ids"].as_str().unwrap_or("").to_string();
        assert!(unread.contains(&e.id.to_string()));
    }

    #[tokio::test]
    async fn create_feed_duplicate_url_bad_request() {
        let db = setup_db().await;
        let st = AppState::new(db.clone());
        // 注册首个用户
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "dup".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        // 登录获取 token
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "dup".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token;
        // 创建同一个 feed 两次
        let body = CreateFeedReq {
            category_id: None,
            r#type: "rss".into(),
            title: None,
            site_url: None,
            feed_url: "https://example.com/feed".into(),
            rule_id: None,
            rule_params_json: None,
            user_agent: None,
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: None,
            disable_http2: None,
            allow_invalid_certs: None,
            request_timeout_ms: Some(1000),
            disabled: None,
        };
        let _ = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(body),
        )
        .await
        .unwrap();
        // 第二次提交相同的 feed_url
        let body2 = CreateFeedReq {
            category_id: None,
            r#type: "rss".into(),
            title: None,
            site_url: None,
            feed_url: "https://example.com/feed".into(),
            rule_id: None,
            rule_params_json: None,
            user_agent: None,
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: None,
            disable_http2: None,
            allow_invalid_certs: None,
            request_timeout_ms: Some(1000),
            disabled: None,
        };
        let err = create_feed(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Json(body2),
        )
        .await
        .err()
        .unwrap();
        let (status, code) = resp_to_status_and_code(err.into_response());
        assert_eq!(status, 400);
        assert_eq!(code, "bad_request");
    }

    #[tokio::test]
    async fn cross_user_mark_read_forbidden() {
        use captura_storage::entity::user;
        let db = setup_db().await;
        let st = AppState::new(db.clone());

        // 用户A：注册并登录
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "alice".into(),
                password: "pa".into(),
            }),
        )
        .await
        .unwrap();
        let login_a = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "alice".into(),
                password: "pa".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let _token_a = login_a.0.token;

        // 直接插入用户B与其 token（避免触发 create_user 限制）
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let user_b = user::ActiveModel {
            username: Set("bob".into()),
            password_hash: Set("h".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&st.db)
        .await
        .unwrap();
        let token_b_plain = "token-b".to_string();
        let token_b_hash = format!("{:x}", Sha256::digest(token_b_plain.as_bytes()));
        let am = token::ActiveModel {
            user_id: Set(user_b.id),
            name: Set(Some("b".into())),
            token_hash: Set(token_b_hash),
            token_plain: Set(Some(token_b_plain.clone())),
            created_at: Set(now),
            last_used_at: Set(Some(now)),
            expires_at: Set(None),
            ..Default::default()
        };
        let _ = am.insert(&st.db).await.unwrap();

        // 为用户A插入一个 feed 与 entry
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://e.com".into())),
            feed_url: Set("https://e.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&st.db)
        .await
        .unwrap();
        let e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            url: Set(Some("https://e.com/1".into())),
            title: Set(Some("hello".into())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&st.db)
        .await
        .unwrap();

        // 用用户B的 token 标记用户A的 entry 为已读，应 forbidden
        let err = crate::entries::mark_read(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token_b_plain).unwrap()),
            Path(e.id),
            Json(crate::entries::BoolBody { value: true }),
        )
        .await
        .err()
        .unwrap();
        let (status, code) = resp_to_status_and_code(err.into_response());
        assert_eq!(status, 403);
        assert_eq!(code, "forbidden");
    }

    #[tokio::test]
    async fn http_login_smoke() {
        use axum::http::{Request, StatusCode};
        let db = setup_db().await;
        let app = Router::new()
            .route("/api/v1/users", post(create_user))
            .route("/api/v1/auth/login", post(auth_login))
            .with_state(AppState::new(db));

        // 注册首个用户
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/v1/users")
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({"username":"u1","password":"p1"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 登录
        let resp = app
            .oneshot(
                Request::post("/api/v1/auth/login")
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({"username":"u1","password":"p1"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn fever_favicons_and_get_binary() {
        use captura_storage::entity::favicon as fv;
        let db = setup_db().await;
        let st = AppState::new(db);

        // user + token + fever
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "alice".into(),
                password: "secret".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "alice".into(),
                password: "secret".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token.clone();
        let _ = set_fever_key(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(1),
            Json(SetFeverKeyReq {
                api_password: "fever".into(),
            }),
        )
        .await
        .unwrap();
        let key = format!("{:x}", Md5::digest(b"alice:fever"));

        // feed + favicon
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let cat = category::ActiveModel {
            user_id: Set(1),
            name: Set("c".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&st.db)
        .await
        .unwrap();
        let mut feed_am: feed::ActiveModel = Default::default();
        feed_am.user_id = Set(1);
        feed_am.category_id = Set(Some(cat.id));
        feed_am.r#type = Set(feed::FeedType::Rss);
        feed_am.title = Set(Some("t".into()));
        feed_am.feed_url = Set("https://example.com/feed".into());
        feed_am.site_url = Set(Some("https://example.com".into()));
        feed_am.created_at = Set(now);
        feed_am.updated_at = Set(now);
        let f = feed_am.insert(&st.db).await.unwrap();

        let fav = fv::ActiveModel {
            feed_id: Set(Some(f.id)),
            url: Set(Some("https://example.com/favicon.ico".into())),
            mime: Set(Some("image/x-icon".into())),
            data: Set(Some(vec![1, 2, 3])),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&st.db)
        .await
        .unwrap();
        let mut fm: feed::ActiveModel = f.into();
        fm.favicon_id = Set(Some(fav.id));
        let f = fm.update(&st.db).await.unwrap();

        // fever favicons
        let resp = crate::compat::fever::endpoint(
            State(st.clone()),
            Query(crate::compat::fever::FeverQuery {
                api: None,
                api_key: Some(key.clone()),
                groups: None,
                feeds: Some(1),
                favicons: Some(1),
                items: None,
                since_id: None,
                limit: None,
                unread_item_ids: None,
                saved_item_ids: None,
                mark: None,
                r#as: None,
                id: None,
                before: None,
            }),
        )
        .await;
        let j = json_from_response(resp).await;
        assert!(j.get("feeds").is_some());
        assert!(j.get("favicons").is_some());
        let favs = j["favicons"].as_array().unwrap();
        assert!(!favs.is_empty());

        // get favicon binary
        let resp = crate::favicon::get(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(fav.id),
        )
        .await
        .unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes.to_vec(), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn refresh_favicon_with_test_hook() {
        let db = setup_db().await;
        let st = AppState::new(db);
        let _ = create_user(
            State(st.clone()),
            Json(CreateUserReq {
                username: "u".into(),
                password: "p".into(),
            }),
        )
        .await
        .unwrap();
        let login = auth_login(
            State(st.clone()),
            Json(AuthLoginReq {
                username: "u".into(),
                password: "p".into(),
                name: None,
            }),
        )
        .await
        .unwrap();
        let token = login.0.token.clone();

        // set test favicon hook
        crate::favicon::TEST_FAVICON_RESP
            .set((vec![9, 9, 9], Some("image/png".into())))
            .ok();

        // feed
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let mut feed_am: feed::ActiveModel = Default::default();
        feed_am.user_id = Set(1);
        feed_am.r#type = Set(feed::FeedType::Rss);
        feed_am.feed_url = Set("https://example.com/feed".into());
        feed_am.site_url = Set(Some("https://example.com".into()));
        feed_am.created_at = Set(now);
        feed_am.updated_at = Set(now);
        let f = feed_am.insert(&st.db).await.unwrap();

        // refresh
        let resp = crate::favicon::refresh(
            State(st.clone()),
            TypedHeader(Authorization::bearer(&token).unwrap()),
            Path(f.id),
        )
        .await
        .unwrap();
        assert!(resp.0.updated);
        assert!(resp.0.favicon_id > 0);
    }
}

// legacy feed refresh/enqueue handlers removed (moved to crates/api/src/feeds.rs)

#[derive(Deserialize)]
#[allow(dead_code)]
struct JobsQuery {
    status: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Serialize)]
#[allow(dead_code)]
struct JobDto {
    id: i64,
    job_type: String,
    status: String,
    run_at: String,
    attempts: i32,
    last_error: Option<String>,
}

#[allow(dead_code)]
async fn list_jobs(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<JobsQuery>,
) -> ApiResult<Json<Vec<JobDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    let mut sel = Job::find().filter(job::Column::UserId.eq(user.user_id));
    if let Some(ref s) = q.status {
        let st = match &s[..] {
            "pending" => job::JobStatus::Pending,
            "running" => job::JobStatus::Running,
            "done" => job::JobStatus::Done,
            "failed" => job::JobStatus::Failed,
            _ => job::JobStatus::Pending,
        };
        sel = sel.filter(job::Column::Status.eq(st));
    }
    let rows = sel
        .order_by_desc(job::Column::RunAt)
        .limit(q.limit.unwrap_or(50))
        .offset(q.offset.unwrap_or(0))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let list = rows
        .into_iter()
        .map(|j| JobDto {
            id: j.id,
            job_type: match j.job_type {
                job::JobType::FeedRefresh => "feed_refresh".into(),
                job::JobType::RuleRefresh => "rule_refresh".into(),
                job::JobType::Favicon => "favicon".into(),
                job::JobType::Prune => "prune".into(),
                job::JobType::Integration => "integration".into(),
            },
            status: match j.status {
                job::JobStatus::Pending => "pending".into(),
                job::JobStatus::Running => "running".into(),
                job::JobStatus::Done => "done".into(),
                job::JobStatus::Failed => "failed".into(),
            },
            run_at: j.run_at.to_rfc3339(),
            attempts: j.attempts,
            last_error: j.last_error,
        })
        .collect();
    Ok(Json(list))
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct IntegrationJobsQuery {
    status: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Serialize)]
#[allow(dead_code)]
struct IntegrationJobDto {
    id: i64,
    status: String,
    run_at: String,
    attempts: i32,
    last_error: Option<String>,
    feed_id: Option<i64>,
    payload: serde_json::Value,
}

#[allow(dead_code)]
async fn list_integration_jobs(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<IntegrationJobsQuery>,
) -> ApiResult<Json<Vec<IntegrationJobDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    let mut sel = Job::find()
        .filter(job::Column::UserId.eq(user.user_id))
        .filter(job::Column::JobType.eq(job::JobType::Integration));
    if let Some(ref s) = q.status {
        let st = match &s[..] {
            "pending" => job::JobStatus::Pending,
            "running" => job::JobStatus::Running,
            "done" => job::JobStatus::Done,
            "failed" => job::JobStatus::Failed,
            _ => job::JobStatus::Pending,
        };
        sel = sel.filter(job::Column::Status.eq(st));
    }
    let rows = sel
        .order_by_desc(job::Column::RunAt)
        .limit(q.limit.unwrap_or(50))
        .offset(q.offset.unwrap_or(0))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let list = rows
        .into_iter()
        .map(|j| IntegrationJobDto {
            id: j.id,
            status: match j.status {
                job::JobStatus::Pending => "pending".into(),
                job::JobStatus::Running => "running".into(),
                job::JobStatus::Done => "done".into(),
                job::JobStatus::Failed => "failed".into(),
            },
            run_at: j.run_at.to_rfc3339(),
            attempts: j.attempts,
            last_error: j.last_error,
            feed_id: j.feed_id,
            payload: j.payload_json.unwrap_or(serde_json::json!({})),
        })
        .collect();
    Ok(Json(list))
}

#[allow(dead_code)]
async fn run_jobs_once(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<serde_json::Value>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let n = scheduler::run_once(&st.db, 10).await.map_err(internal)?;
    Ok(Json(serde_json::json!({"processed": n})))
}

#[allow(dead_code)]
async fn enqueue_due_feeds(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<serde_json::Value>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let n = scheduler::enqueue_due_feeds(&st.db, 100)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"enqueued": n})))
}

// helper fns 移至 crate::util；Google Reader 兼容实现已迁移至 crate::compat::reader

// Reader DTO moved to compat::reader

// ...

// ...

// ...

// ...

// ...

// ...

// ...

// ensure_category moved to compat::reader
pub(crate) async fn oidc_providers(State(st): State<AppState>) -> ApiResult<Json<Vec<String>>> {
    let names: Vec<String> = st
        .cfg
        .oidc_providers
        .iter()
        .map(|p| p.name.clone())
        .collect();
    Ok(Json(names))
}
// 登录限流逻辑请见 crate::util
