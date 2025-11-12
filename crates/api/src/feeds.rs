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
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use url::Url;

use captura_service as service;
use captura_storage::entity::{enclosure, entry};
use captura_storage::entity::{feed, job, prelude::*, rule};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::util::{validate_limit_offset, validate_sort};
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct CreateFeedReq {
    pub category_id: Option<i64>,
    pub r#type: String,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub feed_url: String,
    pub rule_id: Option<i64>,
    pub rule_params_json: Option<serde_json::Value>,
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
    pub integrations_json: Option<serde_json::Value>,
    pub rule_params_json: Option<serde_json::Value>,
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
    if let Some(v) = body.integrations_json {
        if !v.is_object() {
            return Err(bad_request("integrations_json must be an object"));
        }
        am.integrations_json = Set(Some(v));
    }
    if let Some(v) = body.rule_params_json {
        if !v.is_object() {
            return Err(bad_request("rule_params_json must be an object"));
        }
        am.rule_params_json = Set(Some(v));
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
    if body.feed_url.trim().is_empty() {
        return Err(bad_request("invalid feed_url"));
    }
    // captura_hub:// 路由 → 本地规则模板映射
    let normalized_feed_url = body.feed_url.clone();
    let mut hub_mapped_rule: Option<(String, serde_json::Value)> = None;
    if let Some(rest) = normalized_feed_url.strip_prefix("captura_hub://") {
        let (path, params) = rest
            .split_once('?')
            .map(|(p, q)| (p.to_string(), q.to_string()))
            .unwrap_or((rest.to_string(), String::new()));
        let rid = match path.as_str() {
            "github/trending" => Some("captura.route.github.trending".to_string()),
            "hn/front" => Some("captura.route.hn.front".to_string()),
            "lobsters/front" => Some("captura.route.lobsters.front".to_string()),
            "zhihu/hotlist" => Some("captura.route.zhihu.hotlist".to_string()),
            "reuters/top" => Some("captura.route.reuters.top".to_string()),
            "medium/tag" => Some("captura.route.medium.tag".to_string()),
            _ => None,
        };
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
        let params_json = serde_json::Value::Object(map);
        if let Some(rid) = rid {
            hub_mapped_rule = Some((rid, params_json));
        } else {
            return Err(bad_request("unknown captura_hub route"));
        }
    }
    // 验证 URL（仅当非 captura_hub 路由时）
    if hub_mapped_rule.is_none() && Url::parse(&normalized_feed_url).is_err() {
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
    let dup = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::FeedUrl.eq(&normalized_feed_url))
        .one(&st.db)
        .await
        .map_err(internal)?;
    if dup.is_some() {
        return Err(bad_request("feed already exists"));
    }
    // 如果是 captura_hub 路由，优先落地为 rule 型订阅（模板 + 参数）
    if let Some((rid, params)) = hub_mapped_rule {
        // 找模板
        let tpl = Rule::find()
            .filter(rule::Column::RuleId.eq(rid.clone()))
            .one(&st.db)
            .await
            .map_err(internal)?
            .ok_or_else(|| bad_request("rule template not found for hub route"))?;
        let am = feed::ActiveModel {
            user_id: Set(user.user_id),
            category_id: Set(body.category_id),
            r#type: Set(feed::FeedType::Rule),
            title: Set(body.title.clone()),
            site_url: Set(None),
            feed_url: Set(body.feed_url.clone()),
            rule_id: Set(Some(tpl.id)),
            rule_params_json: Set(Some(params)),
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
            integrations_json: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let res = am.insert(&st.db).await.map_err(internal)?;
        return Ok(Json(CreateFeedResp { id: res.id }));
    }
    // 常规订阅路径
    let am = feed::ActiveModel {
        user_id: Set(user.user_id),
        category_id: Set(body.category_id),
        r#type: Set(ftype),
        title: Set(body.title.clone()),
        site_url: Set(body.site_url.clone()),
        feed_url: Set(normalized_feed_url.clone()),
        rule_id: Set(body.rule_id),
        rule_params_json: Set(body.rule_params_json),
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

// ----------- RSS export (generate RSS 2.0 from stored entries) -----------
pub(crate) async fn rss_feed(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let Some(f) = Feed::find_by_id(id).one(&st.db).await.map_err(internal)? else {
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
