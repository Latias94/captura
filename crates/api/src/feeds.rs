use axum::response::Response;
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use url::Url;

use captura_service as service;
use captura_storage::entity::{enclosure, entry};
use captura_storage::entity::{feed, job};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::feed_options::{apply_feed_update_options, FeedUpdateOptions};
use crate::util::{validate_limit_offset, validate_sort};
use crate::AppState;
use captura_types::{EntryView, FeedCountersDto, FeedDto, Paging, Sorting};

#[derive(Deserialize)]
pub(crate) struct CreateFeedReq {
    pub category_id: Option<i64>,
    pub r#type: String,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub feed_url: String,
    pub rule_id: Option<i64>,
    pub rule_params_json: Option<serde_json::Value>,
    // Fetch options
    pub user_agent: Option<String>,
    pub headers_json: Option<serde_json::Value>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
    // Basic auth (for private feeds)
    pub username: Option<String>,
    pub password: Option<String>,
    pub disabled: Option<bool>,
    pub view: Option<EntryView>,
}

#[derive(Serialize)]
pub(crate) struct CreateFeedResp {
    pub id: i64,
}

#[derive(Deserialize)]
pub(crate) struct FeedsQuery {
    pub category_id: Option<i64>,
    pub disabled: Option<bool>,
    pub has_errors: Option<bool>,
    #[serde(flatten)]
    pub sorting: Sorting,
    #[serde(flatten)]
    pub paging: Paging,
}

pub(crate) async fn list_feeds(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    axum::extract::Query(q): axum::extract::Query<FeedsQuery>,
) -> ApiResult<Json<Vec<FeedDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.paging.limit, q.paging.offset)?;
    validate_sort(
        &q.sorting.sort_by,
        &["updated_at", "created_at", "error_count", "title"],
        &q.sorting.order,
    )?;
    let mut sel = feed::Entity::find().filter(feed::Column::UserId.eq(user.user_id));
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
    match q.sorting.sort_by.as_deref() {
        Some("created_at") => {
            sel = match q.sorting.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::CreatedAt),
                _ => sel.order_by_desc(feed::Column::CreatedAt),
            }
        }
        Some("updated_at") => {
            sel = match q.sorting.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::UpdatedAt),
                _ => sel.order_by_desc(feed::Column::UpdatedAt),
            }
        }
        Some("error_count") => {
            sel = match q.sorting.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::ErrorCount),
                _ => sel.order_by_desc(feed::Column::ErrorCount),
            }
        }
        Some("title") => {
            sel = match q.sorting.order.as_deref() {
                Some("desc") => sel.order_by_desc(feed::Column::Title),
                _ => sel.order_by_asc(feed::Column::Title),
            }
        }
        _ => {
            sel = match q.sorting.order.as_deref() {
                Some("asc") => sel.order_by_asc(feed::Column::UpdatedAt),
                _ => sel.order_by_desc(feed::Column::UpdatedAt),
            }
        }
    }
    let l = q.paging.limit.unwrap_or(100);
    sel = sea_orm::QuerySelect::limit(sel, l);
    if let Some(o) = q.paging.offset {
        sel = sea_orm::QuerySelect::offset(sel, o);
    }
    let list = sel.all(&st.db).await.map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|f| FeedDto {
                id: f.id,
                title: f.title,
                feed_url: f.feed_url,
                site_url: f.site_url,
                disabled: f.disabled,
                category_id: f.category_id,
                view: f.view.as_deref().and_then(EntryView::from_str),
            })
            .collect(),
    ))
}

pub(crate) async fn get_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FeedDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let f = load_owned_feed(&st.db, user.user_id, id).await?;
    Ok(Json(FeedDto {
        id: f.id,
        title: f.title,
        feed_url: f.feed_url,
        site_url: f.site_url,
        disabled: f.disabled,
        category_id: f.category_id,
        view: f.view.as_deref().and_then(EntryView::from_str),
    }))
}

#[derive(Deserialize, Default)]
pub(crate) struct UpdateFeedReq {
    pub title: Option<String>,
    pub category_id: Option<i64>,
    pub disabled: Option<bool>,
    // Fetch options
    pub user_agent: Option<String>,
    pub headers_json: Option<serde_json::Value>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
    pub integrations_json: Option<serde_json::Value>,
    pub rule_params_json: Option<serde_json::Value>,
    // Basic auth (for private feeds)
    pub username: Option<String>,
    pub password: Option<String>,
    pub view: Option<EntryView>,
}

pub(crate) async fn update_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateFeedReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let f = load_owned_feed(&st.db, user.user_id, id).await?;
    if let Some(cid) = body.category_id {
        crate::util::assert_category_ownership(&st.db, user.user_id, cid).await?;
    }
    let mut am: feed::ActiveModel = f.into();
    if let Some(t) = body.title {
        am.title = Set(Some(t));
    }
    if let Some(cid) = body.category_id {
        am.category_id = Set(Some(cid));
    }
    if let Some(d) = body.disabled {
        am.disabled = Set(d);
    }
    if let Some(v) = body.view {
        // Store as snake_case string in DB.
        am.view = Set(Some(v.as_str().to_string()));
    }
    apply_feed_update_options(
        &mut am,
        FeedUpdateOptions {
            user_agent: body.user_agent,
            headers_json: body.headers_json,
            cookies: body.cookies,
            proxy_url: body.proxy_url,
            fetch_via_proxy: body.fetch_via_proxy,
            disable_http2: body.disable_http2,
            allow_invalid_certs: body.allow_invalid_certs,
            request_timeout_ms: body.request_timeout_ms,
            integrations_json: body.integrations_json,
            rule_params_json: body.rule_params_json,
            username: body.username,
            password: body.password,
            scraper_rules: None,
            rewrite_rules: None,
            blocklist_rules: None,
            keeplist_rules: None,
            url_rewrite_rules: None,
            feed_url: None,
            site_url: None,
        },
    )?;
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn delete_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let f = load_owned_feed(&st.db, user.user_id, id).await?;
    let am: feed::ActiveModel = f.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn create_feed(
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
    if body.feed_url.trim().is_empty() {
        return Err(bad_request("invalid feed_url"));
    }
    // captura_hub:// route → hub-based subscription (no rule indirection)
    let normalized_feed_url = body.feed_url.clone();
    let mut hub_params_json: Option<serde_json::Value> = None;
    let mut effective_type = ftype;
    if let Some(rest) = normalized_feed_url.strip_prefix("captura_hub://") {
        let (path, params) = rest
            .split_once('?')
            .map(|(p, q)| (p.to_string(), q.to_string()))
            .unwrap_or((rest.to_string(), String::new()));
        let hub_id = path.trim_start_matches('/');
        if captura_rules::routes::registry::find_route_meta(hub_id).is_none() {
            return Err(bad_request("unknown captura_hub route"));
        }
        let mut map = serde_json::Map::new();
        if !params.is_empty() {
            for pair in params.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    map.insert(
                        k.to_string(),
                        serde_json::Value::String(
                            urlencoding::decode(v)
                                .unwrap_or_else(|_| v.into())
                                .into_owned(),
                        ),
                    );
                }
            }
        }
        hub_params_json = Some(serde_json::Value::Object(map));
        effective_type = feed::FeedType::Hub;
    }
    // Validate URL (only when not using captura_hub scheme)
    if !normalized_feed_url.starts_with("captura_hub://")
        && Url::parse(&normalized_feed_url).is_err()
    {
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
        crate::util::assert_category_ownership(&st.db, user.user_id, cid).await?;
    }
    let dup = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::FeedUrl.eq(&normalized_feed_url))
        .one(&st.db)
        .await
        .map_err(internal)?;
    if dup.is_some() {
        return Err(bad_request("feed already exists"));
    }
    // Regular feed or hub subscription path
    let am = feed::ActiveModel {
        user_id: Set(user.user_id),
        category_id: Set(body.category_id),
        r#type: Set(effective_type),
        title: Set(body.title.clone()),
        site_url: Set(body.site_url.clone()),
        feed_url: Set(normalized_feed_url.clone()),
        rule_id: Set(body.rule_id),
        rule_params_json: Set(hub_params_json.or(body.rule_params_json)),
        user_agent: Set(non_empty_opt(body.user_agent.clone())),
        username: Set(non_empty_opt(body.username.clone())),
        password: Set(non_empty_opt(body.password.clone())),
        headers_json: Set(body.headers_json),
        cookies: Set(non_empty_opt(body.cookies.clone())),
        proxy_url: Set(non_empty_opt(body.proxy_url.clone())),
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
        view: Set(body.view.map(|v| v.as_str().to_string())),
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

/// Count read/unread entries per feed for the current user (for /api/v1 first-party clients).
pub(crate) async fn feeds_counters(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<FeedCountersDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let (reads, unreads) =
        service::query::feed_counters_for_user(&st.db, user.user_id).await.map_err(internal)?;
    Ok(Json(FeedCountersDto { reads, unreads }))
}

pub(crate) async fn refresh_feed(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let f = load_owned_feed(&st.db, user.user_id, id).await?;
    let inserted = service::refresh_and_persist(&st.db, &f)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"inserted": inserted})))
}

#[derive(Serialize)]
pub(crate) struct EnqueueResp {
    pub id: i64,
}

pub(crate) async fn enqueue_feed_refresh(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<EnqueueResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let f = load_owned_feed(&st.db, user.user_id, id).await?;
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

// ----------- RSS export (generate RSS 2.0 from stored entries) -----------
pub(crate) async fn rss_feed(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let Some(f) = feed::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    // collect latest entries
    let rows = entry::Entity::find()
        .filter(entry::Column::FeedId.eq(id))
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(50)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let entry_ids: Vec<i64> = rows.iter().map(|e| e.id).collect();
    // enclosures map
    use std::collections::HashMap;
    let mut emap: HashMap<i64, Vec<enclosure::Model>> = HashMap::new();
    if !entry_ids.is_empty() {
        let encs = enclosure::Entity::find()
            .filter(enclosure::Column::EntryId.is_in(entry_ids.clone()))
            .all(&st.db)
            .await
            .map_err(internal)?;
        for e in encs {
            emap.entry(e.entry_id).or_default().push(e);
        }
    }
    // build RSS 2.0
    let mut out = String::new();
    use std::fmt::Write as _;
    writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").ok();
    writeln!(out, "<rss version=\"2.0\">\n<channel>").ok();
    let title = f.title.clone().unwrap_or_else(|| f.feed_url.clone());
    let link = f.site_url.clone().unwrap_or_else(|| f.feed_url.clone());
    writeln!(out, "<title>{}</title>", xml_escape(&title)).ok();
    writeln!(out, "<link>{}</link>", xml_escape(&link)).ok();
    writeln!(out, "<description>Generated by Captura</description>").ok();
    for e in rows {
        writeln!(out, "<item>").ok();
        if let Some(t) = &e.title {
            writeln!(out, "<title>{}</title>", xml_escape(t)).ok();
        }
        if let Some(u) = &e.url {
            writeln!(out, "<link>{}</link>", xml_escape(u)).ok();
        }
        // guid prefer URL else GUID hash/id
        if let Some(u) = &e.url {
            writeln!(out, "<guid isPermaLink=\"true\">{}</guid>", xml_escape(u)).ok();
        } else if let Some(g) = &e.guid {
            writeln!(out, "<guid>{}</guid>", xml_escape(g)).ok();
        } else {
            writeln!(out, "<guid>{}</guid>", e.id).ok();
        }
        if let Some(d) = e.published_at {
            writeln!(out, "<pubDate>{}</pubDate>", d.to_rfc2822()).ok();
        }
        let body = e
            .content_html
            .clone()
            .or(e.summary.clone())
            .unwrap_or_default();
        writeln!(out, "<description>{}</description>", xml_escape(&body)).ok();
        if let Some(list) = emap.get(&e.id) {
            for enc in list {
                let url = xml_escape(&enc.url);
                let len = enc.length.unwrap_or(0);
                let mt = xml_escape(
                    &enc.mime
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                );
                writeln!(
                    out,
                    "<enclosure url=\"{}\" length=\"{}\" type=\"{}\"/>",
                    url, len, mt
                )
                .ok();
            }
        }
        writeln!(out, "</item>").ok();
    }
    writeln!(out, "</channel>\n</rss>").ok();
    let mut resp = Response::new(out.into());
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/rss+xml; charset=utf-8"),
    );
    Ok(resp)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn non_empty_opt(v: Option<String>) -> Option<String> {
    v.and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
}

async fn load_owned_feed(
    db: &DatabaseConnection,
    user_id: i64,
    feed_id: i64,
) -> ApiResult<feed::Model> {
    let Some(f) = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user_id))
        .filter(feed::Column::Id.eq(feed_id))
        .one(db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    Ok(f)
}
