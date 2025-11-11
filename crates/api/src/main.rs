//! Captura API service entrypoint (Axum-based).

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
// use axum::debug_handler;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::typed_header::TypedHeader;
use base64::Engine as _;
use captura_crawler::{self as crawler, CrawlOptions};
use captura_pipeline::{refresh_feed_with_meta, refresh_rule_with_yaml};
use captura_rules::{parse_rule, RuleSpec};
use captura_scheduler as scheduler;
use captura_storage::connect as db_connect;
use captura_storage::entity::{category, entry, feed, job, prelude::*, rule, token, user};
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use md5::Md5;
use migration::migrate;
#[cfg(test)]
use once_cell::sync::OnceCell;
use rand_core::OsRng;
use rand_core::RngCore;
use scraper::{Html, Selector};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set,
};
use sea_orm::prelude::Expr;
use sea_orm::ExprTrait;
use sea_orm::sea_query::{Expr as QExpr};
// use sea_orm::sea_query::{Alias as SAlias, Expr as SExpr, Func as SFunc};
use sea_orm::sea_query::OnConflict;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;
use url::Url;
use axum::Form;

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

    let api_v1 = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        // users & auth
        .route("/users", post(create_user))
        .route("/users/:id/fever-key", post(set_fever_key))
        .route("/auth/login", post(auth_login))
        // feeds & entries
        .route("/feeds", post(create_feed).get(list_feeds))
        .route(
            "/feeds/:id",
            get(get_feed).patch(update_feed).delete(delete_feed),
        )
        .route("/feeds/:id/refresh", post(refresh_feed))
        .route("/feeds/:id/enqueue-refresh", post(enqueue_feed_refresh))
        .route("/feeds/:id/favicon/refresh", post(refresh_favicon))
        .route("/favicons/:id", get(get_favicon))
        .route("/categories", get(list_categories).post(create_category))
        .route(
            "/categories/:id",
            get(get_category)
                .put(update_category)
                .delete(delete_category),
        )
        .route("/entries", get(list_entries))
        .route("/entries/mark-all-read", post(mark_all_read))
        .route("/entries/:id/read", post(mark_read))
        .route("/entries/:id/star", post(mark_star))
        .route("/opml/export", get(opml_export))
        .route("/opml/import", post(opml_import))
        // fever compatibility (read-only subset)
        .route("/fever", get(fever_endpoint).post(fever_endpoint))
        // jobs
        .route("/jobs", get(list_jobs))
        .route("/jobs/run-once", post(run_jobs_once))
        .route("/jobs/enqueue-due-feeds", post(enqueue_due_feeds))
        // rules
        .route("/rules", get(list_rules).post(create_rule))
        .route(
            "/rules/:id",
            get(get_rule).put(update_rule).delete(delete_rule),
        )
        .route("/rules/try", post(try_rule))
        // Google Reader compatible (read-only for now)
        .route(
            "/reader/api/0/subscription/list",
            get(reader_subscription_list),
        )
        .route(
            "/reader/api/0/stream/contents/user/-/state/com.google/reading-list",
            get(reader_stream_contents),
        )
        .route("/reader/api/0/edit-tag", post(reader_edit_tag))
        .route("/reader/api/0/mark-all-as-read", post(reader_mark_all_read))
        .route("/reader/api/0/unread-count", get(reader_unread_count))
        .route("/reader/api/0/subscription/quickadd", post(reader_subscription_quickadd))
        .route("/reader/api/0/subscription/edit", post(reader_subscription_edit))
        .route("/reader/api/0/stream/items/ids", get(reader_items_ids))
        .route("/reader/api/0/stream/items/contents", get(reader_items_contents));

    let app = Router::new()
        .nest("/api/v1", api_v1)
        .nest("/v1", miniflux_router())
        .with_state(AppState { db });

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!(%addr, "listening");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
}

fn miniflux_router() -> Router<AppState> {
    Router::new()
        // feeds
        .route("/feeds", get(mf_list_feeds).post(mf_create_feed))
        .route("/feeds/:id", get(mf_get_feed).put(mf_update_feed).delete(mf_delete_feed))
        .route("/feeds/:id/mark-all-read", post(mf_feed_mark_all_read))
        .route("/feeds/:id/refresh", post(mf_feed_refresh))
        // categories
        .route("/categories", get(mf_list_categories).post(mf_create_category))
        .route("/categories/counters", get(mf_categories_counters))
        // entries
        .route("/entries", get(mf_list_entries).put(mf_update_entries_bulk))
        .route("/entries/:id", put(mf_update_entry))
        .route("/entries/:id/read", post(mf_mark_read))
        .route("/entries/:id/unread", post(mf_mark_unread))
        .route("/entries/:id/bookmark", post(mf_mark_star))
        .route("/entries/:id/unbookmark", post(mf_unmark_star))
        // user
        .route("/me", get(mf_me))
        // import/export (OPML)
        .route("/export", get(mf_export))
        .route("/import", post(mf_import))
}

fn mf_auth<'a>(st: &'a AppState, headers: &'a axum::http::HeaderMap) -> impl std::future::Future<Output = ApiResult<AuthUser>> + 'a {
    async move {
        if let Some(v) = headers.get("X-Auth-Token") {
            if let Ok(token) = v.to_str() {
                return AuthUser::from_bearer(&st.db, token).await;
            }
        }
        // fallback to Authorization Bearer if present (for convenience)
        if let Some(v) = headers.get(axum::http::header::AUTHORIZATION) {
            if let Ok(s) = v.to_str() {
                if let Some(t) = s.strip_prefix("Bearer ") {
                    return AuthUser::from_bearer(&st.db, t).await;
                }
            }
        }
        Err(unauthorized("missing token"))
    }
}

#[derive(Serialize)]
struct MfUserDto { id: i64, username: String }

async fn mf_me(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<MfUserDto>> {
    let auth = mf_auth(&st, &headers).await?;
    let u = User::find_by_id(auth.user_id).one(&st.db).await.map_err(internal)?.ok_or_else(|| not_found("user"))?;
    Ok(Json(MfUserDto { id: u.id, username: u.username }))
}

#[derive(Serialize)]
struct MfCategoryDto { id: i64, title: String }

async fn mf_list_categories(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Vec<MfCategoryDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|c| MfCategoryDto { id: c.id, title: c.name })
        .collect();
    Ok(Json(cats))
}

#[derive(Deserialize)]
struct MfCreateCategory { title: String }

async fn mf_create_category(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateCategory>,
) -> ApiResult<Json<MfCategoryDto>> {
    let auth = mf_auth(&st, &headers).await?;
    if body.title.trim().is_empty() { return Err(bad_request("title required")); }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = category::ActiveModel { user_id: Set(auth.user_id), name: Set(body.title.clone()), created_at: Set(now), ..Default::default() };
    let c = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfCategoryDto { id: c.id, title: c.name }))
}

#[derive(Serialize)]
struct MfFeedDto {
    id: i64,
    feed_url: String,
    site_url: Option<String>,
    title: Option<String>,
    category: Option<MfCategoryDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unread_count: Option<i64>,
}

fn map_feed(f: feed::Model, cat: Option<category::Model>) -> MfFeedDto {
    MfFeedDto {
        id: f.id,
        feed_url: f.feed_url,
        site_url: f.site_url,
        title: f.title,
        category: cat.map(|c| MfCategoryDto { id: c.id, title: c.name }),
        unread_count: None,
    }
}

#[derive(Deserialize)]
struct MfFeedsQuery {
    category_id: Option<i64>,
    disabled: Option<bool>,
    has_errors: Option<bool>,
    withCounters: Option<bool>,
    order: Option<String>,
    direction: Option<String>,
}

async fn mf_list_feeds(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfFeedsQuery>,
) -> ApiResult<Json<Vec<MfFeedDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let mut sel = Feed::find().filter(feed::Column::UserId.eq(auth.user_id)).find_also_related(Category);
    if let Some(cid) = q.category_id { sel = sel.filter(feed::Column::CategoryId.eq(cid)); }
    if let Some(dis) = q.disabled { sel = sel.filter(feed::Column::Disabled.eq(dis)); }
    if let Some(err) = q.has_errors { if err { sel = sel.filter(feed::Column::ErrorCount.gt(0)); } }
    match q.order.as_deref() {
        Some("title") => match q.direction.as_deref() { Some("asc") => sel = sel.order_by_asc(feed::Column::Title), _ => sel = sel.order_by_desc(feed::Column::Title) },
        _ => match q.direction.as_deref() { Some("asc") => sel = sel.order_by_asc(feed::Column::Id), _ => sel = sel.order_by_desc(feed::Column::Id) },
    }
    let feeds = sel
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut counters: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if q.withCounters.unwrap_or(false) {
        for (f, _) in &feeds {
            let cnt = Entry::find()
                .filter(entry::Column::FeedId.eq(f.id))
                .filter(entry::Column::IsRead.eq(false))
                .count(&st.db)
                .await
                .map_err(internal)? as i64;
            counters.insert(f.id, cnt);
        }
    }
    let feeds = feeds
        .into_iter()
        .map(|(f, c)| {
            let mut dto = map_feed(f, c);
            if let Some(cnt) = counters.get(&dto.id) { dto.unread_count = Some(*cnt); }
            dto
        })
        .collect();
    Ok(Json(feeds))
}

#[derive(Deserialize)]
struct MfCreateFeed { feed_url: String, category_id: Option<i64>, title: Option<String> }

async fn mf_create_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateFeed>,
) -> ApiResult<Json<MfFeedDto>> {
    let auth = mf_auth(&st, &headers).await?;
    if body.feed_url.trim().is_empty() { return Err(bad_request("feed_url required")); }
    if let Some(cid) = body.category_id { assert_category_ownership(&st.db, auth.user_id, cid).await?; }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = feed::ActiveModel {
        user_id: Set(auth.user_id),
        category_id: Set(body.category_id),
        r#type: Set(feed::FeedType::Rss),
        title: Set(body.title.clone()),
        site_url: Set(None),
        feed_url: Set(body.feed_url.clone()),
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
        favicon_id: Set(None),
        ..Default::default()
    };
    let f = am.insert(&st.db).await.map_err(internal)?;
    let cat = if let Some(cid) = f.category_id { Category::find_by_id(cid).one(&st.db).await.map_err(internal)? } else { None };
    Ok(Json(map_feed(f, cat)))
}

async fn mf_get_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Json<MfFeedDto>> {
    let auth = mf_auth(&st, &headers).await?;
    let (f, c) = Feed::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .find_also_related(Category)
        .one(&st.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("feed"))?;
    Ok(Json(map_feed(f, c)))
}

#[derive(Deserialize)]
struct MfUpdateFeed { title: Option<String>, category_id: Option<i64>, disabled: Option<bool> }

async fn mf_update_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateFeed>,
) -> ApiResult<Json<MfFeedDto>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = Feed::find_by_id(id).filter(feed::Column::UserId.eq(auth.user_id)).one(&st.db).await.map_err(internal)? else { return Err(not_found("feed")); };
    if let Some(cid) = body.category_id { assert_category_ownership(&st.db, auth.user_id, cid).await?; }
    let mut am: feed::ActiveModel = f.into();
    if let Some(t) = body.title { am.title = Set(Some(t)); }
    if let Some(cid) = body.category_id { am.category_id = Set(Some(cid)); }
    if let Some(d) = body.disabled { am.disabled = Set(d); }
    let f = am.update(&st.db).await.map_err(internal)?;
    let cat = if let Some(cid) = f.category_id { Category::find_by_id(cid).one(&st.db).await.map_err(internal)? } else { None };
    Ok(Json(map_feed(f, cat)))
}

async fn mf_delete_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = Feed::find_by_id(id).filter(feed::Column::UserId.eq(auth.user_id)).one(&st.db).await.map_err(internal)? else { return Err(not_found("feed")); };
    let am: feed::ActiveModel = f.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("OK")
}

async fn mf_feed_mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = Feed::find_by_id(id).filter(feed::Column::UserId.eq(auth.user_id)).one(&st.db).await.map_err(internal)? else { return Err(not_found("feed")); };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let _ = Entry::update_many()
        .col_expr(entry::Column::IsRead, Expr::value(true))
        .col_expr(entry::Column::UpdatedAt, Expr::value(now))
        .filter(entry::Column::FeedId.eq(f.id))
        .filter(entry::Column::IsRead.eq(false))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok("OK")
}

#[derive(Deserialize)]
struct MfEntriesQuery {
    status: Option<String>,
    starred: Option<bool>,
    limit: Option<u64>,
    offset: Option<u64>,
    order: Option<String>,
    q: Option<String>,
    before_id: Option<i64>,
    after_id: Option<i64>,
    feed_id: Option<i64>,
    category_id: Option<i64>,
    published_before: Option<i64>,
    published_after: Option<i64>,
}

#[derive(Serialize)]
struct MfEntryDto {
    id: i64,
    feed_id: i64,
    url: Option<String>,
    title: Option<String>,
    author: Option<String>,
    content: Option<String>,
    published_at: Option<String>,
    status: String,
    starred: bool,
}

async fn mf_list_entries(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfEntriesQuery>,
) -> ApiResult<Json<Vec<MfEntryDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(auth.user_id));
    if let Some(ref s) = q.status {
        match s.as_str() {
            "read" => sel = sel.filter(entry::Column::IsRead.eq(true)),
            "unread" => sel = sel.filter(entry::Column::IsRead.eq(false)),
            _ => {}
        }
    }
    if let Some(star) = q.starred { if star { sel = sel.filter(entry::Column::IsStarred.eq(true)); } }
    if let Some(bid) = q.before_id { sel = sel.filter(entry::Column::Id.lt(bid)); }
    if let Some(aid) = q.after_id { sel = sel.filter(entry::Column::Id.gt(aid)); }
    if let Some(fid) = q.feed_id { sel = sel.filter(entry::Column::FeedId.eq(fid)); }
    if let Some(cid) = q.category_id { sel = sel.filter(feed::Column::CategoryId.eq(cid)); }
    if let Some(ref qq) = q.q {
        if !qq.trim().is_empty() {
            let like = format!("%{}%", qq);
            let cond = Condition::any()
                .add(entry::Column::Title.like(like.as_str()))
                .add(entry::Column::Summary.like(like.as_str()))
                .add(entry::Column::ContentHtml.like(like.as_str()));
            sel = sel.filter(cond);
        }
    }
    if let Some(ts) = q.published_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            let dt = dt.with_timezone(&FixedOffset::east_opt(0).unwrap());
            let cond = Condition::any()
                .add(entry::Column::PublishedAt.lte(dt))
                .add(Condition::all().add(entry::Column::PublishedAt.is_null()).add(entry::Column::CreatedAt.lte(dt)));
            sel = sel.filter(cond);
        }
    }
    if let Some(ts) = q.published_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            let dt = dt.with_timezone(&FixedOffset::east_opt(0).unwrap());
            let cond = Condition::any()
                .add(entry::Column::PublishedAt.gte(dt))
                .add(Condition::all().add(entry::Column::PublishedAt.is_null()).add(entry::Column::CreatedAt.gte(dt)));
            sel = sel.filter(cond);
        }
    }
    sel = match q.order.as_deref() {
        Some("id") => sel.order_by_desc(entry::Column::Id),
        _ => sel.order_by_desc(entry::Column::PublishedAt).order_by_desc(entry::Column::CreatedAt),
    };
    if let Some(l) = q.limit { sel = sel.limit(l); }
    if let Some(o) = q.offset { sel = sel.offset(o); }
    let rows = sel.all(&st.db).await.map_err(internal)?;
    let out = rows.into_iter().map(|e| MfEntryDto {
        id: e.id,
        feed_id: e.feed_id,
        url: e.url,
        title: e.title,
        author: e.author,
        content: e.content_html,
        published_at: e.published_at.map(|d| d.to_rfc3339()),
        status: if e.is_read { "read".into() } else { "unread".into() },
        starred: e.is_starred,
    }).collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
struct MfUpdateEntryBody { status: Option<String>, starred: Option<bool> }

async fn mf_update_entry(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateEntryBody>,
) -> ApiResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else { return Err(not_found("entry")); };
    // Verify ownership
    let owner = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owner { return Err(forbidden("not your entry")); }
    let mut am: entry::ActiveModel = e.into();
    if let Some(ref s) = body.status { am.is_read = Set(s == "read"); }
    if let Some(star) = body.starred { am.is_starred = Set(star); }
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("OK")
}

#[derive(Deserialize)]
struct MfUpdateEntriesBulk { entry_ids: Vec<i64>, status: Option<String>, starred: Option<bool> }

async fn mf_update_entries_bulk(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfUpdateEntriesBulk>,
) -> ApiResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    if body.entry_ids.is_empty() { return Ok("OK"); }
    let mut update = Entry::update_many().filter(entry::Column::Id.is_in(body.entry_ids.clone()));
    if let Some(ref s) = body.status { update = update.col_expr(entry::Column::IsRead, Expr::value(s == "read")); }
    if let Some(star) = body.starred { update = update.col_expr(entry::Column::IsStarred, Expr::value(star)); }
    let _ = update.exec(&st.db).await.map_err(internal)?;
    Ok("OK")
}

async fn mf_mark_read(State(st): State<AppState>, headers: axum::http::HeaderMap, Path(id): Path<i64>) -> ApiResult<&'static str> {
    let _ = mf_update_entry(State(st.clone()), headers.clone(), Path(id), Json(MfUpdateEntryBody { status: Some("read".into()), starred: None })).await?;
    Ok("OK")
}
async fn mf_mark_unread(State(st): State<AppState>, headers: axum::http::HeaderMap, Path(id): Path<i64>) -> ApiResult<&'static str> {
    let _ = mf_update_entry(State(st.clone()), headers.clone(), Path(id), Json(MfUpdateEntryBody { status: Some("unread".into()), starred: None })).await?;
    Ok("OK")
}
async fn mf_mark_star(State(st): State<AppState>, headers: axum::http::HeaderMap, Path(id): Path<i64>) -> ApiResult<&'static str> {
    let _ = mf_update_entry(State(st.clone()), headers.clone(), Path(id), Json(MfUpdateEntryBody { status: None, starred: Some(true) })).await?;
    Ok("OK")
}
async fn mf_unmark_star(State(st): State<AppState>, headers: axum::http::HeaderMap, Path(id): Path<i64>) -> ApiResult<&'static str> {
    let _ = mf_update_entry(State(st.clone()), headers.clone(), Path(id), Json(MfUpdateEntryBody { status: None, starred: Some(false) })).await?;
    Ok("OK")
}

#[derive(Serialize)]
struct MfFeedCounter { feed_id: i64, count: i64 }

async fn mf_feeds_counters(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Vec<MfFeedCounter>>> {
    let auth = mf_auth(&st, &headers).await?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feed_ids: Vec<i64> = feeds.iter().map(|f| f.id).collect();
    let pairs: Vec<(i64, i64)> = if feed_ids.is_empty() {
        Vec::new()
    } else {
        Entry::find()
            .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
            .filter(entry::Column::IsRead.eq(false))
            .select_only()
            .column(entry::Column::FeedId)
            .column_as(QExpr::col(entry::Column::Id).count(), "cnt")
            .group_by(entry::Column::FeedId)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?
    };
    let mut map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for (fid, cnt) in pairs { map.insert(fid, cnt); }
    let mut out = Vec::new();
    for f in feeds { out.push(MfFeedCounter { feed_id: f.id, count: *map.get(&f.id).unwrap_or(&0) }); }
    Ok(Json(out))
}

#[derive(Serialize)]
struct MfCategoryCounter { category_id: i64, count: i64 }

async fn mf_categories_counters(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Vec<MfCategoryCounter>>> {
    let auth = mf_auth(&st, &headers).await?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_ids: Vec<i64> = cats.iter().map(|c| c.id).collect();
    let pairs: Vec<(Option<i64>, i64)> = if cat_ids.is_empty() {
        Vec::new()
    } else {
        Entry::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(auth.user_id))
            .filter(entry::Column::IsRead.eq(false))
            .select_only()
            .column_as(QExpr::col((Feed, feed::Column::CategoryId)), "cat")
            .column_as(QExpr::col(entry::Column::Id).count(), "cnt")
            .group_by(QExpr::col((Feed, feed::Column::CategoryId)))
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?
    };
    let mut map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for (opt_cid, cnt) in pairs {
        if let Some(cid) = opt_cid { map.insert(cid, cnt); }
    }
    let mut out = Vec::new();
    for cid in cat_ids {
        out.push(MfCategoryCounter { category_id: cid, count: *map.get(&cid).unwrap_or(&0) });
    }
    Ok(Json(out))
}

// Refresh a single feed (Miniflux compatible)
#[derive(Serialize)]
struct MfRefreshResp { result: String, inserted: usize }

async fn mf_feed_refresh(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Json<MfRefreshResp>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = Feed::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)? else { return Err(not_found("feed")); };
    let (entries, meta) = if matches!(f.r#type, feed::FeedType::Rule) {
        let rule_yaml = match f.rule_id {
            Some(rid) => {
                let r = Rule::find_by_id(rid)
                    .one(&st.db)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| bad_request("rule missing"))?;
                r.yaml
            }
            None => return Err(bad_request("rule_id required for rule-type feed")),
        };
        (
            refresh_rule_with_yaml(&f, &rule_yaml)
                .await
                .map_err(internal)?,
            None,
        )
    } else {
        refresh_feed_with_meta(&f).await.map_err(internal)?
    };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let guids: Vec<String> = entries.iter().filter_map(|n| n.guid.clone()).collect();
    let existing: std::collections::HashSet<String> = if guids.is_empty() {
        Default::default()
    } else {
        Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .filter(entry::Column::Guid.is_in(guids.clone()))
            .select_only()
            .column(entry::Column::Guid)
            .into_tuple::<Option<String>>()
            .all(&st.db)
            .await
            .map_err(internal)?
            .into_iter()
            .flatten()
            .collect()
    };
    let mut models: Vec<entry::ActiveModel> = Vec::new();
    for n in entries {
        if let Some(guid) = n.guid.clone() {
            if existing.contains(&guid) { continue; }
            let mut am: entry::ActiveModel = Default::default();
            am.feed_id = Set(f.id);
            am.guid = Set(Some(guid));
            am.url = Set(n.url);
            am.title = Set(n.title);
            am.summary = Set(n.summary);
            am.content_html = Set(n.content_html);
            am.author = Set(n.author);
            am.published_at = Set(n.published_at.map(|d| d.with_timezone(&FixedOffset::east_opt(0).unwrap())));
            am.created_at = Set(now);
            am.updated_at = Set(now);
            am.hash = Set(None);
            am.is_read = Set(false);
            am.is_starred = Set(false);
            am.extras_json = Set(Some(n.extras));
            models.push(am);
        }
    }
    let mut inserted = 0usize;
    if !models.is_empty() {
        let _ = Entry::insert_many(models)
            .on_conflict(
                OnConflict::columns([entry::Column::FeedId, entry::Column::Guid])
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&st.db)
            .await
            .map_err(internal)?;
        inserted = guids.len() - existing.len();
    }
    if let Some(m) = meta {
        let mut fm: feed::ActiveModel = Feed::find_by_id(f.id).one(&st.db).await.map_err(internal)?.unwrap().into();
        fm.checked_at = Set(Some(now));
        fm.error_count = Set(0);
        fm.last_status = Set(m.last_status.map(|s| s as i32));
        fm.etag = Set(m.etag);
        fm.last_modified = Set(m.last_modified);
        let ok_secs: i64 = std::env::var("SCHEDULER_SUCCESS_INTERVAL_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(900);
        fm.next_run_at = Set(Some(now + chrono::Duration::seconds(ok_secs.max(60))));
        let _ = fm.update(&st.db).await.map_err(internal)?;
    }
    Ok(Json(MfRefreshResp { result: "OK".into(), inserted }))
}

// Export/Import (OPML wrapper)
async fn mf_export(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<(axum::http::HeaderMap, String)> {
    let auth = mf_auth(&st, &headers).await?;
    let cats = Category::find().filter(category::Column::UserId.eq(auth.user_id)).all(&st.db).await.map_err(internal)?;
    let feeds = Feed::find().filter(feed::Column::UserId.eq(auth.user_id)).all(&st.db).await.map_err(internal)?;
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n<head><title>Captura Export</title></head>\n<body>\n");
    for f in feeds.iter().filter(|f| f.category_id.is_none()) {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
            xml_escape(f.title.as_deref().unwrap_or("")),
            xml_escape(f.title.as_deref().unwrap_or("")),
            xml_escape(&f.feed_url),
            xml_escape(f.site_url.as_deref().unwrap_or(""))
        ));
    }
    for c in cats {
        buf.push_str(&format!("<outline text=\"{}\" title=\"{}\">\n", xml_escape(&c.name), xml_escape(&c.name)));
        for f in feeds.iter().filter(|f| f.category_id == Some(c.id)) {
            buf.push_str(&format!(
                "  <outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
                xml_escape(f.title.as_deref().unwrap_or("")),
                xml_escape(f.title.as_deref().unwrap_or("")),
                xml_escape(&f.feed_url),
                xml_escape(f.site_url.as_deref().unwrap_or(""))
            ));
        }
        buf.push_str("</outline>\n");
    }
    buf.push_str("</body>\n</opml>\n");
    let mut headers_out = axum::http::HeaderMap::new();
    headers_out.insert(axum::http::header::CONTENT_TYPE, axum::http::HeaderValue::from_static("application/xml; charset=utf-8"));
    Ok((headers_out, buf))
}

async fn mf_import(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> ApiResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let outlines = parse_opml_quickxml(&body).unwrap_or_else(|_| extract_outlines(&body));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut cat_map: std::collections::HashMap<String, i64> = Category::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|c| (c.name.clone(), c.id))
        .collect();
    for node in outlines {
        match node {
            OutlineNode::Feed { title, xml_url, html_url, category } => {
                let category_id = if let Some(cat) = category {
                    if let Some(id) = cat_map.get(&cat).copied() { Some(id) } else {
                        let am = category::ActiveModel { user_id: Set(auth.user_id), name: Set(cat.clone()), created_at: Set(now), ..Default::default() };
                        let c = am.insert(&st.db).await.map_err(internal)?;
                        cat_map.insert(cat, c.id);
                        Some(c.id)
                    }
                } else { None };
                let dup = Feed::find().filter(feed::Column::UserId.eq(auth.user_id)).filter(feed::Column::FeedUrl.eq(&xml_url)).one(&st.db).await.map_err(internal)?;
                if dup.is_some() { continue; }
                let am = feed::ActiveModel {
                    user_id: Set(auth.user_id),
                    category_id: Set(category_id),
                    r#type: Set(feed::FeedType::Rss),
                    title: Set(Some(title.unwrap_or_else(|| xml_url.clone()))),
                    site_url: Set(html_url),
                    feed_url: Set(xml_url),
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
                    favicon_id: Set(None),
                    ..Default::default()
                };
                let _ = am.insert(&st.db).await.map_err(internal)?;
            }
            OutlineNode::Category { .. } => {}
        }
    }
    Ok("OK")
}

#[derive(Deserialize)]
struct CreateFeedReq {
    category_id: Option<i64>,
    r#type: String,
    title: Option<String>,
    site_url: Option<String>,
    feed_url: String,
    rule_id: Option<i64>,
    user_agent: Option<String>,
    headers_json: Option<serde_json::Value>,
    cookies: Option<String>,
    proxy_url: Option<String>,
    fetch_via_proxy: Option<bool>,
    disable_http2: Option<bool>,
    allow_invalid_certs: Option<bool>,
    request_timeout_ms: Option<i32>,
    disabled: Option<bool>,
}

#[derive(Serialize)]
struct CreateFeedResp {
    id: i64,
}
#[derive(Serialize)]
struct IdResp {
    id: i64,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
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

type ApiResult<T> = std::result::Result<T, ApiError>;

async fn create_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateFeedReq>,
) -> ApiResult<Json<CreateFeedResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let ftype = match &body.r#type[..] {
        "rss" => feed::FeedType::Rss,
        "atom" => feed::FeedType::Atom,
        "json" => feed::FeedType::Json,
        "rule" => feed::FeedType::Rule,
        _ => return Err(bad_request("invalid feed type")),
    };
    if body.feed_url.trim().is_empty() || Url::parse(&body.feed_url).is_err() {
        return Err(bad_request("invalid feed_url"));
    }
    if let Some(t) = body.request_timeout_ms {
        if t < 0 {
            return Err(bad_request("request_timeout_ms must be positive"));
        }
    }
    if let Some(ref h) = body.headers_json {
        if !h.is_object() {
            return Err(bad_request("headers_json must be an object"));
        }
    }
    if let Some(cid) = body.category_id {
        assert_category_ownership(&st.db, user.user_id, cid).await?;
    }
    let am = feed::ActiveModel {
        user_id: Set(user.user_id),
        category_id: Set(body.category_id),
        r#type: Set(ftype),
        title: Set(body.title.clone()),
        site_url: Set(body.site_url.clone()),
        feed_url: Set(body.feed_url.clone()),
        rule_id: Set(body.rule_id),
        user_agent: Set(body.user_agent.clone()),
        headers_json: Set(body.headers_json),
        cookies: Set(body.cookies.clone()),
        proxy_url: Set(body.proxy_url.clone()),
        fetch_via_proxy: Set(body.fetch_via_proxy.unwrap_or(false)),
        disable_http2: Set(body.disable_http2.unwrap_or(false)),
        allow_invalid_certs: Set(body.allow_invalid_certs.unwrap_or(false)),
        request_timeout_ms: Set(body.request_timeout_ms),
        checked_at: Set(None),
        next_run_at: Set(None),
        etag: Set(None),
        last_modified: Set(None),
        last_status: Set(None),
        error_count: Set(0),
        disabled: Set(body.disabled.unwrap_or(false)),
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
    let res = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(CreateFeedResp { id: res.id }))
}

#[derive(Deserialize)]
struct CreateUserReq {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct CreateUserResp {
    id: i64,
}

async fn create_user(
    State(st): State<AppState>,
    Json(body): Json<CreateUserReq>,
) -> ApiResult<Json<CreateUserResp>> {
    // 仅允许首次用户创建匿名，无用户时不验证；否则需要 token（省略）
    let count = User::find().count(&st.db).await.map_err(internal)?;
    if count > 0 {
        return Err(forbidden("user exists"));
    }
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(body.password.as_bytes(), &salt)
        .map_err(internal)?
        .to_string();
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = user::ActiveModel {
        username: Set(body.username),
        password_hash: Set(hash),
        created_at: Set(now),
        ..Default::default()
    };
    let u = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(CreateUserResp { id: u.id }))
}

#[derive(Deserialize)]
struct AuthLoginReq {
    username: String,
    password: String,
    name: Option<String>,
}

#[derive(Serialize)]
struct AuthLoginResp {
    token: String,
}

async fn auth_login(
    State(st): State<AppState>,
    Json(body): Json<AuthLoginReq>,
) -> ApiResult<Json<AuthLoginResp>> {
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(bad_request("username/password required"));
    }
    let u = User::find()
        .filter(user::Column::Username.eq(&body.username))
        .one(&st.db)
        .await
        .map_err(internal)?;
    let Some(u) = u else {
        return Err(unauthorized("invalid credentials"));
    };
    let parsed = PasswordHash::new(&u.password_hash).map_err(internal)?;
    Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed)
        .map_err(|_| unauthorized("invalid credentials"))?;
    // 颁发简单随机 token（生产应更强）：hash(username+time)
    let mut rand_bytes = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(u.id),
        name: Set(body.name),
        token_hash: Set(token_hash),
        created_at: Set(now),
        last_used_at: Set(Some(now)),
        expires_at: Set(Some(now + chrono::Duration::seconds({
            let v: i64 = std::env::var("CAPTURA_TOKEN_TTL_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(30 * 24 * 3600);
            v.max(3600)
        }))),
        ..Default::default()
    };
    let _ = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(AuthLoginResp { token: token_str }))
}

#[derive(Clone)]
struct AuthUser {
    user_id: i64,
}

impl AuthUser {
    async fn from_bearer(db: &DatabaseConnection, bearer: &str) -> ApiResult<Self> {
        let hash = format!("{:x}", Sha256::digest(bearer.as_bytes()));
        let tok = Token::find()
            .filter(token::Column::TokenHash.eq(hash))
            .one(db)
            .await
            .map_err(internal)?;
        let Some(tok) = tok else {
            return Err(unauthorized("invalid token"));
        };
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        if let Some(exp) = tok.expires_at {
            if exp <= now {
                return Err(unauthorized("token expired"));
            }
        }
        // Update last_used_at best-effort
        let mut am: token::ActiveModel = tok.clone().into();
        am.last_used_at = Set(Some(now));
        let _ = am.update(db).await;
        Ok(Self {
            user_id: tok.user_id,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StatusFilter {
    Read,
    Unread,
    Starred,
}

#[derive(Deserialize)]
struct EntriesQuery {
    feed_id: Option<i64>,
    category_id: Option<i64>,
    status: Option<StatusFilter>,
    limit: Option<u64>,
    offset: Option<u64>,
    q: Option<String>,
    sort_by: Option<String>,
    order: Option<String>,
}

#[derive(Serialize)]
struct EntryDto {
    id: i64,
    feed_id: i64,
    url: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    content_html: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    is_read: bool,
    is_starred: bool,
}

async fn list_entries(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<EntriesQuery>,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    validate_sort(&q.sort_by, &["published_at", "created_at"], &q.order)?;
    if let Some(ref s) = q.q {
        if s.len() > 256 {
            return Err(bad_request("q too long"));
        }
    }
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(fid) = q.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = q.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(sts) = &q.status {
        match sts {
            StatusFilter::Read => sel = sel.filter(entry::Column::IsRead.eq(true)),
            StatusFilter::Unread => sel = sel.filter(entry::Column::IsRead.eq(false)),
            StatusFilter::Starred => sel = sel.filter(entry::Column::IsStarred.eq(true)),
        }
    }
    // search
    if let Some(ref q) = q.q {
        let like = format!("%{}%", q);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    // sorting
    match q.sort_by.as_deref() {
        Some("created_at") => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::CreatedAt),
                _ => sel.order_by_desc(entry::Column::CreatedAt),
            };
        }
        _ => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::PublishedAt),
                _ => sel.order_by_desc(entry::Column::PublishedAt),
            };
            sel = sel.order_by_desc(entry::Column::CreatedAt);
        }
    }
    if let Some(l) = q.limit {
        sel = sel.limit(l);
    }
    if let Some(o) = q.offset {
        sel = sel.offset(o);
    }
    let list = sel.all(&st.db).await.map_err(internal)?;
    let out = list
        .into_iter()
        .map(|e| EntryDto {
            id: e.id,
            feed_id: e.feed_id,
            url: e.url,
            title: e.title,
            summary: e.summary,
            content_html: e.content_html,
            author: e.author,
            published_at: e.published_at.map(|d| d.to_rfc3339()),
            is_read: e.is_read,
            is_starred: e.is_starred,
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
struct BoolBody {
    value: bool,
}

async fn mark_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<BoolBody>,
) -> ApiResult<&'static str> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? {
        let mut am: entry::ActiveModel = e.into();
        am.is_read = Set(body.value);
        am.update(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

async fn mark_star(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<BoolBody>,
) -> ApiResult<&'static str> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? {
        let mut am: entry::ActiveModel = e.into();
        am.is_starred = Set(body.value);
        am.update(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

// ----- Extended feeds & categories -----

#[derive(Deserialize)]
struct FeedsQuery {
    category_id: Option<i64>,
    disabled: Option<bool>,
    has_errors: Option<bool>,
    sort_by: Option<String>,
    order: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Serialize)]
struct FeedDto {
    id: i64,
    title: Option<String>,
    feed_url: String,
    site_url: Option<String>,
    disabled: bool,
    category_id: Option<i64>,
}

async fn list_feeds(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<FeedsQuery>,
) -> ApiResult<Json<Vec<FeedDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    validate_sort(
        &q.sort_by,
        &["updated_at", "created_at", "error_count", "title"],
        &q.order,
    )?;
    let mut sel = Feed::find().filter(feed::Column::UserId.eq(user.user_id));
    if let Some(cid) = q.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(d) = q.disabled {
        sel = sel.filter(feed::Column::Disabled.eq(d));
    }
    if let Some(e) = q.has_errors {
        if e {
            sel = sel.filter(feed::Column::ErrorCount.gt(0));
        } else {
            sel = sel.filter(feed::Column::ErrorCount.eq(0));
        }
    }
    // sorting
    match q.sort_by.as_deref() {
        Some("created_at") => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::CreatedAt),
                _ => sel.order_by_desc(feed::Column::CreatedAt),
            }
        }
        Some("updated_at") => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::UpdatedAt),
                _ => sel.order_by_desc(feed::Column::UpdatedAt),
            }
        }
        Some("error_count") => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::ErrorCount),
                _ => sel.order_by_desc(feed::Column::ErrorCount),
            }
        }
        Some("title") => {
            sel = match q.order.as_deref() {
                Some("desc") => sel.order_by_desc(feed::Column::Title),
                _ => sel.order_by_asc(feed::Column::Title),
            }
        }
        _ => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::UpdatedAt),
                _ => sel.order_by_desc(feed::Column::UpdatedAt),
            }
        }
    }
    if let Some(l) = q.limit {
        sel = sel.limit(l);
    }
    if let Some(o) = q.offset {
        sel = sel.offset(o);
    }
    let list = sel.all(&st.db).await.map_err(internal)?;
    let out = list
        .into_iter()
        .map(|f| FeedDto {
            id: f.id,
            title: f.title,
            feed_url: f.feed_url,
            site_url: f.site_url,
            disabled: f.disabled,
            category_id: f.category_id,
        })
        .collect();
    Ok(Json(out))
}

async fn get_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FeedDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    Ok(Json(FeedDto {
        id: f.id,
        title: f.title,
        feed_url: f.feed_url,
        site_url: f.site_url,
        disabled: f.disabled,
        category_id: f.category_id,
    }))
}

#[derive(Deserialize, Default)]
struct UpdateFeedReq {
    title: Option<String>,
    category_id: Option<i64>,
    disabled: Option<bool>,
    user_agent: Option<String>,
    headers_json: Option<serde_json::Value>,
    cookies: Option<String>,
    proxy_url: Option<String>,
    fetch_via_proxy: Option<bool>,
    disable_http2: Option<bool>,
    allow_invalid_certs: Option<bool>,
    request_timeout_ms: Option<i32>,
}

async fn update_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateFeedReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    let mut am: feed::ActiveModel = f.into();
    if body.title.is_some() {
        am.title = Set(body.title);
    }
    if let Some(cid) = body.category_id {
        assert_category_ownership(&st.db, user.user_id, cid).await?;
        am.category_id = Set(Some(cid));
    }
    if let Some(v) = body.disabled {
        am.disabled = Set(v);
    }
    if body.user_agent.is_some() {
        am.user_agent = Set(body.user_agent);
    }
    if let Some(ref h) = body.headers_json {
        if !h.is_object() {
            return Err(bad_request("headers_json must be an object"));
        }
        am.headers_json = Set(body.headers_json);
    }
    if body.cookies.is_some() {
        am.cookies = Set(body.cookies);
    }
    if body.proxy_url.is_some() {
        am.proxy_url = Set(body.proxy_url);
    }
    if let Some(v) = body.fetch_via_proxy {
        am.fetch_via_proxy = Set(v);
    }
    if let Some(v) = body.disable_http2 {
        am.disable_http2 = Set(v);
    }
    if let Some(v) = body.allow_invalid_certs {
        am.allow_invalid_certs = Set(v);
    }
    if body.request_timeout_ms.is_some() {
        am.request_timeout_ms = Set(body.request_timeout_ms);
    }
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn delete_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    let am: feed::ActiveModel = f.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(Deserialize)]
struct ExtendedEntriesQuery {
    feed_id: Option<i64>,
    category_id: Option<i64>,
    status: Option<StatusFilter>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[allow(dead_code)]
async fn _list_entries_extended(
    db: &DatabaseConnection,
    user_id: i64,
    q: &ExtendedEntriesQuery,
) -> Result<Vec<entry::Model>, sea_orm::DbErr> {
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user_id));
    if let Some(fid) = q.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = q.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(sts) = &q.status {
        match sts {
            StatusFilter::Read => sel = sel.filter(entry::Column::IsRead.eq(true)),
            StatusFilter::Unread => sel = sel.filter(entry::Column::IsRead.eq(false)),
            StatusFilter::Starred => sel = sel.filter(entry::Column::IsStarred.eq(true)),
        }
    }
    if let Some(l) = q.limit {
        sel = sel.limit(l);
    }
    if let Some(o) = q.offset {
        sel = sel.offset(o);
    }
    sel.order_by(entry::Column::PublishedAt, Order::Desc)
        .order_by(entry::Column::CreatedAt, Order::Desc)
        .all(db)
        .await
}

#[derive(Deserialize)]
struct MarkAllReq {
    feed_id: Option<i64>,
    category_id: Option<i64>,
}

async fn mark_all_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<MarkAllReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if body.feed_id.is_none() && body.category_id.is_none() {
        return Err(bad_request("feed_id or category_id required"));
    }
    // Find target entries then update
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(fid) = body.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = body.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    let ids: Vec<i64> = sel
        .select_only()
        .column(entry::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !ids.is_empty() {
        entry::Entity::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .filter(entry::Column::Id.is_in(ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}

// Categories
#[derive(Serialize)]
struct CategoryDto {
    id: i64,
    name: String,
}

async fn list_categories(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<CategoryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .order_by_asc(category::Column::Id)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|c| CategoryDto {
                id: c.id,
                name: c.name,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct CreateCategoryReq {
    name: String,
}

async fn create_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateCategoryReq>,
) -> ApiResult<Json<IdResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(bad_request("invalid category name"));
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = category::ActiveModel {
        user_id: Set(user.user_id),
        name: Set(name.to_string()),
        created_at: Set(now),
        ..Default::default()
    };
    let c = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(IdResp { id: c.id }))
}

#[derive(Deserialize)]
struct UpdateCategoryReq {
    name: String,
}

async fn get_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<CategoryDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(c) = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .filter(category::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category not found"));
    };
    Ok(Json(CategoryDto {
        id: c.id,
        name: c.name,
    }))
}

async fn update_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCategoryReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(c) = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .filter(category::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category not found"));
    };
    let mut am: category::ActiveModel = c.into();
    let name = body.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(bad_request("invalid category name"));
    }
    am.name = Set(name.to_string());
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn delete_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(c) = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .filter(category::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category not found"));
    };
    let am: category::ActiveModel = c.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

// OPML export/import
async fn opml_export(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<(axum::http::HeaderMap, String)> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n<head><title>Captura Export</title></head>\n<body>\n");
    for f in feeds.iter().filter(|f| f.category_id.is_none()) {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
            xml_escape(f.title.as_deref().unwrap_or("")),
            xml_escape(f.title.as_deref().unwrap_or("")),
            xml_escape(&f.feed_url),
            xml_escape(f.site_url.as_deref().unwrap_or(""))
        ));
    }
    for c in cats {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\">\n",
            xml_escape(&c.name),
            xml_escape(&c.name)
        ));
        for f in feeds.iter().filter(|f| f.category_id == Some(c.id)) {
            buf.push_str(&format!(
                "  <outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
                xml_escape(f.title.as_deref().unwrap_or("")),
                xml_escape(f.title.as_deref().unwrap_or("")),
                xml_escape(&f.feed_url),
                xml_escape(f.site_url.as_deref().unwrap_or(""))
            ));
        }
        buf.push_str("</outline>\n");
    }
    buf.push_str("</body>\n</opml>\n");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    Ok((headers, buf))
}

async fn opml_import(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    body: String,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let outlines = parse_opml_quickxml(&body).unwrap_or_else(|_| extract_outlines(&body));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut cat_map: std::collections::HashMap<String, i64> = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|c| (c.name.clone(), c.id))
        .collect();
    for node in outlines {
        match node {
            OutlineNode::Feed {
                title,
                xml_url,
                html_url,
                category,
            } => {
                let category_id = if let Some(cat) = category {
                    if let Some(id) = cat_map.get(&cat).copied() {
                        Some(id)
                    } else {
                        let am = category::ActiveModel {
                            user_id: Set(user.user_id),
                            name: Set(cat.clone()),
                            created_at: Set(now),
                            ..Default::default()
                        };
                        let c = am.insert(&st.db).await.map_err(internal)?;
                        cat_map.insert(cat, c.id);
                        Some(c.id)
                    }
                } else {
                    None
                };
                let dup = Feed::find()
                    .filter(feed::Column::UserId.eq(user.user_id))
                    .filter(feed::Column::FeedUrl.eq(&xml_url))
                    .one(&st.db)
                    .await
                    .map_err(internal)?;
                if dup.is_some() {
                    continue;
                }
                let am = feed::ActiveModel {
                    user_id: Set(user.user_id),
                    category_id: Set(category_id),
                    r#type: Set(feed::FeedType::Rss),
                    title: Set(Some(title.unwrap_or_else(|| xml_url.clone()))),
                    site_url: Set(html_url),
                    feed_url: Set(xml_url),
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
                let _ = am.insert(&st.db).await.map_err(internal)?;
            }
            OutlineNode::Category { .. } => {}
        }
    }
    Ok("ok")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[allow(dead_code)]
enum OutlineNode {
    Category {
        title: String,
    },
    Feed {
        title: Option<String>,
        xml_url: String,
        html_url: Option<String>,
        category: Option<String>,
    },
}

fn extract_outlines(body: &str) -> Vec<OutlineNode> {
    let mut current_category: Option<String> = None;
    let mut nodes = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with("<outline") && t.ends_with("/>") {
            let title = attr_value(t, "title");
            let xml_url = attr_value(t, "xmlUrl");
            if let Some(xu) = xml_url {
                nodes.push(OutlineNode::Feed {
                    title,
                    xml_url: xu,
                    html_url: attr_value(t, "htmlUrl"),
                    category: current_category.clone(),
                });
            } else if let Some(tt) = title {
                nodes.push(OutlineNode::Category { title: tt });
                current_category = None;
            }
        } else if t.starts_with("<outline") {
            if let Some(tt) = attr_value(t, "title") {
                nodes.push(OutlineNode::Category { title: tt.clone() });
                current_category = Some(tt);
            }
        } else if t.starts_with("</outline>") {
            current_category = None;
        }
    }
    nodes
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{}=\"", name);
    if let Some(i) = tag.find(&pat) {
        let rest = &tag[i + pat.len()..];
        if let Some(j) = rest.find('"') {
            return Some(rest[..j].to_string());
        }
    }
    None
}

fn parse_opml_quickxml(body: &str) -> Result<Vec<OutlineNode>, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_str(body);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut nodes = Vec::new();
    let mut cat_stack: Vec<String> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"outline" {
                    let mut title = None;
                    let mut text = None;
                    let mut xml_url = None;
                    let mut html_url = None;
                    for a in e.attributes().flatten() {
                        let key = a.key.as_ref();
                        let val = a
                            .decode_and_unescape_value(&reader)
                            .map_err(|e| e.to_string())?
                            .to_string();
                        match key {
                            b"title" => title = Some(val.clone()),
                            b"text" => text = Some(val.clone()),
                            b"xmlUrl" => xml_url = Some(val.clone()),
                            b"htmlUrl" => html_url = Some(val.clone()),
                            _ => {}
                        }
                    }
                    if let Some(xu) = xml_url {
                        let name = title.or(text);
                        let cat = cat_stack.last().cloned();
                        nodes.push(OutlineNode::Feed {
                            title: name,
                            xml_url: xu,
                            html_url,
                            category: cat,
                        });
                    } else {
                        let name = title.or(text).unwrap_or_default();
                        nodes.push(OutlineNode::Category {
                            title: name.clone(),
                        });
                        if matches!(
                            reader.read_event_into(&mut Vec::new()),
                            Ok(Event::Start(_))
                                | Ok(Event::Empty(_))
                                | Ok(Event::Text(_))
                                | Ok(Event::CData(_))
                                | Ok(Event::Comment(_))
                                | Ok(Event::Decl(_))
                                | Ok(Event::PI(_))
                                | Ok(Event::DocType(_))
                                | Ok(Event::Eof)
                        ) {
                            // Push only on non-empty category; this branch is a placeholder to keep simple
                        }
                        cat_stack.push(name);
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"outline" {
                    // pop category if stack not empty
                    let _ = cat_stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(nodes)
}

// ---------- Rules ----------

#[derive(Serialize)]
struct RuleDto {
    id: i64,
    rule_id: String,
    namespace: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct CreateRuleReq {
    yaml: String,
    version: Option<String>,
    maintainer: Option<String>,
}

fn rule_namespace(id: &str) -> Option<String> {
    id.rsplit_once('.').map(|(ns, _)| ns.to_string())
}

async fn create_rule(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateRuleReq>,
) -> ApiResult<Json<IdResp>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let spec: RuleSpec = parse_rule(&body.yaml)
        .map_err(|e| bad_request(format!("invalid rule yaml: {}", e.to_string())))?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let examples = serde_json::to_value(&spec.examples).map_err(internal)?;
    let am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        version: Set(body.version.clone()),
        namespace: Set(rule_namespace(&spec.id)),
        description: Set(spec.description.clone()),
        yaml: Set(body.yaml.clone()),
        examples_json: Set(Some(examples)),
        verified_at: Set(Some(now)),
        maintainer: Set(body.maintainer.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let rec = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(IdResp { id: rec.id }))
}

#[derive(Deserialize)]
struct RulesQuery {
    q: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn list_rules(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<RulesQuery>,
) -> ApiResult<Json<Vec<RuleDto>>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    let mut sel = Rule::find();
    if let Some(ref s) = q.q {
        let like = format!("%{}%", s);
        sel = sel.filter(
            Condition::any()
                .add(rule::Column::RuleId.like(like.as_str()))
                .add(rule::Column::Description.like(like.as_str())),
        );
    }
    if let Some(l) = q.limit {
        sel = sel.limit(l);
    }
    if let Some(o) = q.offset {
        sel = sel.offset(o);
    }
    let list = sel
        .order_by_desc(rule::Column::UpdatedAt)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|r| RuleDto {
                id: r.id,
                rule_id: r.rule_id,
                namespace: r.namespace,
                version: r.version,
                description: r.description,
            })
            .collect(),
    ))
}

async fn get_rule(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RuleDto>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(r) = Rule::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("rule not found"));
    };
    Ok(Json(RuleDto {
        id: r.id,
        rule_id: r.rule_id,
        namespace: r.namespace,
        version: r.version,
        description: r.description,
    }))
}

async fn update_rule(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<CreateRuleReq>,
) -> ApiResult<&'static str> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(r) = Rule::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("rule not found"));
    };
    let spec: RuleSpec = parse_rule(&body.yaml)
        .map_err(|e| bad_request(format!("invalid rule yaml: {}", e.to_string())))?;
    let examples = serde_json::to_value(&spec.examples).map_err(internal)?;
    let mut am: rule::ActiveModel = r.into();
    am.rule_id = Set(spec.id.clone());
    am.version = Set(body.version.clone());
    am.namespace = Set(rule_namespace(&spec.id));
    am.description = Set(spec.description.clone());
    am.yaml = Set(body.yaml.clone());
    am.examples_json = Set(Some(examples));
    am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
    am.maintainer = Set(body.maintainer.clone());
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn delete_rule(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    // prevent delete when feeds reference the rule
    let used = Feed::find()
        .filter(feed::Column::RuleId.eq(id))
        .count(&st.db)
        .await
        .map_err(internal)?;
    if used > 0 {
        return Err(forbidden("rule is in use by feeds"));
    }
    let Some(r) = Rule::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("rule not found"));
    };
    let am: rule::ActiveModel = r.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

// ---------- Fever (read-only subset) ----------

#[derive(Deserialize)]
struct FeverQuery {
    api: Option<i32>,
    api_key: Option<String>,
    groups: Option<i32>,
    feeds: Option<i32>,
    favicons: Option<i32>,
    items: Option<i32>,
    since_id: Option<i64>,
    limit: Option<u64>,
    unread_item_ids: Option<i32>,
    saved_item_ids: Option<i32>,
    // write actions
    mark: Option<String>, // item|feed|group|all
    #[serde(rename = "as")]
    r#as: Option<String>, // read|unread|saved|unsaved
    id: Option<String>,   // item ids (csv) or feed/group id
    before: Option<i64>,  // timestamp
}

#[derive(Serialize)]
struct FeverBase {
    api_version: i32,
    auth: i32,
    last_refreshed_on_time: i64,
}

async fn fever_endpoint(State(st): State<AppState>, Query(q): Query<FeverQuery>) -> Response {
    // Authenticate via api_key MD5(username:api_password) stored as user.fever_key_md5
    let mut base = FeverBase {
        api_version: 3,
        auth: 0,
        last_refreshed_on_time: Utc::now().timestamp(),
    };
    let Some(ref api_key) = q.api_key else {
        return Json(base).into_response();
    };
    let user = User::find()
        .filter(user::Column::FeverKeyMd5.eq(api_key))
        .one(&st.db)
        .await;
    let Ok(Some(user)) = user else {
        return Json(base).into_response();
    };
    base.auth = 1;

    // Respond to probes
    if q.api.unwrap_or(0) == 1
        && q.groups.is_none()
        && q.feeds.is_none()
        && q.items.is_none()
        && q.unread_item_ids.is_none()
        && q.saved_item_ids.is_none()
    {
        return Json(base).into_response();
    }

    // First, handle write actions if present (best-effort)
    if let Some(ref mark) = q.mark {
        let _ = fever_apply_write(&st.db, user.id, mark, q.r#as.as_deref(), q.id.as_deref(), q.before).await;
    }

    // Accumulate response map
    use serde_json::json;
    let mut resp = json!({
        "api_version": base.api_version,
        "auth": base.auth,
        "last_refreshed_on_time": base.last_refreshed_on_time,
    });

    if q.groups.unwrap_or(0) == 1 {
        let cats = Category::find()
            .filter(category::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let groups: Vec<_> = cats
            .iter()
            .map(|c| json!({"id": c.id, "title": c.name}))
            .collect();
        resp["groups"] = json!(groups);
        // feeds_groups mapping (flat: category->feed ids)
        let feeds = Feed::find()
            .filter(feed::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let mut map: Vec<serde_json::Value> = Vec::new();
        for c in &cats {
            let ids: Vec<i64> = feeds
                .iter()
                .filter(|f| f.category_id == Some(c.id))
                .map(|f| f.id)
                .collect();
            if !ids.is_empty() {
                map.push(json!({"group_id": c.id, "feed_ids": ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")}));
            }
        }
        resp["feeds_groups"] = json!(map);
    }

    if q.feeds.unwrap_or(0) == 1 {
        let feeds = Feed::find()
            .filter(feed::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let feeds_json: Vec<_> = feeds
            .iter()
            .map(|f| {
                json!({
                    "id": f.id,
                    "favicon_id": f.favicon_id.unwrap_or(0),
                    "title": f.title,
                    "url": f.feed_url,
                    "site_url": f.site_url,
                    "group_id": f.category_id.unwrap_or(0),
                })
            })
            .collect();
        resp["feeds"] = json!(feeds_json);
    }

    if q.items.unwrap_or(0) == 1 {
        let mut sel = Entry::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.id));
        if let Some(since) = q.since_id {
            sel = sel.filter(entry::Column::Id.gt(since));
        }
        let lim = q.limit.unwrap_or(50).min(200) as u64;
        let items = sel
            .order_by_asc(entry::Column::Id)
            .limit(lim)
            .all(&st.db)
            .await
            .unwrap_or_default();
        let json_items: Vec<_> = items.iter().map(|e| json!({
            "id": e.id,
            "feed_id": e.feed_id,
            "title": e.title,
            "author": e.author,
            "html": e.content_html,
            "url": e.url,
            "is_saved": if e.is_starred {1} else {0},
            "is_read": if e.is_read {1} else {0},
            "created_on_time": e.published_at.map(|d| d.timestamp()).unwrap_or_else(|| e.created_at.timestamp()),
        })).collect();
        resp["items"] = json!(json_items);
        resp["total_items"] = json!(json_items.len());
    }

    if q.favicons.unwrap_or(0) == 1 {
        use serde_json::json;
        let feeds = Feed::find()
            .filter(feed::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let feed_ids: Vec<i64> = feeds.iter().map(|f| f.id).collect();
        // try join to favicon by feed_id; if none, skip (client can fallback)
        let favs = Favicon::find()
            .filter(captura_storage::entity::favicon::Column::FeedId.is_in(feed_ids))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let list: Vec<_> = favs
            .iter()
            .filter_map(|fv| {
                fv.data.as_ref().map(|bytes| {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                    json!({ "id": fv.id, "data": b64 })
                })
            })
            .collect();
        resp["favicons"] = json!(list);
    }

    if q.unread_item_ids.unwrap_or(0) == 1 {
        let ids: Vec<i64> = Entry::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.id))
            .filter(entry::Column::IsRead.eq(false))
            .select_only()
            .column(entry::Column::Id)
            .into_tuple()
            .all(&st.db)
            .await
            .unwrap_or_default();
        resp["unread_item_ids"] = json!(ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(","));
    }

    if q.saved_item_ids.unwrap_or(0) == 1 {
        let ids: Vec<i64> = Entry::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.id))
            .filter(entry::Column::IsStarred.eq(true))
            .select_only()
            .column(entry::Column::Id)
            .into_tuple()
            .all(&st.db)
            .await
            .unwrap_or_default();
        resp["saved_item_ids"] = json!(ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(","));
    }

    Json(resp).into_response()
}

async fn fever_apply_write(
    db: &DatabaseConnection,
    user_id: i64,
    mark: &str,
    asv: Option<&str>,
    id: Option<&str>,
    before: Option<i64>,
) -> Result<(), DbErr> {
    use sea_orm::{ActiveValue::Set, Condition, QueryOrder};
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let asv = asv.unwrap_or("");
    match mark {
        "item" => {
            let ids: Vec<i64> = id
                .unwrap_or("")
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect();
            if ids.is_empty() { return Ok(()); }
            // filter to only entries belonging to user
            let feed_ids: Vec<i64> = Feed::find()
                .filter(feed::Column::UserId.eq(user_id))
                .select_only()
                .column(feed::Column::Id)
                .into_tuple()
                .all(db)
                .await?;
            match asv {
                "read" | "unread" => {
                    let val = asv == "read";
                    let _ = Entry::update_many()
                        .col_expr(entry::Column::IsRead, Expr::value(val))
                        .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                        .filter(entry::Column::Id.is_in(ids))
                        .filter(entry::Column::FeedId.is_in(feed_ids))
                        .exec(db)
                        .await?;
                }
                "saved" | "unsaved" => {
                    let val = asv == "saved";
                    let _ = Entry::update_many()
                        .col_expr(entry::Column::IsStarred, Expr::value(val))
                        .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                        .filter(entry::Column::Id.is_in(ids))
                        .filter(entry::Column::FeedId.is_in(feed_ids))
                        .exec(db)
                        .await?;
                }
                _ => {}
            }
        }
        "feed" => {
            let fid: i64 = id.and_then(|s| s.parse().ok()).unwrap_or_default();
            if fid == 0 { return Ok(()); }
            if asv == "read" {
                let cond = Condition::all()
                    .add(entry::Column::FeedId.eq(fid))
                    .add(entry::Column::IsRead.eq(false));
                let cond = if let Some(ts) = before { cond.add(entry::Column::CreatedAt.lte(chrono::DateTime::from_timestamp(ts,0).unwrap().with_timezone(&FixedOffset::east_opt(0).unwrap()))) } else { cond };
                // ensure the feed belongs to user
                if Feed::find_by_id(fid).filter(feed::Column::UserId.eq(user_id)).one(db).await?.is_some() {
                    let _ = Entry::update_many()
                        .col_expr(entry::Column::IsRead, Expr::value(true))
                        .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                        .filter(cond)
                        .exec(db)
                        .await?;
                }
            }
        }
        "group" => {
            let gid: i64 = id.and_then(|s| s.parse().ok()).unwrap_or_default();
            if gid == 0 { return Ok(()); }
            if asv == "read" {
                // all feeds in this user's category
                let feeds: Vec<i64> = Feed::find()
                    .filter(feed::Column::UserId.eq(user_id))
                    .filter(feed::Column::CategoryId.eq(gid))
                    .select_only()
                    .column(feed::Column::Id)
                    .into_tuple()
                    .all(db)
                    .await?;
                if !feeds.is_empty() {
                    let mut cond = Condition::all()
                        .add(entry::Column::FeedId.is_in(feeds))
                        .add(entry::Column::IsRead.eq(false));
                    if let Some(ts) = before {
                        if let Some(dt) = chrono::DateTime::from_timestamp(ts,0) {
                            cond = cond.add(entry::Column::CreatedAt.lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())));
                        }
                    }
                    let _ = Entry::update_many()
                        .col_expr(entry::Column::IsRead, Expr::value(true))
                        .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                        .filter(cond)
                        .exec(db)
                        .await?;
                }
            }
        }
        "all" => {
            if asv == "read" {
                let mut cond = Condition::all()
                    .add(entry::Column::IsRead.eq(false));
                if let Some(ts) = before {
                    if let Some(dt) = chrono::DateTime::from_timestamp(ts,0) {
                        cond = cond.add(entry::Column::CreatedAt.lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())));
                    }
                }
                let _ = Entry::update_many()
                    .col_expr(entry::Column::IsRead, Expr::value(true))
                    .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                    .filter(cond)
                    .exec(db)
                    .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Deserialize)]
struct SetFeverKeyReq {
    api_password: String,
}

// 设置/更新 Fever API 密钥（md5(username:api_password) 小写十六进制）
async fn set_fever_key(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(req): Json<SetFeverKeyReq>,
) -> ApiResult<&'static str> {
    let auth = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if auth.user_id != id {
        return Err(forbidden("cannot set fever key for other user"));
    }
    if req.api_password.trim().is_empty() {
        return Err(bad_request("api_password required"));
    }
    let Some(u) = User::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("user not found"));
    };
    let s = format!("{}:{}", u.username, req.api_password);
    let key = format!("{:x}", Md5::digest(s.as_bytes()));
    let mut am: user::ActiveModel = u.into();
    am.fever_key_md5 = Set(Some(key));
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(Serialize)]
struct FaviconResp {
    favicon_id: i64,
    updated: bool,
}

#[cfg(not(test))]
async fn fetch_favicon(site: &str) -> anyhow::Result<(Vec<u8>, Option<String>, String)> {
    let mut base = Url::parse(site)?;
    base.set_path("/favicon.ico");
    base.set_query(None);
    base.set_fragment(None);
    let cli = reqwest::Client::builder()
        .user_agent("captura/0.1")
        .build()?;
    let res = cli.get(base.as_str()).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("http status {}", res.status());
    }
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = res.bytes().await?.to_vec();
    Ok((bytes, mime, base.to_string()))
}

#[cfg(test)]
static TEST_FAVICON_RESP: OnceCell<(Vec<u8>, Option<String>)> = OnceCell::new();

#[cfg(test)]
async fn fetch_favicon(_site: &str) -> anyhow::Result<(Vec<u8>, Option<String>, String)> {
    let (bytes, mime) = TEST_FAVICON_RESP
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no test favicon set"))?;
    Ok((bytes, mime, "test://favicon.ico".to_string()))
}

// 刷新某个 Feed 的 favicon（简单策略：尝试 site_url + /favicon.ico）
async fn refresh_favicon(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FaviconResp>> {
    use captura_storage::entity::favicon as fv;
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::Id.eq(id))
        .filter(feed::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    let site = match f.site_url.clone().or_else(|| Some(f.feed_url.clone())) {
        Some(s) => s,
        None => return Err(bad_request("no site_url/feed_url")),
    };
    let (bytes, mime, actual_url) = fetch_favicon(&site)
        .await
        .map_err(|_| not_found("favicon not found"))?;

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = fv::ActiveModel {
        feed_id: Set(Some(f.id)),
        url: Set(Some(actual_url)),
        mime: Set(mime),
        data: Set(Some(bytes)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let fav = am.insert(&st.db).await.map_err(internal)?;
    let mut fm: feed::ActiveModel = f.into();
    fm.favicon_id = Set(Some(fav.id));
    fm.update(&st.db).await.map_err(internal)?;
    Ok(Json(FaviconResp {
        favicon_id: fav.id,
        updated: true,
    }))
}

// 直接返回 favicon 二进制
async fn get_favicon(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    use captura_storage::entity::favicon as fv;
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = fv::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("favicon not found"));
    };
    use axum::body::Bytes;
    let body = Bytes::from(f.data.unwrap_or_default());
    let mut resp = Response::new(body.into());
    if let Some(ct) = f.mime {
        resp.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_str(&ct)
                .unwrap_or(axum::http::HeaderValue::from_static("image/x-icon")),
        );
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum_extra::typed_header::TypedHeader;
    use headers::authorization::Bearer;
    use headers::Authorization;
    use http_body_util::BodyExt;
    use md5::Md5;
    use serde_json::Value;

    async fn setup_db() -> DatabaseConnection {
        let db = captura_storage::connect("sqlite::memory:").await.unwrap();
        create_min_schema_sqlite(&db).await;
        db
    }

    async fn create_min_schema_sqlite(db: &DatabaseConnection) {
        use sea_orm::ConnectionTrait;
        // user
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS user (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              username TEXT NOT NULL UNIQUE,
              password_hash TEXT NOT NULL,
              fever_key_md5 TEXT,
              created_at TEXT NOT NULL
            );
        "#,
        )
        .await
        .unwrap();

        // api_token
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS api_token (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              user_id INTEGER NOT NULL,
              name TEXT,
              token_hash TEXT NOT NULL,
              created_at TEXT NOT NULL,
              last_used_at TEXT
            );
        "#,
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_api_token_user ON api_token(user_id);",
        )
        .await
        .unwrap();

        // category
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS category (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              user_id INTEGER NOT NULL,
              name TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
        "#,
        )
        .await
        .unwrap();

        // feed
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS feed (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              user_id INTEGER NOT NULL,
              category_id INTEGER,
              type TEXT NOT NULL,
              title TEXT,
              site_url TEXT,
              feed_url TEXT NOT NULL,
              favicon_id INTEGER,
              rule_id INTEGER,
              user_agent TEXT,
              headers_json TEXT,
              cookies TEXT,
              proxy_url TEXT,
              fetch_via_proxy INTEGER NOT NULL DEFAULT 0,
              disable_http2 INTEGER NOT NULL DEFAULT 0,
              allow_invalid_certs INTEGER NOT NULL DEFAULT 0,
              request_timeout_ms INTEGER,
              checked_at TEXT,
              next_run_at TEXT,
              etag TEXT,
              last_modified TEXT,
              last_status INTEGER,
              error_count INTEGER NOT NULL DEFAULT 0,
              disabled INTEGER NOT NULL DEFAULT 0,
              scraper_rules TEXT,
              rewrite_rules TEXT,
              blocklist_rules TEXT,
              keeplist_rules TEXT,
              url_rewrite_rules TEXT,
              block_filter_entry_rules TEXT,
              keep_filter_entry_rules TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
        "#,
        )
        .await
        .unwrap();

        // entry
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS entry (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              feed_id INTEGER NOT NULL,
              guid TEXT,
              url TEXT,
              title TEXT,
              summary TEXT,
              content_html TEXT,
              author TEXT,
              published_at TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              hash TEXT,
              is_read INTEGER NOT NULL DEFAULT 0,
              is_starred INTEGER NOT NULL DEFAULT 0,
              extras_json TEXT
            );
        "#,
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_entry_feed_guid ON entry(feed_id, guid);",
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_entry_feed_published ON entry(feed_id, published_at);",
        )
        .await
        .unwrap();

        // favicon
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS favicon (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              feed_id INTEGER,
              url TEXT,
              mime TEXT,
              data BLOB,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
        "#,
        )
        .await
        .unwrap();
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_favicon_feed ON favicon(feed_id);")
            .await
            .unwrap();
    }

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
        let st = AppState { db };
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
        let st = AppState { db };
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
        let st = AppState { db };
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
        let st = AppState { db };
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
        let st = AppState { db };
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
            user_agent: Some("captura-tests/0.1".into()),
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: Some(false),
            disable_http2: Some(false),
            allow_invalid_certs: Some(false),
            request_timeout_ms: Some(15000),
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
        let st = AppState { db };

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
        let base = fever_endpoint(
            State(st.clone()),
            Query(FeverQuery {
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
        let resp = fever_endpoint(
            State(st.clone()),
            Query(FeverQuery {
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
            }),
        )
        .await;
        let j = json_from_response(resp).await;
        assert!(j.get("groups").is_some());
        assert!(j.get("feeds_groups").is_some());
        assert!(j.get("feeds").is_some());

        // 7) items & unread ids
        let resp = fever_endpoint(
            State(st.clone()),
            Query(FeverQuery {
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
    async fn fever_favicons_and_get_binary() {
        use captura_storage::entity::favicon as fv;
        let db = setup_db().await;
        let st = AppState { db };

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
        let resp = fever_endpoint(
            State(st.clone()),
            Query(FeverQuery {
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
            }),
        )
        .await;
        let j = json_from_response(resp).await;
        assert!(j.get("feeds").is_some());
        assert!(j.get("favicons").is_some());
        let favs = j["favicons"].as_array().unwrap();
        assert!(!favs.is_empty());

        // get favicon binary
        let resp = get_favicon(
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
        let st = AppState { db };
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
        TEST_FAVICON_RESP
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
        let resp = refresh_favicon(
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

async fn refresh_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::Id.eq(id))
        .filter(feed::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    let (entries, meta) = if matches!(f.r#type, feed::FeedType::Rule) {
        let rule_yaml = match f.rule_id {
            Some(rid) => {
                let r = Rule::find_by_id(rid)
                    .one(&st.db)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| bad_request("rule missing"))?;
                r.yaml
            }
            None => return Err(bad_request("rule_id required for rule-type feed")),
        };
        (
            refresh_rule_with_yaml(&f, &rule_yaml)
                .await
                .map_err(internal)?,
            None,
        )
    } else {
        refresh_feed_with_meta(&f).await.map_err(internal)?
    };

    // insert entries
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut inserted = 0;
    let guids: Vec<String> = entries
        .iter()
        .filter_map(|n| n.guid.clone())
        .collect();
    let existing: std::collections::HashSet<String> = if guids.is_empty() {
        Default::default()
    } else {
        Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .filter(entry::Column::Guid.is_in(guids.clone()))
            .select_only()
            .column(entry::Column::Guid)
            .into_tuple::<Option<String>>()
            .all(&st.db)
            .await
            .map_err(internal)?
            .into_iter()
            .flatten()
            .collect()
    };
    let mut models: Vec<entry::ActiveModel> = Vec::new();
    for n in entries {
        if let Some(guid) = n.guid.clone() {
            if existing.contains(&guid) { continue; }
            let mut am: entry::ActiveModel = Default::default();
            am.feed_id = Set(f.id);
            am.guid = Set(Some(guid));
            am.url = Set(n.url);
            am.title = Set(n.title);
            am.summary = Set(n.summary);
            am.content_html = Set(n.content_html);
            am.author = Set(n.author);
            am.published_at = Set(n
                .published_at
                .map(|d| d.with_timezone(&FixedOffset::east_opt(0).unwrap())));
            am.created_at = Set(now);
            am.updated_at = Set(now);
            am.hash = Set(None);
            am.is_read = Set(false);
            am.is_starred = Set(false);
            am.extras_json = Set(Some(n.extras));
            models.push(am);
        }
    }
    if !models.is_empty() {
        let _ = Entry::insert_many(models)
            .on_conflict(
                OnConflict::columns([entry::Column::FeedId, entry::Column::Guid])
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&st.db)
            .await
            .map_err(internal)?;
        inserted = guids.len() - existing.len();
    }

    // Update feed meta if standard fetch provided metadata
    if let Some(m) = meta {
        let mut fm: feed::ActiveModel = Feed::find_by_id(f.id)
            .one(&st.db)
            .await
            .map_err(internal)?
            .unwrap()
            .into();
        fm.checked_at = Set(Some(now));
        fm.error_count = Set(0);
        fm.last_status = Set(m.last_status.map(|s| s as i32));
        fm.etag = Set(m.etag);
        fm.last_modified = Set(m.last_modified);
        let ok_secs: i64 = std::env::var("SCHEDULER_SUCCESS_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(900);
        fm.next_run_at = Set(Some(now + chrono::Duration::seconds(ok_secs.max(60))));
        let _ = fm.update(&st.db).await.map_err(internal)?;
    }

    Ok(Json(serde_json::json!({"inserted": inserted})))
}

#[derive(Serialize)]
struct EnqueueResp {
    id: i64,
}

async fn enqueue_feed_refresh(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<EnqueueResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::Id.eq(id))
        .filter(feed::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = job::ActiveModel {
        user_id: Set(user.user_id),
        feed_id: Set(Some(f.id)),
        rule_id: Set(None),
        job_type: Set(job::JobType::FeedRefresh),
        status: Set(job::JobStatus::Pending),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(0),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let j = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(EnqueueResp { id: j.id }))
}

#[derive(Deserialize)]
struct JobsQuery {
    status: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Serialize)]
struct JobDto {
    id: i64,
    job_type: String,
    status: String,
    run_at: String,
    attempts: i32,
    last_error: Option<String>,
}

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

async fn run_jobs_once(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<serde_json::Value>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let n = scheduler::run_once(&st.db, 10).await.map_err(internal)?;
    Ok(Json(serde_json::json!({"processed": n})))
}

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

fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        e.to_string(),
    )
}
fn not_found(msg: &str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "not_found", msg)
}
fn bad_request<S: Into<String>>(msg: S) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "bad_request", msg)
}
fn unauthorized(msg: &str) -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", msg)
}
fn forbidden(msg: &str) -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "forbidden", msg)
}

#[derive(Deserialize)]
struct TryRuleReq {
    url: String,
    rule_id: Option<i64>,
    yaml: Option<String>,
}

#[derive(Serialize)]
struct TryRuleEntry {
    title: Option<String>,
    url: Option<String>,
    content_len: usize,
}
#[derive(Serialize)]
struct TryRuleResp {
    used_smart: bool,
    list_url: String,
    item_count: usize,
    entries: Vec<TryRuleEntry>,
    ua: Option<String>,
    timeout_ms: Option<u64>,
    respect_robots: Option<bool>,
    delay_ms: Option<u64>,
    limit: Option<usize>,
    proxy_applied: bool,
    list_html_len: usize,
    fallback_used: bool,
    http_status: Option<u16>,
    duration_ms: u128,
    final_url: Option<String>,
    redirect_count: Option<u32>,
    list_item_matches: Option<usize>,
    content_selector_matches: Option<usize>,
}

#[axum::debug_handler]
async fn try_rule(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(req): Json<TryRuleReq>,
) -> ApiResult<Json<TryRuleResp>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if req.url.trim().is_empty() {
        return Err(bad_request("url required"));
    }
    let yaml = if let Some(y) = req.yaml {
        y
    } else {
        let rid = req
            .rule_id
            .ok_or_else(|| bad_request("rule_id or yaml required"))?;
        let r = Rule::find_by_id(rid)
            .one(&st.db)
            .await
            .map_err(internal)?
            .ok_or_else(|| not_found("rule not found"))?;
        r.yaml
    };
    let mut spec = captura_rules::parse_rule(&yaml).map_err(internal)?;
    let _list = match &mut spec.list {
        Some(l) => {
            l.url = req.url.clone();
            l
        }
        None => return Err(bad_request("rule has no list section")),
    };

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let feed_model = feed::Model {
        id: 0,
        user_id: 0,
        category_id: None,
        r#type: feed::FeedType::Rule,
        title: Some("preview".into()),
        site_url: None,
        feed_url: req.url.clone(),
        favicon_id: None,
        rule_id: None,
        user_agent: spec.fetch.user_agent.clone(),
        headers_json: None,
        cookies: None,
        proxy_url: None,
        fetch_via_proxy: false,
        disable_http2: false,
        allow_invalid_certs: false,
        request_timeout_ms: spec.fetch.timeout_ms.map(|v| v as i32),
        checked_at: None,
        next_run_at: None,
        etag: None,
        last_modified: None,
        last_status: None,
        error_count: 0,
        disabled: false,
        scraper_rules: None,
        rewrite_rules: None,
        blocklist_rules: None,
        keeplist_rules: None,
        url_rewrite_rules: None,
        block_filter_entry_rules: None,
        keep_filter_entry_rules: None,
        created_at: now,
        updated_at: now,
    };

    let entries = captura_pipeline::refresh_rule_feed(&feed_model, &spec)
        .await
        .map_err(internal)?;
    let used_smart = spec.fetch.smart.unwrap_or(false);
    // compute list_html_len + logs (fallback + status + timing)
    let mut list_html_len = 0usize;
    let mut list_html = String::new();
    let proxy_applied = spec
        .fetch
        .proxy_url
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let mut fallback_used = false;
    let mut http_status: Option<u16> = None;
    let mut duration_ms: u128 = 0;
    let mut final_url: Option<String> = None;
    let mut redirect_count: Option<u32> = None;
    let started = std::time::Instant::now();
    if used_smart && !proxy_applied {
        let opts = CrawlOptions {
            user_agent: spec.fetch.user_agent.clone(),
            respect_robots: spec.fetch.respect_robots.unwrap_or(true),
            smart: true,
            delay_ms: spec.fetch.delay_ms.unwrap_or(250),
            limit: spec.fetch.limit,
            proxy_url: None,
        };
        match crawler::fetch_html(&req.url, &opts).await {
            Ok(html) => {
                list_html_len = html.len();
                list_html = html;
                duration_ms = started.elapsed().as_millis();
            }
            Err(_) => {
                // fallback to HTTP
                fallback_used = true;
                let mut builder = reqwest::Client::builder();
                if let Some(ref ua) = spec.fetch.user_agent {
                    builder = builder.user_agent(ua.clone());
                }
                if let Some(ms) = spec.fetch.timeout_ms {
                    builder = builder.timeout(std::time::Duration::from_millis(ms));
                }
                if let Some(ref p) = spec.fetch.proxy_url {
                    if !p.is_empty() {
                        if let Ok(proxy) = reqwest::Proxy::all(p) {
                            builder = builder.proxy(proxy);
                        }
                    }
                }
                if let Ok(http) = builder.redirect(reqwest::redirect::Policy::none()).build() {
                    let header_map = if let Some(ref hdrs) = spec.fetch.headers {
                        let mut hm = reqwest::header::HeaderMap::new();
                        for (k, v) in hdrs.iter() {
                            if let Some(s) = v.as_str() {
                                if let Ok(name) =
                                    reqwest::header::HeaderName::from_bytes(k.as_bytes())
                                {
                                    if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                                        hm.insert(name, val);
                                    }
                                }
                            }
                        }
                        Some(hm)
                    } else {
                        None
                    };
                    let mut current = req.url.clone();
                    let mut redirects = 0u32;
                    loop {
                        let mut rq = http.get(&current);
                        if let Some(ref hm) = header_map {
                            rq = rq.headers(hm.clone());
                        }
                        match rq.send().await {
                            Ok(resp) => {
                                http_status = Some(resp.status().as_u16());
                                if resp.status().is_redirection() {
                                    if redirects >= 10 {
                                        break;
                                    }
                                    if let Some(loc) = resp
                                        .headers()
                                        .get(reqwest::header::LOCATION)
                                        .and_then(|v| v.to_str().ok())
                                    {
                                        if let Ok(next) = resp.url().join(loc) {
                                            current = next.to_string();
                                            redirects += 1;
                                            continue;
                                        } else {
                                            break;
                                        }
                                    } else {
                                        break;
                                    }
                                } else {
                                    final_url = Some(resp.url().to_string());
                                    if let Ok(html) = resp.text().await {
                                        list_html_len = html.len();
                                        list_html = html;
                                    }
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    redirect_count = Some(redirects);
                }
                duration_ms = started.elapsed().as_millis();
            }
        }
    } else {
        let mut builder = reqwest::Client::builder();
        if let Some(ref ua) = spec.fetch.user_agent {
            builder = builder.user_agent(ua.clone());
        }
        if let Some(ms) = spec.fetch.timeout_ms {
            builder = builder.timeout(std::time::Duration::from_millis(ms));
        }
        if let Some(ref p) = spec.fetch.proxy_url {
            if !p.is_empty() {
                if let Ok(proxy) = reqwest::Proxy::all(p) {
                    builder = builder.proxy(proxy);
                }
            }
        }
        if let Ok(http) = builder.redirect(reqwest::redirect::Policy::none()).build() {
            let header_map = if let Some(ref hdrs) = spec.fetch.headers {
                let mut hm = reqwest::header::HeaderMap::new();
                for (k, v) in hdrs.iter() {
                    if let Some(s) = v.as_str() {
                        if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                            if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                                hm.insert(name, val);
                            }
                        }
                    }
                }
                Some(hm)
            } else {
                None
            };
            let mut current = req.url.clone();
            let mut redirects = 0u32;
            loop {
                let mut rq = http.get(&current);
                if let Some(ref hm) = header_map {
                    rq = rq.headers(hm.clone());
                }
                match rq.send().await {
                    Ok(resp) => {
                        http_status = Some(resp.status().as_u16());
                        if resp.status().is_redirection() {
                            if redirects >= 10 {
                                break;
                            }
                            if let Some(loc) = resp
                                .headers()
                                .get(reqwest::header::LOCATION)
                                .and_then(|v| v.to_str().ok())
                            {
                                if let Ok(next) = resp.url().join(loc) {
                                    current = next.to_string();
                                    redirects += 1;
                                    continue;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        } else {
                            final_url = Some(resp.url().to_string());
                            if let Ok(html) = resp.text().await {
                                list_html_len = html.len();
                                list_html = html;
                            }
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            redirect_count = Some(redirects);
        }
        duration_ms = started.elapsed().as_millis();
    }

    // selector match stats
    let mut list_item_matches: Option<usize> = None;
    let mut content_selector_matches: Option<usize> = None;
    if let Some(list) = &spec.list {
        if let Ok(sel) = scraper::Selector::parse(&list.item) {
            let doc = scraper::Html::parse_document(&list_html);
            list_item_matches = Some(doc.select(&sel).count());
        }
    }
    if let Some(first) = entries.iter().find(|e| e.url.is_some()) {
        if spec.content.r#use == "css" {
            if let Some(ref sel_str) = spec.content.selector {
                // fetch content html first (avoid holding selector across await)
                let mut content_html = String::new();
                if used_smart && !proxy_applied {
                    let opts = CrawlOptions {
                        user_agent: spec.fetch.user_agent.clone(),
                        respect_robots: spec.fetch.respect_robots.unwrap_or(true),
                        smart: true,
                        delay_ms: spec.fetch.delay_ms.unwrap_or(250),
                        limit: spec.fetch.limit,
                        proxy_url: None,
                    };
                    if let Some(ref u) = first.url {
                        if let Ok(h) = crawler::fetch_html(u, &opts).await {
                            content_html = h;
                        }
                    }
                } else {
                    let mut builder = reqwest::Client::builder();
                    if let Some(ref ua) = spec.fetch.user_agent {
                        builder = builder.user_agent(ua.clone());
                    }
                    if let Some(ms) = spec.fetch.timeout_ms {
                        builder = builder.timeout(std::time::Duration::from_millis(ms));
                    }
                    if let Some(ref p) = spec.fetch.proxy_url {
                        if !p.is_empty() {
                            if let Ok(proxy) = reqwest::Proxy::all(p) {
                                builder = builder.proxy(proxy);
                            }
                        }
                    }
                    if let Ok(http) = builder.build() {
                        if let Some(ref u) = first.url {
                            if let Ok(resp) = http.get(u).send().await {
                                if let Ok(h) = resp.text().await {
                                    content_html = h;
                                }
                            }
                        }
                    }
                }
                if let Ok(sel) = Selector::parse(sel_str) {
                    let doc = Html::parse_document(&content_html);
                    content_selector_matches = Some(doc.select(&sel).count());
                }
            }
        }
    }
    let mut out = Vec::new();
    for e in entries.iter().take(5) {
        let len = e.content_html.as_ref().map(|s| s.len()).unwrap_or(0);
        out.push(TryRuleEntry {
            title: e.title.clone(),
            url: e.url.clone(),
            content_len: len,
        });
    }
    Ok(Json(TryRuleResp {
        used_smart,
        list_url: req.url,
        item_count: entries.len(),
        entries: out,
        ua: spec.fetch.user_agent.clone(),
        timeout_ms: spec.fetch.timeout_ms,
        respect_robots: spec.fetch.respect_robots,
        delay_ms: spec.fetch.delay_ms,
        limit: spec.fetch.limit,
        proxy_applied,
        list_html_len,
        fallback_used,
        http_status,
        duration_ms,
        final_url,
        redirect_count,
        list_item_matches,
        content_selector_matches,
    }))
}
fn validate_limit_offset(limit: Option<u64>, offset: Option<u64>) -> ApiResult<()> {
    if let Some(l) = limit {
        if l > 500 {
            return Err(bad_request("limit too large (max 500)"));
        }
    }
    if let Some(_o) = offset { /* allow any u64 */ }
    Ok(())
}

fn validate_sort(
    sort_by: &Option<String>,
    allowed: &[&str],
    order: &Option<String>,
) -> ApiResult<()> {
    if let Some(ref s) = sort_by {
        if !allowed.iter().any(|a| a == s) {
            return Err(bad_request("invalid sort_by"));
        }
    }
    if let Some(ref o) = order {
        if o != "asc" && o != "desc" {
            return Err(bad_request("invalid order"));
        }
    }
    Ok(())
}

async fn assert_category_ownership(
    db: &DatabaseConnection,
    user_id: i64,
    category_id: i64,
) -> ApiResult<()> {
    let cat = Category::find_by_id(category_id)
        .one(db)
        .await
        .map_err(internal)?;
    let Some(cat) = cat else {
        return Err(bad_request("category not found"));
    };
    if cat.user_id != user_id {
        return Err(forbidden("category not owned by user"));
    }
    Ok(())
}
#[derive(Deserialize)]
struct ReaderQuery {
    output: Option<String>,
    n: Option<u64>,
    s: Option<String>,
    c: Option<String>,
    q: Option<String>,
}

#[derive(Serialize)]
struct ReaderSubscriptionCategory {
    id: String,
    label: String,
}

#[derive(Serialize)]
struct ReaderSubscriptionItem {
    id: String,
    title: String,
    categories: Vec<ReaderSubscriptionCategory>,
    url: String,
    #[serde(rename = "htmlUrl")]
    html_url: Option<String>,
    #[serde(rename = "iconUrl")]
    icon_url: Option<String>,
}

#[derive(Serialize)]
struct ReaderSubscriptionListResp {
    subscriptions: Vec<ReaderSubscriptionItem>,
}

async fn reader_subscription_list(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    _q: Query<ReaderQuery>,
) -> ApiResult<Json<ReaderSubscriptionListResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_map: std::collections::HashMap<i64, String> =
        cats.into_iter().map(|c| (c.id, c.name)).collect();
    let items: Vec<ReaderSubscriptionItem> = feeds
        .into_iter()
        .map(|f| ReaderSubscriptionItem {
            id: format!("feed/{}", f.feed_url),
            title: f.title.unwrap_or_else(|| f.feed_url.clone()),
            categories: f
                .category_id
                .and_then(|id| cat_map.get(&id).cloned())
                .map(|name| vec![ReaderSubscriptionCategory {
                    id: format!("user/-/label/{}", name),
                    label: name,
                }])
                .unwrap_or_default(),
            url: f.feed_url.clone(),
            html_url: f.site_url.clone(),
            icon_url: f
                .favicon_id
                .map(|i| format!("/api/v1/favicons/{}", i)),
        })
        .collect();
    Ok(Json(ReaderSubscriptionListResp { subscriptions: items }))
}

#[derive(Serialize)]
struct ReaderOrigin {
    #[serde(rename = "streamId")]
    stream_id: String,
    title: Option<String>,
    #[serde(rename = "htmlUrl")]
    html_url: Option<String>,
}

#[derive(Serialize)]
struct ReaderLink { href: String, r#type: &'static str }

#[derive(Serialize)]
struct ReaderContent { content: String }

#[derive(Serialize)]
struct ReaderItem {
    id: String,
    title: Option<String>,
    published: i64,
    updated: i64,
    #[serde(rename = "crawlTimeMsec")]
    crawl_time_msec: String,
    categories: Vec<String>,
    alternate: Vec<ReaderLink>,
    origin: ReaderOrigin,
    author: Option<String>,
    summary: Option<ReaderContent>,
    content: Option<ReaderContent>,
}

#[derive(Serialize)]
struct ReaderStreamResp {
    items: Vec<ReaderItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation: Option<String>,
}

async fn reader_stream_contents(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderQuery>,
) -> ApiResult<Json<ReaderStreamResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let limit = q.n.unwrap_or(50).min(200);
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_map: std::collections::HashMap<i64, String> =
        cats.into_iter().map(|c| (c.id, c.name)).collect();

    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));

    // filter by stream id
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let feed_url = s.trim_start_matches("feed/");
            sel = sel.filter(feed::Column::FeedUrl.eq(feed_url));
        } else if s.starts_with("user/-/label/") {
            let label = s.trim_start_matches("user/-/label/");
            // map label to category
            let cat = Category::find()
                .filter(category::Column::UserId.eq(user.user_id))
                .filter(category::Column::Name.eq(label))
                .one(&st.db)
                .await
                .map_err(internal)?;
            if let Some(cat) = cat {
                sel = sel.filter(feed::Column::CategoryId.eq(cat.id));
            } else {
                sel = sel.filter(entry::Column::Id.eq(-1));
            }
        } else if s.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(true));
        } else if s.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(true));
        } else if s.ends_with("/reading-list") {
            // default list = all items
        }
    }

    // simple search over title/summary/content_html
    if let Some(ref qq) = q.q {
        if !qq.trim().is_empty() {
            let like = format!("%{}%", qq);
            let cond = Condition::any()
                .add(entry::Column::Title.like(like.as_str()))
                .add(entry::Column::Summary.like(like.as_str()))
                .add(entry::Column::ContentHtml.like(like.as_str()));
            sel = sel.filter(cond);
        }
    }

    // continuation based on entry id
    if let Some(ref c) = q.c {
        let id_cut = c.chars().rev().take_while(|ch| ch.is_ascii_digit()).collect::<String>().chars().rev().collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }

    sel = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit);

    let rows = sel.find_also_related(Feed).all(&st.db).await.map_err(internal)?;

    let items: Vec<ReaderItem> = rows
        .into_iter()
        .filter_map(|(e, f)| f.map(|feed| (e, feed)))
        .map(|(e, f)| {
            let published = e
                .published_at
                .map(|d| d.timestamp())
                .unwrap_or_else(|| e.created_at.timestamp());
            let updated = e.updated_at.timestamp();
            let mut categories = Vec::new();
            // states
            if e.is_read {
                categories.push("user/-/state/com.google/read".to_string());
            }
            if e.is_starred {
                categories.push("user/-/state/com.google/starred".to_string());
            }
            categories.push("user/-/state/com.google/reading-list".to_string());
            if let Some(cid) = f.category_id {
                if let Some(name) = cat_map.get(&cid) {
                    categories.push(format!("user/-/label/{}", name));
                }
            }
            ReaderItem {
                id: format!("tag:captura,item:{}", e.id),
                title: e.title.clone(),
                published,
                updated,
                crawl_time_msec: (e.created_at.timestamp_millis()).to_string(),
                categories,
                alternate: e
                    .url
                    .clone()
                    .map(|u| vec![ReaderLink { href: u, r#type: "text/html" }])
                    .unwrap_or_default(),
                origin: ReaderOrigin {
                    stream_id: format!("feed/{}", f.feed_url),
                    title: f.title.clone(),
                    html_url: f.site_url.clone(),
                },
                author: e.author.clone(),
                summary: e.summary.clone().map(|s| ReaderContent { content: s }),
                content: e.content_html.clone().map(|s| ReaderContent { content: s }),
            }
        })
        .collect();

    let cont = items
        .last()
        .and_then(|it| it.id.split(':').last().and_then(|s| s.parse::<i64>().ok()))
        .map(|id| format!("tag:captura,item:{}", id));
    Ok(Json(ReaderStreamResp { items, continuation: cont }))
}

#[derive(Deserialize)]
struct ReaderItemsIdsQuery {
    n: Option<u64>,
    s: Option<String>,
    c: Option<String>,
    xt: Option<String>,
    q: Option<String>,
}

#[derive(Serialize)]
struct ReaderItemRef {
    id: String,
    #[serde(rename = "directStreamIds")]
    direct_stream_ids: Vec<String>,
    #[serde(rename = "timestampUsec")]
    timestamp_usec: String,
}

#[derive(Serialize)]
struct ReaderItemsIdsResp {
    #[serde(rename = "itemRefs")]
    item_refs: Vec<ReaderItemRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation: Option<String>,
}

async fn reader_items_ids(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderItemsIdsQuery>,
) -> ApiResult<Json<ReaderItemsIdsResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let limit = q.n.unwrap_or(50).min(500);
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_map: std::collections::HashMap<i64, String> =
        cats.into_iter().map(|c| (c.id, c.name)).collect();

    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let feed_url = s.trim_start_matches("feed/");
            sel = sel.filter(feed::Column::FeedUrl.eq(feed_url));
        } else if s.starts_with("user/-/label/") {
            let label = s.trim_start_matches("user/-/label/");
            let cat = Category::find()
                .filter(category::Column::UserId.eq(user.user_id))
                .filter(category::Column::Name.eq(label))
                .one(&st.db)
                .await
                .map_err(internal)?;
            if let Some(cat) = cat {
                sel = sel.filter(feed::Column::CategoryId.eq(cat.id));
            } else {
                sel = sel.filter(entry::Column::Id.eq(-1));
            }
        } else if s.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(true));
        } else if s.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(true));
        } else if s.ends_with("/reading-list") {
            // default list
        }
    }
    // xt exclude tag (e.g., read)
    if let Some(ref xt) = q.xt {
        if xt.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(false));
        } else if xt.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(false));
        }
    }
    if let Some(ref c) = q.c {
        let id_cut = c.chars().rev().take_while(|ch| ch.is_ascii_digit()).collect::<String>().chars().rev().collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref qq) = q.q {
        if !qq.trim().is_empty() {
            let like = format!("%{}%", qq);
            let cond = Condition::any()
                .add(entry::Column::Title.like(like.as_str()))
                .add(entry::Column::Summary.like(like.as_str()))
                .add(entry::Column::ContentHtml.like(like.as_str()));
            sel = sel.filter(cond);
        }
    }
    let rows = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit)
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;

    let mut item_refs = Vec::new();
    for (e, f) in rows.iter().filter_map(|(e, f)| f.as_ref().map(|ff| (e, ff))) {
        let ts = e
            .published_at
            .unwrap_or(e.created_at)
            .timestamp_micros()
            .to_string();
        let mut streams = vec![
            "user/-/state/com.google/reading-list".to_string(),
            format!("feed/{}", f.feed_url),
        ];
        if let Some(cid) = f.category_id {
            if let Some(name) = cat_map.get(&cid) {
                streams.push(format!("user/-/label/{}", name));
            }
        }
        let id = format!("tag:captura,item:{}", e.id);
        item_refs.push(ReaderItemRef {
            id,
            direct_stream_ids: streams,
            timestamp_usec: ts,
        });
    }
    let cont = item_refs
        .last()
        .and_then(|it| it.id.split(':').last().and_then(|s| s.parse::<i64>().ok()))
        .map(|id| format!("tag:captura,item:{}", id));
    Ok(Json(ReaderItemsIdsResp {
        item_refs,
        continuation: cont,
    }))
}

#[derive(Deserialize)]
struct ReaderItemsContentsQuery {
    i: Vec<String>,
}

#[derive(Serialize)]
struct ReaderItemsContentsResp {
    items: Vec<ReaderItem>,
}

async fn reader_items_contents(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderItemsContentsQuery>,
) -> ApiResult<Json<ReaderItemsContentsResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let mut ids: Vec<i64> = Vec::new();
    for raw in &q.i {
        for part in raw.split(',') {
            let last = part.trim().split(':').last().unwrap_or("");
            if let Ok(id) = last.parse::<i64>() {
                ids.push(id);
            }
        }
    }
    if ids.is_empty() {
        return Ok(Json(ReaderItemsContentsResp { items: vec![] }));
    }
    let rows = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(entry::Column::Id.is_in(ids))
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_map: std::collections::HashMap<i64, String> =
        cats.into_iter().map(|c| (c.id, c.name)).collect();
    let items: Vec<ReaderItem> = rows
        .into_iter()
        .filter_map(|(e, f)| f.map(|feed| (e, feed)))
        .map(|(e, f)| {
            let published = e
                .published_at
                .map(|d| d.timestamp())
                .unwrap_or_else(|| e.created_at.timestamp());
            let updated = e.updated_at.timestamp();
            let mut categories = Vec::new();
            if e.is_read {
                categories.push("user/-/state/com.google/read".to_string());
            }
            if e.is_starred {
                categories.push("user/-/state/com.google/starred".to_string());
            }
            categories.push("user/-/state/com.google/reading-list".to_string());
            if let Some(cid) = f.category_id {
                if let Some(name) = cat_map.get(&cid) {
                    categories.push(format!("user/-/label/{}", name));
                }
            }
            ReaderItem {
                id: format!("tag:captura,item:{}", e.id),
                title: e.title.clone(),
                published,
                updated,
                crawl_time_msec: (e.created_at.timestamp_millis()).to_string(),
                categories,
                alternate: e
                    .url
                    .clone()
                    .map(|u| vec![ReaderLink { href: u, r#type: "text/html" }])
                    .unwrap_or_default(),
                origin: ReaderOrigin {
                    stream_id: format!("feed/{}", f.feed_url),
                    title: f.title.clone(),
                    html_url: f.site_url.clone(),
                },
                author: e.author.clone(),
                summary: e.summary.clone().map(|s| ReaderContent { content: s }),
                content: e.content_html.clone().map(|s| ReaderContent { content: s }),
            }
        })
        .collect();
    Ok(Json(ReaderItemsContentsResp { items }))
}

#[derive(Deserialize)]
struct ReaderEditTagForm {
    // add/remove tag
    a: Option<String>,
    r: Option<String>,
    // item ids (comma separated)
    i: Option<String>,
}

async fn reader_edit_tag(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderEditTagForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let ids: Vec<i64> = f
        .i
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();
    if ids.is_empty() { return Ok("OK"); }

    // ensure items belong to user
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    if let Some(a) = f.a.as_deref() {
        if a.ends_with("/read") {
            let _ = Entry::update_many()
                .col_expr(entry::Column::IsRead, Expr::value(true))
                .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        } else if a.ends_with("/starred") {
            let _ = Entry::update_many()
                .col_expr(entry::Column::IsStarred, Expr::value(true))
                .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        }
    }
    if let Some(rm) = f.r.as_deref() {
        if rm.ends_with("/read") {
            let _ = Entry::update_many()
                .col_expr(entry::Column::IsRead, Expr::value(false))
                .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        } else if rm.ends_with("/starred") {
            let _ = Entry::update_many()
                .col_expr(entry::Column::IsStarred, Expr::value(false))
                .col_expr(entry::Column::UpdatedAt, Expr::value(now))
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        }
    }
    Ok("OK")
}

#[derive(Deserialize)]
struct ReaderMarkAllForm {
    s: Option<String>, // stream id (feed/..., user/-/label/..., reading-list)
    ts: Option<i64>,   // timestamp
}

async fn reader_mark_all_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderMarkAllForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let mut cond = Condition::all();
    // scope to user
    cond = cond.add(entry::Column::FeedId.in_subquery({
        use sea_orm::sea_query::{Alias, Expr as QExpr, Query as QQuery, SelectStatement};
        let mut q: SelectStatement = QQuery::select();
        q.column((Alias::new("feed"), Alias::new("id")));
        q.from(Alias::new("feed"));
        q.and_where(QExpr::col((Alias::new("feed"), Alias::new("user_id"))).eq(user.user_id));
        q
    }));
    cond = cond.add(entry::Column::IsRead.eq(false));
    if let Some(ts) = f.ts {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            cond = cond.add(entry::Column::CreatedAt.lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())));
        }
    }
    if let Some(ref s) = f.s {
        if s.starts_with("feed/") {
            let feed_url = s.trim_start_matches("feed/");
            cond = cond.add(entry::Column::FeedId.in_subquery({
                use sea_orm::sea_query::{Alias, Expr as QExpr, Query as QQuery, SelectStatement};
                let mut q: SelectStatement = QQuery::select();
                q.column((Alias::new("feed"), Alias::new("id")));
                q.from(Alias::new("feed"));
                q.and_where(QExpr::col((Alias::new("feed"), Alias::new("user_id"))).eq(user.user_id));
                q.and_where(QExpr::col((Alias::new("feed"), Alias::new("feed_url"))).eq(feed_url));
                q
            }));
        } else if s.starts_with("user/-/label/") {
            let label = s.trim_start_matches("user/-/label/");
            cond = cond.add(entry::Column::FeedId.in_subquery({
                use sea_orm::sea_query::{Alias, Expr as QExpr, Query as QQuery, SelectStatement};
                let mut sub: SelectStatement = QQuery::select();
                sub.column((Alias::new("category"), Alias::new("id")));
                sub.from(Alias::new("category"));
                sub.and_where(QExpr::col((Alias::new("category"), Alias::new("name"))).eq(label));

                let mut q: SelectStatement = QQuery::select();
                q.column((Alias::new("feed"), Alias::new("id")));
                q.from(Alias::new("feed"));
                q.and_where(QExpr::col((Alias::new("feed"), Alias::new("user_id"))).eq(user.user_id));
                q.and_where(QExpr::col((Alias::new("feed"), Alias::new("category_id"))).in_subquery(sub));
                q
            }));
        }
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let _ = Entry::update_many()
        .col_expr(entry::Column::IsRead, Expr::value(true))
        .col_expr(entry::Column::UpdatedAt, Expr::value(now))
        .filter(cond)
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok("OK")
}

#[derive(Serialize)]
struct ReaderUnreadCountItem {
    id: String,
    count: i64,
    #[serde(rename = "newestItemTimestampUsec")]
    newest_item_ts_usec: String,
}

#[derive(Serialize)]
struct ReaderUnreadCountResp {
    unreadcounts: Vec<ReaderUnreadCountItem>,
}

async fn reader_unread_count(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<ReaderUnreadCountResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    // feeds map
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut feed_map = std::collections::HashMap::new();
    for f in &feeds {
        feed_map.insert(f.id, f.clone());
    }
    // unread counts per feed (simple per-feed queries for clarity)
    let mut per_feed: Vec<(i64, i64)> = Vec::new();
    for f in &feeds {
        let cnt = Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .filter(entry::Column::IsRead.eq(false))
            .count(&st.db)
            .await
            .map_err(internal)? as i64;
        if cnt > 0 { per_feed.push((f.id, cnt)); }
    }
    // newest unread per scope (total + feed + label)
    let newest_total = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(entry::Column::IsRead.eq(false))
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .one(&st.db)
        .await
        .map_err(internal)?
        .map(|e| e.published_at.unwrap_or(e.created_at).timestamp_micros())
        .unwrap_or(0);

    let mut items: Vec<ReaderUnreadCountItem> = Vec::new();
    // total reading list
    let total_unread = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(entry::Column::IsRead.eq(false))
        .count(&st.db)
        .await
        .map_err(internal)?;
    items.push(ReaderUnreadCountItem {
        id: "user/-/state/com.google/reading-list".to_string(),
        count: total_unread as i64,
        newest_item_ts_usec: newest_total.to_string(),
    });
    // per feed
    for (fid, cnt) in per_feed {
        if let Some(f) = feed_map.get(&fid) {
            // newest for this feed
            let newest = Entry::find()
                .filter(entry::Column::FeedId.eq(fid))
                .filter(entry::Column::IsRead.eq(false))
                .order_by_desc(entry::Column::PublishedAt)
                .order_by_desc(entry::Column::CreatedAt)
                .one(&st.db)
                .await
                .map_err(internal)?
                .map(|e| e.published_at.unwrap_or(e.created_at).timestamp_micros())
                .unwrap_or(0);
            items.push(ReaderUnreadCountItem {
                id: format!("feed/{}", f.feed_url),
                count: cnt,
                newest_item_ts_usec: newest.to_string(),
            });
        }
    }
    // per label (category)
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    for c in cats {
        // count unread where feed.category_id = c.id
        let cnt = Entry::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.user_id))
            .filter(feed::Column::CategoryId.eq(c.id))
            .filter(entry::Column::IsRead.eq(false))
            .count(&st.db)
            .await
            .map_err(internal)? as i64;
        if cnt > 0 {
            let newest = Entry::find()
                .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
                .filter(feed::Column::UserId.eq(user.user_id))
                .filter(feed::Column::CategoryId.eq(c.id))
                .filter(entry::Column::IsRead.eq(false))
                .order_by_desc(entry::Column::PublishedAt)
                .order_by_desc(entry::Column::CreatedAt)
                .one(&st.db)
                .await
                .map_err(internal)?
                .map(|e| e.published_at.unwrap_or(e.created_at).timestamp_micros())
                .unwrap_or(0);
            items.push(ReaderUnreadCountItem {
                id: format!("user/-/label/{}", c.name),
                count: cnt,
                newest_item_ts_usec: newest.to_string(),
            });
        }
    }
    Ok(Json(ReaderUnreadCountResp { unreadcounts: items }))
}

#[derive(Deserialize)]
struct ReaderQuickAddForm { quickadd: String }

#[derive(Serialize)]
struct ReaderQuickAddResp {
    numResults: i32,
    streamId: String,
    query: String,
}

async fn reader_subscription_quickadd(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderQuickAddForm>,
) -> ApiResult<Json<ReaderQuickAddResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let url = f.quickadd.trim();
    // exists?
    let dup = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::FeedUrl.eq(url))
        .one(&st.db)
        .await
        .map_err(internal)?;
    if dup.is_none() {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let am = feed::ActiveModel {
            user_id: Set(user.user_id),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(None),
            site_url: Set(None),
            feed_url: Set(url.to_string()),
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
            favicon_id: Set(None),
            ..Default::default()
        };
        let _ = am.insert(&st.db).await.map_err(internal)?;
    }
    Ok(Json(ReaderQuickAddResp { numResults: 1, streamId: format!("feed/{}", url), query: url.to_string() }))
}

#[derive(Deserialize)]
struct ReaderSubEditForm {
    ac: String, // subscribe | unsubscribe | edit
    s: String,  // stream id: feed/<url>
    t: Option<String>, // title
    a: Option<String>, // add label id
    r: Option<String>, // remove label id
}

async fn reader_subscription_edit(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderSubEditForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let feed_url = f.s.trim_start_matches("feed/");
    match f.ac.as_str() {
        "subscribe" => {
            let exists = Feed::find()
                .filter(feed::Column::UserId.eq(user.user_id))
                .filter(feed::Column::FeedUrl.eq(feed_url))
                .one(&st.db)
                .await
                .map_err(internal)?;
            if exists.is_none() {
                let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
                // optional category from a label
                let category_id = if let Some(a) = f.a.as_deref() {
                    if a.starts_with("user/-/label/") {
                        let name = a.trim_start_matches("user/-/label/").to_string();
                        ensure_category(&st.db, user.user_id, &name, now).await.map_err(internal)?
                    } else { None }
                } else { None };
                let am = feed::ActiveModel {
                    user_id: Set(user.user_id),
                    category_id: Set(category_id),
                    r#type: Set(feed::FeedType::Rss),
                    title: Set(f.t.clone()),
                    site_url: Set(None),
                    feed_url: Set(feed_url.to_string()),
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
                    favicon_id: Set(None),
                    ..Default::default()
                };
                let _ = am.insert(&st.db).await.map_err(internal)?;
            }
        }
        "unsubscribe" => {
            if let Some(fm) = Feed::find()
                .filter(feed::Column::UserId.eq(user.user_id))
                .filter(feed::Column::FeedUrl.eq(feed_url))
                .one(&st.db)
                .await
                .map_err(internal)?
            {
                let am: feed::ActiveModel = fm.into();
                am.delete(&st.db).await.map_err(internal)?;
            }
        }
        "edit" => {
            if let Some(mut fm) = Feed::find()
                .filter(feed::Column::UserId.eq(user.user_id))
                .filter(feed::Column::FeedUrl.eq(feed_url))
                .one(&st.db)
                .await
                .map_err(internal)?
            {
                let mut am: feed::ActiveModel = fm.into();
                if let Some(t) = f.t.clone() { am.title = Set(Some(t)); }
                if let Some(a) = f.a.as_deref() {
                    if a.starts_with("user/-/label/") {
                        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
                        let name = a.trim_start_matches("user/-/label/");
                        am.category_id = Set(ensure_category(&st.db, user.user_id, name, now).await.map_err(internal)?);
                    }
                }
                if let Some(rm) = f.r.as_deref() {
                    if rm.starts_with("user/-/label/") {
                        // 单分类模型：移除等同于置空
                        am.category_id = Set(None);
                    }
                }
                let _ = am.update(&st.db).await.map_err(internal)?;
            }
        }
        _ => {}
    }
    Ok("OK")
}

async fn ensure_category(
    db: &DatabaseConnection,
    user_id: i64,
    name: &str,
    now: chrono::DateTime<FixedOffset>,
) -> Result<Option<i64>, DbErr> {
    if let Some(c) = Category::find()
        .filter(category::Column::UserId.eq(user_id))
        .filter(category::Column::Name.eq(name))
        .one(db)
        .await? {
        Ok(Some(c.id))
    } else {
        let am = category::ActiveModel { user_id: Set(user_id), name: Set(name.to_string()), created_at: Set(now), ..Default::default() };
        let c = am.insert(db).await?;
        Ok(Some(c.id))
    }
}
