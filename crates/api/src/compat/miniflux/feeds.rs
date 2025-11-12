use super::error::MfResult;
use super::types::{map_feed, MfFeedDto};
use crate::auth::mf_auth;
use crate::error::{bad_request, internal, not_found};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};

use captura_service as service;
use captura_storage::entity::prelude::*;
use captura_storage::entity::{entry, feed, job};

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
    pub url: String,
    pub category_id: Option<i64>,
    pub title: Option<String>,
}

pub(crate) async fn create(
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

pub(crate) async fn get(
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
    pub category_id: Option<i64>,
    pub title: Option<String>,
    pub disabled: Option<bool>,
}

pub(crate) async fn update(
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

pub(crate) async fn delete(
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

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
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

#[derive(serde::Deserialize)]
pub struct MfRefreshFeedsQuery {
    pub category_id: Option<i64>,
}

pub(crate) async fn refresh_all(
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
    Ok(Json(serde_json::json!({"enqueued": enqueued})))
}

// 单个订阅立即刷新（直连 service，返回插入条数）
pub(crate) async fn refresh_one(
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
    // unread per feed
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
    // read per feed
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
