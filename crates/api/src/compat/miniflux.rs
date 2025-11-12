use crate::auth::mf_auth;
use crate::error::{bad_request, internal, not_found};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::Engine as _;
use captura_service as service;
use captura_storage::entity::enclosure;
use captura_storage::entity::{category, entry, feed, job, prelude::*};
use captura_storage::entity::{entry_label, label, token};
use chrono::{FixedOffset, Utc};
use rand_core::{OsRng, RngCore};
use scraper::{Html, Selector};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use url::Url;

// Miniflux 兼容错误：{"error_message": "..."}
pub(crate) struct MfError {
    status: StatusCode,
    message: String,
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

type MfResult<T> = Result<T, MfError>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(mf_me))
        .route(
            "/categories",
            get(mf_list_categories).post(mf_create_category),
        )
        .route("/categories/counters", get(mf_categories_counters))
        .route(
            "/categories/:id/mark-all-as-read",
            put(mf_category_mark_all_read),
        )
        .route("/categories/:id/feeds", get(mf_category_feeds))
        .route("/categories/:id/refresh", put(mf_category_refresh))
        .route(
            "/categories/:id",
            put(mf_update_category).delete(mf_delete_category),
        )
        .route("/feeds/counters", get(mf_feeds_counters))
        .route("/version", get(mf_version))
        .route("/feeds", get(mf_list_feeds).post(mf_create_feed))
        .route("/feeds/refresh", put(mf_refresh_feeds))
        .route(
            "/feeds/:id",
            get(mf_get_feed).put(mf_update_feed).delete(mf_delete_feed),
        )
        .route("/feeds/:id/mark-all-read", post(mf_feed_mark_all_read))
        .route("/feeds/:id/refresh", post(mf_feed_refresh))
        .route("/feeds/:id/icon", get(mf_feed_icon))
        .route("/entries", get(mf_list_entries).put(mf_update_entries_bulk))
        .route("/entries/:id", get(mf_get_entry).put(mf_update_entry))
        .route("/entries/:id/star", put(mf_toggle_star))
        .route("/entries/:id/bookmark", put(mf_toggle_star))
        .route("/entries/:id/save", post(mf_save_entry))
        .route(
            "/entries/:id/tags",
            post(mf_add_entry_tags).delete(mf_remove_entry_tags),
        )
        .route("/entries/:id/fetch-content", get(mf_fetch_entry_content))
        .route("/feeds/:id/entries", get(mf_feed_entries))
        .route("/categories/:id/entries", get(mf_category_entries))
        .route("/flush-history", put(mf_flush_history))
        .route("/users/:id/mark-all-as-read", put(mf_user_mark_all_read))
        .route("/api-keys", get(mf_api_keys).post(mf_create_api_key))
        .route("/api-keys/:id", delete(mf_delete_api_key))
        .route("/icons/:id", get(mf_icon))
        .route(
            "/enclosures/:id",
            get(mf_get_enclosure).put(mf_update_enclosure),
        )
        .route("/export", get(mf_export))
        .route("/import", post(mf_import))
        .route("/discover", post(mf_discover))
        .route("/integrations/status", get(mf_integrations_status))
        .route("/tags", get(mf_list_tags).post(mf_create_tag))
        .route(
            "/tags/:name",
            get(mf_get_tag).delete(mf_delete_tag).put(mf_rename_tag),
        )
}

#[derive(serde::Serialize)]
pub(crate) struct MfUserDto {
    id: i64,
    username: String,
}

async fn mf_me(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<MfUserDto>> {
    let auth = mf_auth(&st, &headers).await?;
    let u = User::find_by_id(auth.user_id)
        .one(&st.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("user"))?;
    Ok(Json(MfUserDto {
        id: u.id,
        username: u.username,
    }))
}

#[derive(serde::Serialize)]
pub(crate) struct MfCategoryDto {
    id: i64,
    title: String,
}

async fn mf_list_categories(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfCategoryDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .order_by_asc(category::Column::Id)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        cats.into_iter()
            .map(|c| MfCategoryDto {
                id: c.id,
                title: c.name,
            })
            .collect(),
    ))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfCreateCategory {
    title: String,
}

async fn mf_create_category(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateCategory>,
) -> MfResult<Json<MfCategoryDto>> {
    let auth = mf_auth(&st, &headers).await?;
    if body.title.trim().is_empty() {
        return Err(bad_request("title required").into());
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = category::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(body.title.clone()),
        created_at: Set(now),
        ..Default::default()
    };
    let c = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfCategoryDto {
        id: c.id,
        title: c.name,
    }))
}

#[derive(serde::Serialize)]
pub(crate) struct MfFeedDto {
    id: i64,
    #[serde(rename = "user_id")]
    user_id: i64,
    feed_url: String,
    site_url: Option<String>,
    title: Option<String>,
    #[serde(rename = "checked_at", skip_serializing_if = "Option::is_none")]
    checked_at: Option<String>,
    #[serde(rename = "etag_header", skip_serializing_if = "Option::is_none")]
    etag_header: Option<String>,
    #[serde(
        rename = "last_modified_header",
        skip_serializing_if = "Option::is_none"
    )]
    last_modified_header: Option<String>,
    #[serde(
        rename = "parsing_error_message",
        skip_serializing_if = "Option::is_none"
    )]
    parsing_error_message: Option<String>,
    #[serde(rename = "parsing_error_count")]
    parsing_error_count: i32,
    disabled: bool,
    #[serde(rename = "ignore_http_cache")]
    ignore_http_cache: bool,
    #[serde(rename = "allow_self_signed_certificates")]
    allow_self_signed_certificates: bool,
    #[serde(rename = "fetch_via_proxy")]
    fetch_via_proxy: bool,
    #[serde(rename = "scraper_rules", skip_serializing_if = "Option::is_none")]
    scraper_rules: Option<String>,
    #[serde(rename = "rewrite_rules", skip_serializing_if = "Option::is_none")]
    rewrite_rules: Option<String>,
    #[serde(rename = "urlrewrite_rules", skip_serializing_if = "Option::is_none")]
    urlrewrite_rules: Option<String>,
    #[serde(rename = "blocklist_rules", skip_serializing_if = "Option::is_none")]
    blocklist_rules: Option<String>,
    #[serde(rename = "keeplist_rules", skip_serializing_if = "Option::is_none")]
    keeplist_rules: Option<String>,
    #[serde(
        rename = "block_filter_entry_rules",
        skip_serializing_if = "Option::is_none"
    )]
    block_filter_entry_rules: Option<String>,
    #[serde(
        rename = "keep_filter_entry_rules",
        skip_serializing_if = "Option::is_none"
    )]
    keep_filter_entry_rules: Option<String>,
    #[serde(rename = "crawler")]
    crawler: bool,
    #[serde(rename = "user_agent", skip_serializing_if = "Option::is_none")]
    user_agent: Option<String>,
    #[serde(rename = "cookie", skip_serializing_if = "Option::is_none")]
    cookie: Option<String>,
    #[serde(rename = "username", skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<MfCategoryDto>,
    #[serde(rename = "hide_globally")]
    hide_globally: bool,
    #[serde(rename = "disable_http2")]
    disable_http2: bool,
    #[serde(rename = "proxy_url", skip_serializing_if = "Option::is_none")]
    proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unread_count: Option<i64>,
}

fn map_feed(f: feed::Model, cat: Option<category::Model>) -> MfFeedDto {
    MfFeedDto {
        id: f.id,
        user_id: f.user_id,
        feed_url: f.feed_url,
        site_url: f.site_url,
        title: f.title,
        checked_at: f.checked_at.map(|d| d.to_rfc3339()),
        etag_header: f.etag,
        last_modified_header: f.last_modified,
        parsing_error_message: None,
        parsing_error_count: f.error_count,
        disabled: f.disabled,
        ignore_http_cache: false,
        allow_self_signed_certificates: f.allow_invalid_certs,
        fetch_via_proxy: f.fetch_via_proxy,
        scraper_rules: f.scraper_rules,
        rewrite_rules: f.rewrite_rules,
        urlrewrite_rules: f.url_rewrite_rules,
        blocklist_rules: f.blocklist_rules,
        keeplist_rules: f.keeplist_rules,
        block_filter_entry_rules: f.block_filter_entry_rules,
        keep_filter_entry_rules: f.keep_filter_entry_rules,
        crawler: false,
        user_agent: f.user_agent,
        cookie: f.cookies,
        username: None,
        password: None,
        category: cat.map(|c| MfCategoryDto {
            id: c.id,
            title: c.name,
        }),
        hide_globally: false,
        disable_http2: f.disable_http2,
        proxy_url: f.proxy_url,
        unread_count: None,
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct MfFeedsQuery {
    category_id: Option<i64>,
    disabled: Option<bool>,
    has_errors: Option<bool>,
    #[serde(rename = "withCounters")]
    with_counters: Option<bool>,
    order: Option<String>, // id|title|checked_at|updated_at|created_at|error_count
    direction: Option<String>, // asc|desc
}

async fn mf_list_feeds(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfFeedsQuery>,
) -> MfResult<Json<Vec<MfFeedDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let mut sel = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .find_also_related(Category);
    if let Some(cid) = q.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(d) = q.disabled {
        sel = sel.filter(feed::Column::Disabled.eq(d));
    }
    if let Some(e) = q.has_errors {
        sel = sel.filter(if e {
            feed::Column::ErrorCount.gt(0)
        } else {
            feed::Column::ErrorCount.eq(0)
        });
    }
    // ordering
    match q.order.as_deref() {
        Some("title") => {
            sel = if matches!(q.direction.as_deref(), Some("desc")) {
                sel.order_by_desc(feed::Column::Title)
            } else {
                sel.order_by_asc(feed::Column::Title)
            }
        }
        Some("checked_at") => {
            sel = if matches!(q.direction.as_deref(), Some("asc")) {
                sel.order_by_asc(feed::Column::CheckedAt)
            } else {
                sel.order_by_desc(feed::Column::CheckedAt)
            }
        }
        Some("updated_at") => {
            sel = if matches!(q.direction.as_deref(), Some("asc")) {
                sel.order_by_asc(feed::Column::UpdatedAt)
            } else {
                sel.order_by_desc(feed::Column::UpdatedAt)
            }
        }
        Some("created_at") => {
            sel = if matches!(q.direction.as_deref(), Some("asc")) {
                sel.order_by_asc(feed::Column::CreatedAt)
            } else {
                sel.order_by_desc(feed::Column::CreatedAt)
            }
        }
        Some("error_count") => {
            sel = if matches!(q.direction.as_deref(), Some("asc")) {
                sel.order_by_asc(feed::Column::ErrorCount)
            } else {
                sel.order_by_desc(feed::Column::ErrorCount)
            }
        }
        Some("id") => {
            sel = if matches!(q.direction.as_deref(), Some("desc")) {
                sel.order_by_desc(feed::Column::Id)
            } else {
                sel.order_by_asc(feed::Column::Id)
            }
        }
        _ => sel = sel.order_by_asc(feed::Column::Id),
    }

    let list = sel.all(&st.db).await.map_err(internal)?;
    let mut out: Vec<MfFeedDto> = list.into_iter().map(|(f, c)| map_feed(f, c)).collect();
    if q.with_counters.unwrap_or(false) {
        let feed_ids: Vec<i64> = out.iter().map(|d| d.id).collect();
        if !feed_ids.is_empty() {
            let pairs: Vec<(i64, i64)> = Entry::find()
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .filter(entry::Column::IsRead.eq(false))
                .select_only()
                .column(entry::Column::FeedId)
                .column_as(entry::Column::Id.count(), "cnt")
                .group_by(entry::Column::FeedId)
                .into_tuple()
                .all(&st.db)
                .await
                .map_err(internal)?;
            for d in &mut out {
                d.unread_count = pairs.iter().find(|(fid, _)| *fid == d.id).map(|p| p.1);
            }
        }
    }
    Ok(Json(out))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfCreateFeed {
    url: String,
    category_id: Option<i64>,
    title: Option<String>,
}

async fn mf_create_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateFeed>,
) -> MfResult<Json<MfFeedDto>> {
    let auth = mf_auth(&st, &headers).await?;
    if body.url.trim().is_empty() {
        return Err(bad_request("url required").into());
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = feed::ActiveModel {
        user_id: Set(auth.user_id),
        category_id: Set(body.category_id),
        r#type: Set(feed::FeedType::Rss),
        title: Set(body.title.clone()),
        site_url: Set(None),
        feed_url: Set(body.url.clone()),
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
    Ok(Json(map_feed(f, None)))
}

async fn mf_get_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfFeedDto>> {
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

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfUpdateFeed {
    category_id: Option<i64>,
    title: Option<String>,
    disabled: Option<bool>,
}

async fn mf_update_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateFeed>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = Feed::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let mut am: feed::ActiveModel = f.into();
    if body.category_id.is_some() {
        am.category_id = Set(body.category_id);
    }
    if body.title.is_some() {
        am.title = Set(body.title);
    }
    if let Some(v) = body.disabled {
        am.disabled = Set(v);
    }
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn mf_delete_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = Feed::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let am: feed::ActiveModel = f.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn mf_feed_mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    // 仅允许更新当前用户归属的订阅源
    let Some(_f) = Feed::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let _ = Entry::update_many()
        .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
        .filter(entry::Column::FeedId.eq(id))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok("ok")
}

async fn mf_feed_refresh(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<serde_json::Value>> {
    let _auth = mf_auth(&st, &headers).await?;
    let n = service::refresh_and_persist_by_id(&st.db, id)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"inserted": n})))
}

#[derive(serde::Deserialize)]
struct MfRefreshFeedsQuery {
    category_id: Option<i64>,
}

// 刷新当前用户所有（或某分类）订阅源
async fn mf_refresh_feeds(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfRefreshFeedsQuery>,
) -> MfResult<Json<serde_json::Value>> {
    let auth = mf_auth(&st, &headers).await?;
    let mut sel = Feed::find().filter(feed::Column::UserId.eq(auth.user_id));
    if let Some(cid) = q.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    let feeds: Vec<i64> = sel
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut enqueued = 0i64;
    for fid in feeds {
        // 去重：如已存在 pending/running 则跳过
        let exists = Job::find()
            .filter(job::Column::FeedId.eq(fid))
            .filter(job::Column::JobType.eq(job::JobType::FeedRefresh))
            .filter(
                job::Column::Status.is_in(vec![job::JobStatus::Pending, job::JobStatus::Running]),
            )
            .count(&st.db)
            .await
            .map_err(internal)?;
        if exists > 0 {
            continue;
        }
        let am = job::ActiveModel {
            user_id: Set(auth.user_id),
            feed_id: Set(Some(fid)),
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
        let _ = am.insert(&st.db).await.map_err(internal)?;
        enqueued += 1;
    }
    Ok(Json(json!({"enqueued": enqueued})))
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfEntriesQuery {
    status: Option<String>,
    feed_id: Option<i64>,
    category_id: Option<i64>,
    starred: Option<bool>,
    search: Option<String>,
    before_id: Option<i64>,
    after_id: Option<i64>,
    order: Option<String>,     // published_at | id
    direction: Option<String>, // asc | desc
    content: Option<bool>,     // include content_html when true (default true)
    limit: Option<u64>,
    offset: Option<u64>,
    // time filters (epoch seconds)
    published_before: Option<i64>,
    published_after: Option<i64>,
    changed_before: Option<i64>,
    changed_after: Option<i64>,
}

#[derive(serde::Serialize)]
pub(crate) struct MfEntryDto {
    id: i64,
    #[serde(rename = "published_at")]
    date: Option<String>,
    #[serde(rename = "changed_at")]
    changed_at: Option<String>,
    #[serde(rename = "created_at")]
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    feed: Option<MfFeedDto>,
    hash: Option<String>,
    url: Option<String>,
    #[serde(rename = "comments_url")]
    comments_url: Option<String>,
    title: Option<String>,
    status: String,
    content: Option<String>,
    author: Option<String>,
    #[serde(rename = "share_code")]
    share_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enclosures: Option<Vec<MfEnclosureDto>>,
    tags: Vec<String>,
    #[serde(rename = "reading_time")]
    reading_time: i32,
    #[serde(rename = "user_id")]
    user_id: i64,
    #[serde(rename = "feed_id")]
    feed_id: i64,
    starred: bool,
}

#[derive(serde::Serialize, Clone)]
pub(crate) struct MfEnclosureDto {
    id: i64,
    url: String,
    #[serde(rename = "mime_type")]
    mime_type: String,
    size: i64,
    #[serde(rename = "media_progression")]
    media_progression: i64,
}

#[derive(serde::Serialize)]
struct MfEntryResultSet {
    total: i64,
    entries: Vec<MfEntryDto>,
}

async fn mf_list_entries(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfEntriesQuery>,
) -> MfResult<Json<MfEntryResultSet>> {
    let auth = mf_auth(&st, &headers).await?;
    // 限定当前用户可见的 feed_id 集合（可选按 category 裁剪）
    let mut feed_sel = Feed::find().filter(feed::Column::UserId.eq(auth.user_id));
    if let Some(cid) = q.category_id {
        feed_sel = feed_sel.filter(feed::Column::CategoryId.eq(cid));
    }
    let feed_ids: Vec<i64> = feed_sel
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut sel = Entry::find().filter(entry::Column::FeedId.is_in(feed_ids));
    if let Some(fid) = q.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(ref s) = q.status {
        match s.as_str() {
            "unread" => sel = sel.filter(entry::Column::IsRead.eq(false)),
            "read" => sel = sel.filter(entry::Column::IsRead.eq(true)),
            "starred" => sel = sel.filter(entry::Column::IsStarred.eq(true)),
            _ => {}
        }
    }
    if let Some(star) = q.starred {
        sel = sel.filter(entry::Column::IsStarred.eq(star));
    }
    if let Some(ref k) = q.search {
        let like = format!("%{}%", k);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    if let Some(b) = q.before_id {
        sel = sel.filter(entry::Column::Id.lt(b));
    }
    if let Some(a) = q.after_id {
        sel = sel.filter(entry::Column::Id.gt(a));
    }
    // time-based filters
    if let Some(ts) = q.published_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::PublishedAt
                    .lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    if let Some(ts) = q.published_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::PublishedAt
                    .gte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    if let Some(ts) = q.changed_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::UpdatedAt.lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    if let Some(ts) = q.changed_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::UpdatedAt.gte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    // ordering
    match q.order.as_deref() {
        Some("id") => {
            if matches!(q.direction.as_deref(), Some("asc")) {
                sel = sel.order_by_asc(entry::Column::Id);
            } else {
                sel = sel.order_by_desc(entry::Column::Id);
            }
        }
        _ => {
            if matches!(q.direction.as_deref(), Some("asc")) {
                sel = sel
                    .order_by_asc(entry::Column::PublishedAt)
                    .order_by_asc(entry::Column::CreatedAt);
            } else {
                sel = sel
                    .order_by_desc(entry::Column::PublishedAt)
                    .order_by_desc(entry::Column::CreatedAt);
            }
        }
    }
    let limit = q.limit.unwrap_or(100).min(500);
    let offset = q.offset.unwrap_or(0);
    let count = sel.clone().count(&st.db).await.map_err(internal)? as i64;
    sel = sea_orm::QuerySelect::limit(sel, limit);
    sel = sea_orm::QuerySelect::offset(sel, offset);
    if let Some(o) = q.offset {
        sel = sea_orm::QuerySelect::offset(sel, o);
    }
    let include_content = q.content.unwrap_or(true);
    let wpm: usize = std::env::var("READ_SPEED_WPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .max(50) as usize;
    let rows = sel
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let entry_ids: Vec<i64> = rows.iter().map(|(e, _)| e.id).collect();
    let mut enc_map: std::collections::HashMap<i64, Vec<MfEnclosureDto>> =
        std::collections::HashMap::new();
    let mut tag_map: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    if !entry_ids.is_empty() {
        let encs = Enclosure::find()
            .filter(enclosure::Column::EntryId.is_in(entry_ids.clone()))
            .all(&st.db)
            .await
            .map_err(internal)?;
        for en in encs {
            let list = enc_map.entry(en.entry_id).or_default();
            list.push(MfEnclosureDto {
                id: en.id,
                url: en.url,
                mime_type: en.mime.clone().unwrap_or_default(),
                size: en.length.unwrap_or(0),
                media_progression: 0,
            });
        }
        // load labels as tags
        let pairs: Vec<(i64, String)> = EntryLabel::find()
            .join(
                sea_orm::JoinType::InnerJoin,
                entry_label::Relation::Label.def(),
            )
            .filter(entry_label::Column::EntryId.is_in(entry_ids.clone()))
            .filter(label::Column::UserId.eq(auth.user_id))
            .select_only()
            .column(entry_label::Column::EntryId)
            .column(label::Column::Name)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?;
        for (eid, name) in pairs {
            tag_map.entry(eid).or_default().push(name);
        }
    }
    let entries = rows
        .into_iter()
        .map(|(e, fopt)| {
            let status = if e.is_read { "read" } else { "unread" }.to_string();
            let feed_dto = fopt.map(|f| map_feed(f, None));
            let encs = enc_map.get(&e.id).cloned();
            let tags = tag_map.get(&e.id).cloned().unwrap_or_default();
            let reading_time = if include_content {
                let body = e
                    .content_html
                    .clone()
                    .or(e.summary.clone())
                    .unwrap_or_default();
                let text = strip_html(&body);
                let words = text.split_whitespace().count();
                std::cmp::max(1, (words + wpm - 1) / wpm) as i32
            } else {
                0
            };
            MfEntryDto {
                id: e.id,
                date: e.published_at.map(|d| d.to_rfc3339()),
                changed_at: Some(e.updated_at.to_rfc3339()),
                created_at: e.created_at.to_rfc3339(),
                feed: feed_dto,
                hash: e.hash,
                url: e.url,
                comments_url: None,
                title: e.title,
                status,
                content: if include_content {
                    e.content_html
                } else {
                    None
                },
                author: e.author,
                share_code: None,
                enclosures: encs,
                tags,
                reading_time,
                user_id: auth.user_id,
                feed_id: e.feed_id,
                starred: e.is_starred,
            }
        })
        .collect();
    Ok(Json(MfEntryResultSet {
        total: count,
        entries,
    }))
}

async fn mf_get_entry(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfEntryDto>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    let Some(f) = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    // collect tags
    let pairs: Vec<(i64, String)> = EntryLabel::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(entry_label::Column::EntryId.eq(id))
        .filter(label::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(entry_label::Column::EntryId)
        .column(label::Column::Name)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let tags: Vec<String> = pairs.into_iter().map(|(_, n)| n).collect();
    let include_content = true;
    let wpm: usize = std::env::var("READ_SPEED_WPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .max(50) as usize;
    let reading_time = if include_content {
        let body = e
            .content_html
            .clone()
            .or(e.summary.clone())
            .unwrap_or_default();
        let text = strip_html(&body);
        let words = text.split_whitespace().count();
        std::cmp::max(1, (words + wpm - 1) / wpm) as i32
    } else {
        0
    };
    let dto = MfEntryDto {
        id: e.id,
        date: e.published_at.map(|d| d.to_rfc3339()),
        changed_at: Some(e.updated_at.to_rfc3339()),
        created_at: e.created_at.to_rfc3339(),
        feed: Some(map_feed(f, None)),
        hash: e.hash,
        url: e.url,
        comments_url: None,
        title: e.title,
        status: if e.is_read {
            "read".into()
        } else {
            "unread".into()
        },
        content: e.content_html,
        author: e.author,
        share_code: None,
        enclosures: None,
        tags,
        reading_time,
        user_id: auth.user_id,
        feed_id: e.feed_id,
        starred: e.is_starred,
    };
    Ok(Json(dto))
}

#[derive(serde::Serialize)]
struct MfCatCounter {
    category_id: Option<i64>,
    unread: i64,
}

async fn mf_categories_counters(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfCatCounter>>> {
    let auth = mf_auth(&st, &headers).await?;
    // 统计分类未读：先按 feed 未读，再汇总到 category
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feed_ids: Vec<i64> = feeds.iter().map(|f| f.id).collect();
    if feed_ids.is_empty() {
        return Ok(Json(vec![]));
    }
    let pairs: Vec<(i64, i64)> = Entry::find()
        .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
        .filter(entry::Column::IsRead.eq(false))
        .select_only()
        .column(entry::Column::FeedId)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(entry::Column::FeedId)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    use std::collections::HashMap;
    let mut cat_map: HashMap<Option<i64>, i64> = HashMap::new();
    let feed_cat: HashMap<i64, Option<i64>> =
        feeds.into_iter().map(|f| (f.id, f.category_id)).collect();
    for (fid, cnt) in pairs {
        let cat = feed_cat.get(&fid).cloned().unwrap_or(None);
        *cat_map.entry(cat).or_insert(0) += cnt;
    }
    let out = cat_map
        .into_iter()
        .map(|(category_id, unread)| MfCatCounter {
            category_id,
            unread,
        })
        .collect();
    Ok(Json(out))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfUpdateEntriesBulk {
    entry_ids: Vec<i64>,
    status: String,
}

async fn mf_update_entries_bulk(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfUpdateEntriesBulk>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    if body.entry_ids.is_empty() {
        return Ok("ok");
    }
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut upd = Entry::update_many()
        .filter(entry::Column::Id.is_in(body.entry_ids.clone()))
        .filter(entry::Column::FeedId.is_in(feed_ids));
    match body.status.as_str() {
        "read" => {
            upd = upd.col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true));
        }
        "unread" => {
            upd = upd.col_expr(
                entry::Column::IsRead,
                sea_orm::sea_query::Expr::value(false),
            );
        }
        "star" | "bookmark" => {
            upd = upd.col_expr(
                entry::Column::IsStarred,
                sea_orm::sea_query::Expr::value(true),
            );
        }
        "unstar" | "unbookmark" => {
            upd = upd.col_expr(
                entry::Column::IsStarred,
                sea_orm::sea_query::Expr::value(false),
            );
        }
        _ => {}
    }
    let _ = upd.exec(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(serde::Deserialize)]
pub(crate) struct MfUpdateEntry {
    status: String,
}

async fn mf_update_entry(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateEntry>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    let owned = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let mut am: entry::ActiveModel = e.into();
    match body.status.as_str() {
        "read" => am.is_read = Set(true),
        "unread" => am.is_read = Set(false),
        "star" | "bookmark" => am.is_starred = Set(true),
        "unstar" | "unbookmark" => am.is_starred = Set(false),
        _ => {}
    }
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn mf_toggle_star(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    let owned = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let new_star = !e.is_starred;
    let mut am: entry::ActiveModel = e.into();
    am.is_starred = Set(new_star);
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn mf_save_entry(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    // 验证归属
    let owned = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    // 简单持久化：在 extras_json 写入 saved 标记与时间戳
    let now = Utc::now()
        .with_timezone(&FixedOffset::east_opt(0).unwrap())
        .to_rfc3339();
    let extras = json!({ "saved": true, "saved_at": now });
    let mut am: entry::ActiveModel = e.into();
    am.extras_json = Set(Some(extras));
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(serde::Serialize)]
struct MfEntryContentResp {
    content: String,
}

async fn mf_fetch_entry_content(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfEntryContentResp>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    let owned = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let content = e
        .content_html
        .unwrap_or_else(|| e.summary.unwrap_or_default());
    Ok(Json(MfEntryContentResp { content }))
}

async fn mf_flush_history(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    // 策略：删除当前用户已读且未加星的旧条目（updated_at 早于阈值）
    let days: i64 = std::env::var("FLUSH_HISTORY_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
        .max(1);
    let cutoff =
        Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()) - chrono::Duration::days(days);
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if feed_ids.is_empty() {
        return Ok("ok");
    }
    let _ = Entry::delete_many()
        .filter(entry::Column::FeedId.is_in(feed_ids))
        .filter(entry::Column::IsRead.eq(true))
        .filter(entry::Column::IsStarred.eq(false))
        .filter(entry::Column::UpdatedAt.lte(cutoff))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok("ok")
}

// duplicates removed; see single implementation below

async fn mf_icon(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let _auth = mf_auth(&st, &headers).await?;
    use captura_storage::entity::favicon as fv;
    let Some(v) = fv::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("icon").into());
    };
    let bytes = v.data.unwrap_or_default();
    let mime = v.mime.unwrap_or_else(|| "image/x-icon".into());
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(&mime).unwrap_or(axum::http::HeaderValue::from_static(
            "application/octet-stream",
        )),
    );
    Ok((headers, bytes).into_response())
}

// 通过 Feed ID 获取该订阅源的 icon 数据
async fn mf_feed_icon(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await?;
    use captura_storage::entity::favicon as fv;
    let Some(f) = Feed::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let Some(fid) = f.favicon_id else {
        return Err(not_found("icon").into());
    };
    let Some(v) = fv::Entity::find_by_id(fid)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("icon").into());
    };
    let bytes = v.data.unwrap_or_default();
    let mime = v.mime.unwrap_or_else(|| "image/x-icon".into());
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(&mime).unwrap_or(axum::http::HeaderValue::from_static(
            "application/octet-stream",
        )),
    );
    Ok((headers, bytes).into_response())
}

#[derive(serde::Serialize)]
struct MfEnclosureDtoFull {
    id: i64,
    #[serde(rename = "user_id")]
    user_id: i64,
    #[serde(rename = "entry_id")]
    entry_id: i64,
    url: String,
    #[serde(rename = "mime_type")]
    mime_type: String,
    size: i64,
    #[serde(rename = "media_progression")]
    media_progression: i64,
}

async fn mf_get_enclosure(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfEnclosureDtoFull>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(en) = Enclosure::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("enclosure").into());
    };
    // 验证归属
    let owned = Entry::find_by_id(en.entry_id)
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("enclosure").into());
    }
    Ok(Json(MfEnclosureDtoFull {
        id: en.id,
        user_id: auth.user_id,
        entry_id: en.entry_id,
        url: en.url,
        mime_type: en.mime.unwrap_or_default(),
        size: en.length.unwrap_or(0),
        media_progression: en.media_progression.unwrap_or(0),
    }))
}

#[derive(serde::Deserialize)]
struct MfEnclosureUpdate {
    #[serde(rename = "media_progression")]
    media_progression: i64,
}

async fn mf_update_enclosure(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfEnclosureUpdate>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(en) = Enclosure::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("enclosure").into());
    };
    // 验证归属
    let owned = Entry::find_by_id(en.entry_id)
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("enclosure").into());
    }
    let mut am: enclosure::ActiveModel = en.into();
    am.media_progression = Set(Some(body.media_progression));
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(serde::Serialize)]
struct MfFeedCountersResp {
    reads: std::collections::HashMap<i64, i64>,
    unreads: std::collections::HashMap<i64, i64>,
}

async fn mf_feeds_counters(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<MfFeedCountersResp>> {
    let auth = mf_auth(&st, &headers).await?;
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut reads = std::collections::HashMap::new();
    let mut unreads = std::collections::HashMap::new();
    if feed_ids.is_empty() {
        return Ok(Json(MfFeedCountersResp { reads, unreads }));
    }
    let unread_pairs: Vec<(i64, i64)> = Entry::find()
        .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
        .filter(entry::Column::IsRead.eq(false))
        .select_only()
        .column(entry::Column::FeedId)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(entry::Column::FeedId)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    for (fid, cnt) in unread_pairs {
        unreads.insert(fid, cnt);
    }
    let read_pairs: Vec<(i64, i64)> = Entry::find()
        .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
        .filter(entry::Column::IsRead.eq(true))
        .select_only()
        .column(entry::Column::FeedId)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(entry::Column::FeedId)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    for (fid, cnt) in read_pairs {
        reads.insert(fid, cnt);
    }
    Ok(Json(MfFeedCountersResp { reads, unreads }))
}

#[derive(serde::Serialize)]
struct MfVersionResp {
    version: String,
    commit: String,
    build_date: String,
    go_version: String,
    compiler: String,
    arch: String,
    os: String,
}

async fn mf_version(
    State(_st): State<AppState>,
    _headers: axum::http::HeaderMap,
) -> MfResult<Json<MfVersionResp>> {
    // 提供最小版本信息对齐 Miniflux client，非 Go 实现字段留空或使用 Rust 对应
    Ok(Json(MfVersionResp {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: String::new(),
        build_date: String::new(),
        go_version: String::new(),
        compiler: String::from("rustc"),
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
    }))
}

async fn mf_feed_entries(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(mut q): Query<MfEntriesQuery>,
) -> MfResult<Json<MfEntryResultSet>> {
    q.feed_id = Some(id);
    mf_list_entries(State(st), headers, Query(q)).await
}

async fn mf_category_entries(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(mut q): Query<MfEntriesQuery>,
) -> MfResult<Json<MfEntryResultSet>> {
    q.category_id = Some(id);
    mf_list_entries(State(st), headers, Query(q)).await
}

async fn mf_category_feeds(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<Vec<MfFeedDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let list = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .find_also_related(Category)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter().map(|(f, c)| map_feed(f, c)).collect(),
    ))
}

async fn mf_category_refresh(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    for f in feeds {
        let _ = service::refresh_and_persist(&st.db, &f).await;
    }
    Ok("ok")
}

#[derive(serde::Deserialize)]
struct MfUpdateCategory {
    title: String,
}

async fn mf_update_category(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateCategory>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    if body.title.trim().is_empty() {
        return Err(bad_request("title required").into());
    }
    let Some(cat) = Category::find_by_id(id)
        .filter(category::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category").into());
    };
    let mut am: category::ActiveModel = cat.into();
    am.name = Set(body.title);
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

async fn mf_delete_category(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let _ = Feed::update_many()
        .col_expr(
            feed::Column::CategoryId,
            sea_orm::sea_query::Expr::value(Option::<i64>::None),
        )
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    if let Some(cat) = Category::find_by_id(id)
        .filter(category::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let am: category::ActiveModel = cat.into();
        let _ = am.delete(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

async fn mf_user_mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    if auth.user_id != id {
        return Err(not_found("user").into());
    }
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !feed_ids.is_empty() {
        let _ = Entry::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .filter(entry::Column::FeedId.is_in(feed_ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}

#[derive(serde::Deserialize)]
struct MfSetTagsReq {
    tags: Vec<String>,
}

// 为条目添加标签（按名称，若不存在则创建）
async fn mf_add_entry_tags(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfSetTagsReq>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    // 校验归属
    let owned = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }

    // 预处理标签名：去空白、去重
    let mut names: Vec<String> = body
        .tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok("ok");
    }

    // 查询已存在的 label
    let existing: Vec<(i64, String)> = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.is_in(names.clone()))
        .select_only()
        .column(label::Column::Id)
        .column(label::Column::Name)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut name_to_id: std::collections::HashMap<String, i64> =
        existing.into_iter().map(|(id, n)| (n, id)).collect();
    // 创建缺失的 label
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let missing: Vec<String> = names
        .iter()
        .filter(|n| !name_to_id.contains_key(*n))
        .cloned()
        .collect();
    for n in missing {
        let am = label::ActiveModel {
            user_id: Set(auth.user_id),
            name: Set(n.clone()),
            color: Set(None),
            created_at: Set(now),
            ..Default::default()
        };
        let l = am.insert(&st.db).await.map_err(internal)?;
        name_to_id.insert(n, l.id);
    }
    // 建立 entry_label 关联（若不存在）
    let label_ids: Vec<i64> = names
        .iter()
        .filter_map(|n| name_to_id.get(n).copied())
        .collect();
    if !label_ids.is_empty() {
        // 查询现有关联
        let existing_pairs: Vec<i64> = EntryLabel::find()
            .filter(entry_label::Column::EntryId.eq(id))
            .filter(entry_label::Column::LabelId.is_in(label_ids.clone()))
            .select_only()
            .column(entry_label::Column::LabelId)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?;
        let exist_set: std::collections::HashSet<i64> = existing_pairs.into_iter().collect();
        for lid in label_ids.into_iter().filter(|lid| !exist_set.contains(lid)) {
            let am = entry_label::ActiveModel {
                entry_id: Set(id),
                label_id: Set(lid),
                ..Default::default()
            };
            let _ = am.insert(&st.db).await.map_err(internal)?;
        }
    }
    Ok("ok")
}

// 为条目移除标签（按名称）
async fn mf_remove_entry_tags(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfSetTagsReq>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? else {
        return Err(not_found("entry").into());
    };
    // 校验归属
    let owned = Feed::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let mut names: Vec<String> = body
        .tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok("ok");
    }
    let label_ids: Vec<i64> = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.is_in(names))
        .select_only()
        .column(label::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !label_ids.is_empty() {
        let _ = EntryLabel::delete_many()
            .filter(entry_label::Column::EntryId.eq(id))
            .filter(entry_label::Column::LabelId.is_in(label_ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}

#[derive(serde::Serialize)]
struct MfApiKeyDto {
    id: i64,
    #[serde(rename = "user_id")]
    user_id: i64,
    token: String,
    description: Option<String>,
    #[serde(rename = "last_used_at")]
    last_used_at: Option<String>,
    #[serde(rename = "created_at")]
    created_at: String,
}

#[derive(serde::Deserialize)]
struct MfCreateApiKeyReq {
    description: Option<String>,
}

async fn mf_api_keys(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfApiKeyDto>>> {
    let auth = mf_auth(&st, &headers).await?;
    let keys = Token::find()
        .filter(token::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut list = Vec::new();
    for k in keys {
        list.push(MfApiKeyDto {
            id: k.id,
            user_id: k.user_id,
            token: k.token_plain.unwrap_or_default(),
            description: k.name,
            last_used_at: k.last_used_at.map(|d| d.to_rfc3339()),
            created_at: k.created_at.to_rfc3339(),
        });
    }
    Ok(Json(list))
}

async fn mf_create_api_key(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateApiKeyReq>,
) -> MfResult<Json<MfApiKeyDto>> {
    let auth = mf_auth(&st, &headers).await?;
    let mut rand_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(body.description.clone()),
        token_hash: Set(token_hash),
        token_plain: Set(Some(token_str.clone())),
        created_at: Set(now),
        last_used_at: Set(None),
        expires_at: Set(None),
        ..Default::default()
    };
    let k = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfApiKeyDto {
        id: k.id,
        user_id: auth.user_id,
        token: token_str,
        description: k.name,
        last_used_at: None,
        created_at: k.created_at.to_rfc3339(),
    }))
}

async fn mf_delete_api_key(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    if let Some(k) = Token::find_by_id(id)
        .filter(token::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let am: token::ActiveModel = k.into();
        let _ = am.delete(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(ch)
                }
            }
        }
    }
    out
}

async fn mf_category_mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !feed_ids.is_empty() {
        let _ = Entry::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .filter(entry::Column::FeedId.is_in(feed_ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}

async fn mf_export(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<(axum::http::HeaderMap, String)> {
    let auth = mf_auth(&st, &headers).await?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n<head><title>Captura Export</title></head>\n<body>\n");
    for f in feeds.iter().filter(|f| f.category_id.is_none()) {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
            crate::opml::xml_escape(f.title.as_deref().unwrap_or("")),
            crate::opml::xml_escape(f.title.as_deref().unwrap_or("")),
            crate::opml::xml_escape(&f.feed_url),
            crate::opml::xml_escape(f.site_url.as_deref().unwrap_or(""))
        ));
    }
    for c in cats {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\">\n",
            crate::opml::xml_escape(&c.name),
            crate::opml::xml_escape(&c.name)
        ));
        for f in feeds.iter().filter(|f| f.category_id == Some(c.id)) {
            buf.push_str(&format!(
                "  <outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
                crate::opml::xml_escape(f.title.as_deref().unwrap_or("")),
                crate::opml::xml_escape(f.title.as_deref().unwrap_or("")),
                crate::opml::xml_escape(&f.feed_url),
                crate::opml::xml_escape(f.site_url.as_deref().unwrap_or(""))
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

async fn mf_import(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    // 直接复用 opml::import 逻辑（简化：在此实现重复逻辑）
    const MAX_OPML_BYTES: usize = 2_000_000;
    if body.len() > MAX_OPML_BYTES {
        return Err(bad_request("OPML too large").into());
    }
    let outlines = crate::opml::parse_opml_quickxml(&body)
        .unwrap_or_else(|_| crate::opml::extract_outlines(&body));
    const MAX_OUTLINES: usize = 2000;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut cat_map: std::collections::HashMap<String, i64> = Category::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|c| (c.name.clone(), c.id))
        .collect();
    for node in outlines.into_iter().take(MAX_OUTLINES) {
        match node {
            crate::opml::OutlineNode::Feed {
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
                            user_id: Set(auth.user_id),
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
                    .filter(feed::Column::UserId.eq(auth.user_id))
                    .filter(feed::Column::FeedUrl.eq(&xml_url))
                    .one(&st.db)
                    .await
                    .map_err(internal)?;
                if dup.is_some() {
                    continue;
                }
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
            crate::opml::OutlineNode::Category { .. } => {}
        }
    }
    Ok("ok")
}

#[derive(serde::Deserialize)]
struct MfDiscoverReq {
    url: String,
}

#[derive(serde::Serialize)]
struct MfSubscriptionDto {
    title: String,
    url: String,
    r#type: String,
}

fn push_candidate(
    list: &mut Vec<MfSubscriptionDto>,
    seen: &mut std::collections::HashSet<String>,
    title: String,
    url: String,
    typ: &str,
) {
    if seen.insert(url.clone()) {
        list.push(MfSubscriptionDto {
            title,
            url,
            r#type: typ.to_string(),
        });
    }
}

#[derive(serde::Deserialize, Default)]
struct MfDiscoverQuery {
    verify: Option<bool>,
}

async fn mf_discover(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfDiscoverQuery>,
    Json(body): Json<MfDiscoverReq>,
) -> MfResult<Json<Vec<MfSubscriptionDto>>> {
    let _auth = mf_auth(&st, &headers).await?;
    // 校验 URL
    let base = Url::parse(&body.url).map_err(|_| bad_request("invalid url"))?;
    let client = reqwest::Client::builder()
        .user_agent("captura-discover/0.1")
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(internal)?;
    let resp = client.get(base.clone()).send().await.map_err(internal)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(not_found("unreachable").into());
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let body_text = resp.text().await.map_err(internal)?;

    let mut list: Vec<MfSubscriptionDto> = Vec::new();
    // Helper: push if URL not seen
    let mut seen = std::collections::HashSet::new();

    // 若内容看起来就是 feed，本身加入
    let lower = body_text[..std::cmp::min(body_text.len(), 8192)].to_ascii_lowercase();
    if content_type.contains("xml") || lower.contains("<rss") || lower.contains("<feed") {
        push_candidate(
            &mut list,
            &mut seen,
            base.as_str().to_string(),
            base.as_str().to_string(),
            if lower.contains("<rss") {
                "rss"
            } else {
                "atom"
            },
        );
    }
    if content_type.contains("json") || lower.contains("jsonfeed.org/version") {
        push_candidate(
            &mut list,
            &mut seen,
            base.as_str().to_string(),
            base.as_str().to_string(),
            "json",
        );
    }

    // HTML 中的 link rel="alternate"
    if content_type.contains("html") || (!content_type.contains("xml") && lower.contains("<html")) {
        let doc = Html::parse_document(&body_text);
        let sel = Selector::parse("link[rel]").unwrap();
        for el in doc.select(&sel) {
            let rel = el.value().attr("rel").unwrap_or("").to_ascii_lowercase();
            if !rel.contains("alternate") {
                continue;
            }
            let typ = el.value().attr("type").unwrap_or("").to_ascii_lowercase();
            let href = match el.value().attr("href") {
                Some(h) if !h.is_empty() => h,
                _ => continue,
            };
            let Ok(abs) = base.join(href) else { continue };
            let title = el.value().attr("title").unwrap_or("");
            if typ.contains("rss") || typ.contains("xml") {
                push_candidate(
                    &mut list,
                    &mut seen,
                    title.to_string().if_empty(abs.as_str()),
                    abs.to_string(),
                    "rss",
                );
            } else if typ.contains("atom") {
                push_candidate(
                    &mut list,
                    &mut seen,
                    title.to_string().if_empty(abs.as_str()),
                    abs.to_string(),
                    "atom",
                );
            } else if typ.contains("json") {
                push_candidate(
                    &mut list,
                    &mut seen,
                    title.to_string().if_empty(abs.as_str()),
                    abs.to_string(),
                    "json",
                );
            } else if typ.is_empty() {
                // 无 type，按后缀/路径启发式判断
                let href_l = abs.as_str().to_ascii_lowercase();
                let guess = if href_l.ends_with(".xml")
                    || href_l.contains("/feed")
                    || href_l.contains("/rss")
                {
                    Some("rss")
                } else if href_l.ends_with(".atom") || href_l.contains("/atom") {
                    Some("atom")
                } else if href_l.ends_with(".json") {
                    Some("json")
                } else {
                    None
                };
                if let Some(t) = guess {
                    push_candidate(
                        &mut list,
                        &mut seen,
                        title.to_string().if_empty(abs.as_str()),
                        abs.to_string(),
                        t,
                    );
                }
            }
        }
        // 如果还没有命中，尝试常见路径猜测（不发起网络请求，仅给出候选）
        let should_guess = list.is_empty();
        if should_guess {
            for suffix in [
                "/feed",
                "/feed.xml",
                "/rss",
                "/index.xml",
                "/atom.xml",
                "/feed.json",
            ] {
                if let Ok(abs) = base.join(suffix) {
                    let href_l = abs.as_str().to_ascii_lowercase();
                    let t = if href_l.ends_with(".json") {
                        "json"
                    } else if href_l.contains("atom") {
                        "atom"
                    } else {
                        "rss"
                    };
                    push_candidate(
                        &mut list,
                        &mut seen,
                        abs.as_str().to_string(),
                        abs.as_str().to_string(),
                        t,
                    );
                }
            }
        }
    }

    if list.is_empty() {
        return Err(not_found("no_subscription").into());
    }

    if q.verify.unwrap_or(false) {
        // 顺序校验候选有效性，保留 2xx 的 URL（最多检查前 10 项）
        let mut verified: Vec<MfSubscriptionDto> = Vec::new();
        for cand in list.into_iter().take(10) {
            let resp = client.head(&cand.url).send().await;
            let ok = match resp {
                Ok(r) if r.status().is_success() => true,
                _ => {
                    // fallback GET (部分站点不支持 HEAD)
                    match client.get(&cand.url).send().await {
                        Ok(r) if r.status().is_success() => true,
                        _ => false,
                    }
                }
            };
            if ok {
                verified.push(cand);
            }
        }
        if verified.is_empty() {
            return Err(not_found("no_subscription").into());
        }
        return Ok(Json(verified));
    }

    Ok(Json(list))
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[derive(serde::Serialize)]
struct MfIntegrationsStatus {
    #[serde(rename = "has_integrations")]
    has_integrations: bool,
}

async fn mf_integrations_status(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<MfIntegrationsStatus>> {
    let _auth = mf_auth(&st, &headers).await?;
    // 先返回 false；后续可按配置探测集成
    Ok(Json(MfIntegrationsStatus {
        has_integrations: false,
    }))
}

#[derive(serde::Serialize)]
struct MfTag {
    title: String,
    count: i64,
}

async fn mf_list_tags(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfTag>>> {
    let auth = mf_auth(&st, &headers).await?;
    // 先取所有标签，再做计数映射补零
    let labels = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .order_by_asc(label::Column::Name)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let counts: Vec<(i64, i64)> = EntryLabel::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(label::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(entry_label::Column::LabelId)
        .column_as(entry_label::Column::Id.count(), "cnt")
        .group_by(entry_label::Column::LabelId)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut cnt_map = std::collections::HashMap::new();
    for (lid, cnt) in counts {
        cnt_map.insert(lid, cnt);
    }
    let out = labels
        .into_iter()
        .map(|l| MfTag {
            title: l.name,
            count: *cnt_map.get(&l.id).unwrap_or(&0),
        })
        .collect();
    Ok(Json(out))
}

#[derive(serde::Deserialize)]
struct MfCreateTagReq {
    title: String,
    color: Option<String>,
}

async fn mf_create_tag(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateTagReq>,
) -> MfResult<Json<MfTag>> {
    let auth = mf_auth(&st, &headers).await?;
    let name = body.title.trim();
    if name.is_empty() {
        return Err(bad_request("title required").into());
    }
    // 存在即返回（幂等）
    if let Some(l) = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        return Ok(Json(MfTag {
            title: l.name,
            count: 0,
        }));
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = label::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(name.to_string()),
        color: Set(body.color.clone()),
        created_at: Set(now),
        ..Default::default()
    };
    let l = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfTag {
        title: l.name,
        count: 0,
    }))
}

async fn mf_delete_tag(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(l) = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name.as_str()))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("tag").into());
    };
    // 先删除 entry_label 关联，再删标签
    let _ = EntryLabel::delete_many()
        .filter(entry_label::Column::LabelId.eq(l.id))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    let am: label::ActiveModel = l.into();
    let _ = am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(serde::Deserialize)]
struct MfRenameTagReq {
    title: String,
    color: Option<String>,
}

async fn mf_rename_tag(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<MfRenameTagReq>,
) -> MfResult<Json<MfTag>> {
    let auth = mf_auth(&st, &headers).await?;
    let new_name = body.title.trim();
    if new_name.is_empty() {
        return Err(bad_request("title required").into());
    }
    let Some(l) = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name.as_str()))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("tag").into());
    };
    // 若新名已被占用，返回 400
    if let Some(existing) = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(new_name))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        if existing.id != l.id {
            return Err(bad_request("tag already exists").into());
        }
    }
    let mut am: label::ActiveModel = l.into();
    am.name = Set(new_name.to_string());
    if body.color.is_some() {
        am.color = Set(body.color.clone());
    }
    let l = am.update(&st.db).await.map_err(internal)?;
    // 计数
    let cnt = EntryLabel::find()
        .filter(entry_label::Column::LabelId.eq(l.id))
        .count(&st.db)
        .await
        .map_err(internal)? as i64;
    Ok(Json(MfTag {
        title: l.name,
        count: cnt,
    }))
}

// -----------------------------
// HTTP 级最小集成测试（sqlite::memory + migration）
// -----------------------------
#[cfg(test)]
mod it {
    use super::*;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::util::ServiceExt;

    async fn setup_db() -> sea_orm::DatabaseConnection {
        captura_testkit::setup_db().await
    }

    async fn seed_user_and_token(db: &sea_orm::DatabaseConnection) -> String {
        let (_uid, token) = captura_testkit::seed_user_and_token(db, "u").await;
        token
    }

    async fn json_body(resp: axum::response::Response) -> serde_json::Value {
        let status = resp.status();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        if status == StatusCode::NO_CONTENT {
            return serde_json::Value::Null;
        }
        serde_json::from_slice(&body).unwrap_or_else(|_| serde_json::Value::Null)
    }

    #[tokio::test]
    async fn me_and_tags_flow() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        // feed + entry
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            url: Set(Some("https://example.com/1".into())),
            title: Set(Some("hello".into())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });

        // GET /v1/me
        let resp = app
            .clone()
            .oneshot(
                Request::get("/me")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // POST /v1/entries/:id/tags
        let resp = app
            .clone()
            .oneshot(
                Request::post(format!("/entries/{}/tags", e.id))
                    .header("X-Auth-Token", token.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        json!({"tags":["t1","t2"]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // GET /v1/tags
        let resp = app
            .clone()
            .oneshot(
                Request::get("/tags")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.is_array());

        // PUT /v1/tags/:name
        let resp = app
            .clone()
            .oneshot(
                Request::put("/tags/t1")
                    .header("X-Auth-Token", token.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(json!({"title":"t3"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // DELETE /v1/tags/:name
        let resp = app
            .oneshot(
                Request::delete("/tags/t2")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn entries_filters_basic() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let e1 = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g1".into())),
            title: Set(Some("hello".into())),
            is_read: Set(false),
            is_starred: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _e2 = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g2".into())),
            title: Set(Some("world".into())),
            is_read: Set(true),
            is_starred: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });
        // unread only
        let resp = app
            .clone()
            .oneshot(
                Request::get("/entries?status=unread&limit=10")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let j = json_body(resp).await;
        if status != StatusCode::OK {
            panic!("/v1/entries returned {}: {}", status, j);
        }
        assert_eq!(j["total"].as_i64().unwrap_or(-1), 1);
        assert_eq!(j["entries"][0]["id"].as_i64().unwrap_or(-1), e1.id);
    }

    #[tokio::test]
    async fn icon_binary_and_bookmark_alias() {
        use captura_storage::entity::favicon as fv;
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        // feed + entry + favicon
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            title: Set(Some("x".into())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let fav = fv::ActiveModel {
            feed_id: Set(Some(f.id)),
            url: Set(Some("https://example.com/favicon.ico".into())),
            mime: Set(Some("image/x-icon".into())),
            data: Set(Some(vec![7, 8, 9])),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let mut fm: feed::ActiveModel = f.into();
        fm.favicon_id = Set(Some(fav.id));
        let f = fm.update(&db).await.unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });

        // GET /v1/feeds/:id/icon binary
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/feeds/{}/icon", f.id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("image/"));
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.to_vec(), vec![7, 8, 9]);

        // PUT /v1/entries/:id/bookmark alias
        let resp = app
            .clone()
            .oneshot(
                Request::put(format!("/entries/{}/bookmark", e.id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // GET /v1/entries/:id and check starred
        let resp = app
            .oneshot(
                Request::get(format!("/entries/{}", e.id))
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert_eq!(j["id"].as_i64().unwrap_or(-1), e.id);
        assert!(j["starred"].as_bool().unwrap_or(false));
    }

    #[tokio::test]
    async fn feeds_counters_basic() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        // two feeds
        let f1 = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("f1".into())),
            site_url: Set(Some("https://a".into())),
            feed_url: Set("https://a/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let f2 = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("f2".into())),
            site_url: Set(Some("https://b".into())),
            feed_url: Set("https://b/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        // entries: f1 -> 1 read + 1 unread; f2 -> 1 unread
        let _ = entry::ActiveModel {
            feed_id: Set(f1.id),
            guid: Set(Some("g1".into())),
            title: Set(Some("e1".into())),
            is_read: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _ = entry::ActiveModel {
            feed_id: Set(f1.id),
            guid: Set(Some("g2".into())),
            title: Set(Some("e2".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _ = entry::ActiveModel {
            feed_id: Set(f2.id),
            guid: Set(Some("g3".into())),
            title: Set(Some("e3".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let app = router().with_state(crate::AppState { db: db.clone() });
        let resp = app
            .oneshot(
                Request::get("/feeds/counters")
                    .header("X-Auth-Token", token.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        // reads[f1]=1; unreads[f1]=1; unreads[f2]=1
        assert_eq!(j["reads"][f1.id.to_string()].as_i64().unwrap_or(-1), 1);
        assert_eq!(j["unreads"][f1.id.to_string()].as_i64().unwrap_or(-1), 1);
        assert_eq!(j["unreads"][f2.id.to_string()].as_i64().unwrap_or(-1), 1);
    }

    #[tokio::test]
    async fn opml_export_import_roundtrip() {
        // source db
        let db1 = setup_db().await;
        let token1 = seed_user_and_token(&db1).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let c = category::ActiveModel {
            user_id: Set(1),
            name: Set("cat".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&db1)
        .await
        .unwrap();
        let _ = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(Some(c.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("title".into())),
            site_url: Set(Some("https://site".into())),
            feed_url: Set("https://site/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db1)
        .await
        .unwrap();
        let app1 = router().with_state(crate::AppState { db: db1.clone() });
        let resp = app1
            .clone()
            .oneshot(
                Request::get("/export")
                    .header("X-Auth-Token", token1.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let xml = String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(xml.starts_with("<?xml"));

        // target db
        let db2 = setup_db().await;
        let token2 = seed_user_and_token(&db2).await;
        let app2 = router().with_state(crate::AppState { db: db2.clone() });
        // import XML
        let resp = app2
            .clone()
            .oneshot(
                Request::post("/import")
                    .header("X-Auth-Token", token2.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/xml")
                    .body(axum::body::Body::from(xml.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // list feeds
        let resp = app2
            .oneshot(
                Request::get("/feeds")
                    .header("X-Auth-Token", token2.as_str())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.is_array());
        assert_eq!(j.as_array().unwrap().len(), 1);
        assert_eq!(j[0]["feed_url"].as_str().unwrap_or(""), "https://site/feed");
    }

    #[tokio::test]
    async fn discover_local_html() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        // start local server serving HTML with rel=alternate rss
        let app_site = axum::Router::new()
            .route(
                "/",
                axum::routing::get(|| async {
                    axum::http::Response::builder()
                        .header(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                        .body(
                            "<html><head><link rel=\"alternate\" type=\"application/rss+xml\" title=\"Site RSS\" href=\"/feed.xml\"></head><body>ok</body></html>"
                                .to_string(),
                        )
                        .unwrap()
                }),
            )
            .route(
                "/feed.xml",
                axum::routing::get(|| async {
                    axum::http::Response::builder()
                        .header(axum::http::header::CONTENT_TYPE, "application/rss+xml")
                        .body("<?xml version=\"1.0\"?><rss></rss>".to_string())
                        .unwrap()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app_site).await;
        });

        // call /v1/discover
        let app = router().with_state(crate::AppState { db: db.clone() });
        let url = format!("http://{}:{}", addr.ip(), addr.port());
        let body = serde_json::json!({"url": url});
        let resp = app
            .oneshot(
                Request::post("/discover")
                    .header("X-Auth-Token", token.as_str())
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let list = json_body(resp).await;
        assert!(list.is_array());
        assert!(!list.as_array().unwrap().is_empty());
        // ensure discovered feed URL is absolute to /feed.xml
        let first_url = list[0]["url"].as_str().unwrap_or("");
        assert!(first_url.ends_with("/feed.xml"));
    }

    #[tokio::test]
    async fn fever_flow_basic() {
        use captura_storage::entity::user;
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        // set fever key for user 1
        let mut u: user::ActiveModel = user::Model {
            id: 1,
            username: "u".into(),
            password_hash: "h".into(),
            fever_key_md5: None,
            created_at: Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()),
        }
        .into();
        u.fever_key_md5 = Set(Some(format!("{:x}", md5::Md5::digest(b"u:fever"))));
        let _ = u
            .update(&db)
            .await
            .unwrap_or_else(|_| panic!("failed to set fever key"));
        // seed feed and entry
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let c = category::ActiveModel {
            user_id: Set(1),
            name: Set("news".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(Some(c.id)),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example".into())),
            feed_url: Set("https://example/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            title: Set(Some("hi".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        // build app with fever route
        let app = Router::new()
            .merge(super::router())
            .route(
                "/fever",
                axum::routing::get(crate::compat::fever::endpoint)
                    .post(crate::compat::fever::endpoint),
            )
            .with_state(crate::AppState { db: db.clone() });

        let key = format!("{:x}", md5::Md5::digest(b"u:fever"));
        // groups & feeds
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/fever?api_key={}&groups=1&feeds=1", key))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.get("groups").is_some());
        assert!(j.get("feeds").is_some());

        // items & unread ids
        let resp = app
            .oneshot(
                Request::get(format!(
                    "/fever?api_key={}&items=1&since_id=0&limit=50&unread_item_ids=1",
                    key
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.get("items").is_some());
        assert!(j.get("unread_item_ids").is_some());
    }

    #[tokio::test]
    async fn reader_unread_count_basic() {
        let db = setup_db().await;
        let token = seed_user_and_token(&db).await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let f = feed::ActiveModel {
            user_id: Set(1),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(Some("t".into())),
            site_url: Set(Some("https://example.com".into())),
            feed_url: Set("https://example.com/feed".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let _e = entry::ActiveModel {
            feed_id: Set(f.id),
            guid: Set(Some("g".into())),
            title: Set(Some("hello".into())),
            is_read: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        // app with reader routes
        let app = Router::new()
            .route(
                "/reader/api/0/unread-count",
                axum::routing::get(crate::compat::reader::unread_count),
            )
            .with_state(crate::AppState { db: db.clone() });
        let resp = app
            .oneshot(
                Request::get("/reader/api/0/unread-count")
                    .header(
                        axum::http::header::AUTHORIZATION,
                        format!("Bearer {}", token),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let j = json_body(resp).await;
        assert!(j.get("unreadcounts").is_some());
    }
}

async fn mf_get_tag(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> MfResult<Json<MfTag>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(l) = Label::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name.as_str()))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("tag").into());
    };
    let cnt = EntryLabel::find()
        .filter(entry_label::Column::LabelId.eq(l.id))
        .count(&st.db)
        .await
        .map_err(internal)? as i64;
    Ok(Json(MfTag {
        title: l.name,
        count: cnt,
    }))
}
