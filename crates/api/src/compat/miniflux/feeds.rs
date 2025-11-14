use super::error::{from_api_error, internal, MfResult};
use super::types::{map_feed, MfFeedDto};
use crate::auth::mf_auth;
use crate::error::{bad_request, not_found};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};

use axum::response::IntoResponse;
use captura_service as service;
use captura_storage::entity::{category, entry, feed, job};

#[derive(serde::Deserialize)]
pub(crate) struct MfFeedsQuery {
    pub category_id: Option<i64>,
    pub disabled: Option<bool>,
    pub has_errors: Option<bool>,
    #[serde(rename = "withCounters")]
    pub with_counters: Option<bool>,
    pub order: Option<String>, // id|title|checked_at|updated_at|created_at|error_count
    pub direction: Option<String>, // asc|desc
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfFeedsQuery>,
) -> MfResult<Json<Vec<MfFeedDto>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let mut sel = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .find_also_related(category::Entity);
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
            let pairs: Vec<(i64, i64)> = entry::Entity::find()
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
    pub url: String,
    pub category_id: Option<i64>,
    pub title: Option<String>,
    // 认证：可选用户名、密码（用于私有源 Basic Auth）
    pub username: Option<String>,
    pub password: Option<String>,
    // 抓取参数（可选）
    pub user_agent: Option<String>,
    pub cookie: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    #[serde(rename = "allow_self_signed_certificates")]
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
}

pub(crate) async fn create(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateFeed>,
) -> MfResult<Json<MfFeedDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
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
        user_agent: Set(body.user_agent.filter(|s| !s.is_empty())),
        username: Set(body.username.filter(|s| !s.is_empty())),
        password: Set(body.password.filter(|s| !s.is_empty())),
        headers_json: Set(None),
        cookies: Set(body.cookie.filter(|s| !s.is_empty())),
        proxy_url: Set(body.proxy_url.filter(|s| !s.is_empty())),
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

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfFeedDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let (f, c) = feed::Entity::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .find_also_related(category::Entity)
        .one(&st.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("feed"))?;
    Ok(Json(map_feed(f, c)))
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfUpdateFeed {
    pub category_id: Option<i64>,
    pub title: Option<String>,
    pub disabled: Option<bool>,
    // 兼容 Miniflux 字段（按现有模型可映射者）
    pub user_agent: Option<String>,
    pub cookie: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    #[serde(rename = "allow_self_signed_certificates")]
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
    pub scraper_rules: Option<String>,
    pub rewrite_rules: Option<String>,
    pub blocklist_rules: Option<String>,
    pub keeplist_rules: Option<String>,
    #[serde(rename = "urlrewrite_rules")]
    pub url_rewrite_rules: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    // 接收但当前忽略（为保持与 Miniflux API 兼容）
    #[allow(dead_code)]
    pub ignore_http_cache: Option<bool>,
    pub feed_url: Option<String>,
    pub site_url: Option<String>,
}

pub(crate) async fn update(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateFeed>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(f) = feed::Entity::find_by_id(id)
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
    crate::feed_options::apply_feed_update_options(
        &mut am,
        crate::feed_options::FeedUpdateOptions {
            user_agent: body.user_agent,
            headers_json: None,
            cookies: body.cookie,
            proxy_url: body.proxy_url,
            fetch_via_proxy: body.fetch_via_proxy,
            disable_http2: body.disable_http2,
            allow_invalid_certs: body.allow_invalid_certs,
            request_timeout_ms: body.request_timeout_ms,
            integrations_json: None,
            rule_params_json: None,
            username: body.username,
            password: body.password,
            scraper_rules: body.scraper_rules,
            rewrite_rules: body.rewrite_rules,
            blocklist_rules: body.blocklist_rules,
            keeplist_rules: body.keeplist_rules,
            url_rewrite_rules: body.url_rewrite_rules,
            feed_url: body.feed_url,
            site_url: body.site_url,
        },
    )?;
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(f) = feed::Entity::find_by_id(id)
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

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(_f) = feed::Entity::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let _ = entry::Entity::update_many()
        .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
        .filter(entry::Column::FeedId.eq(id))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}

#[derive(serde::Deserialize)]
pub struct MfRefreshFeedsQuery {
    pub category_id: Option<i64>,
}

pub(crate) async fn refresh_all(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfRefreshFeedsQuery>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let mut sel = feed::Entity::find().filter(feed::Column::UserId.eq(auth.user_id));
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
        let exists = job::Entity::find()
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
    // 按 Miniflux 语义返回 204，无响应体
    let _ = enqueued; // 保留逻辑以便后续统计/日志
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}

// 单个订阅立即刷新（直连 service，返回插入条数）
pub(crate) async fn refresh_one(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let _auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let _n = service::refresh_and_persist_by_id(&st.db, id)
        .await
        .map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}

#[derive(serde::Serialize)]
pub(crate) struct MfFeedCountersResp {
    pub reads: std::collections::HashMap<i64, i64>,
    pub unreads: std::collections::HashMap<i64, i64>,
}

// GET /v1/feeds/counters
pub(crate) async fn counters(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<MfFeedCountersResp>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let feed_ids: Vec<i64> = feed::Entity::find()
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
    // unread per feed
    let unread_pairs: Vec<(i64, i64)> = entry::Entity::find()
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
    // read per feed
    let read_pairs: Vec<(i64, i64)> = entry::Entity::find()
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
