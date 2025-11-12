use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use url::Url;

use captura_service as service;
use captura_storage::entity::{feed, job, prelude::*};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::AppState;
use crate::{validate_limit_offset, validate_sort};

#[derive(Deserialize)]
pub(crate) struct CreateFeedReq {
    pub category_id: Option<i64>,
    pub r#type: String,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub feed_url: String,
    pub rule_id: Option<i64>,
    pub user_agent: Option<String>,
    pub headers_json: Option<serde_json::Value>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
    pub disabled: Option<bool>,
}

#[derive(Serialize)]
pub(crate) struct CreateFeedResp {
    pub id: i64,
}

#[derive(Serialize)]
pub(crate) struct FeedDto {
    pub id: i64,
    pub title: Option<String>,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub disabled: bool,
    pub category_id: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct FeedsQuery {
    pub category_id: Option<i64>,
    pub disabled: Option<bool>,
    pub has_errors: Option<bool>,
    pub sort_by: Option<String>,
    pub order: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

pub(crate) async fn list_feeds(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    axum::extract::Query(q): axum::extract::Query<FeedsQuery>,
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
        sel = sel.filter(if e {
            feed::Column::ErrorCount.gt(0)
        } else {
            feed::Column::ErrorCount.eq(0)
        });
    }
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
    let l = q.limit.unwrap_or(100);
    sel = sea_orm::QuerySelect::limit(sel, l);
    if let Some(o) = q.offset {
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
pub(crate) struct UpdateFeedReq {
    pub title: Option<String>,
    pub category_id: Option<i64>,
    pub disabled: Option<bool>,
    pub user_agent: Option<String>,
    pub headers_json: Option<serde_json::Value>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
}

pub(crate) async fn update_feed(
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
    if let Some(cid) = body.category_id {
        crate::assert_category_ownership(&st.db, user.user_id, cid).await?;
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
    if let Some(ua) = body.user_agent {
        am.user_agent = Set(Some(ua));
    }
    if let Some(h) = body.headers_json {
        am.headers_json = Set(Some(h));
    }
    if let Some(c) = body.cookies {
        am.cookies = Set(Some(c));
    }
    if let Some(p) = body.proxy_url {
        am.proxy_url = Set(Some(p));
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
    if let Some(v) = body.request_timeout_ms {
        am.request_timeout_ms = Set(Some(v));
    }
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn delete_feed(
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
        crate::assert_category_ownership(&st.db, user.user_id, cid).await?;
    }
    let dup = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::FeedUrl.eq(&body.feed_url))
        .one(&st.db)
        .await
        .map_err(internal)?;
    if dup.is_some() {
        return Err(bad_request("feed already exists"));
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

pub(crate) async fn refresh_feed(
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
