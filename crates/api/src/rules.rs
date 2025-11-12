use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};

use captura_crawler::{self as crawler, CrawlOptions};
use captura_pipeline;
use captura_rules::{parse_rule, RuleSpec};
use captura_storage::entity::{feed, prelude::*, rule};

use crate::auth::AuthUser;
use crate::error::{bad_request, forbidden, internal, not_found, ApiResult};
use crate::validate_limit_offset;
use crate::AppState;

#[derive(Serialize)]
pub(crate) struct RuleDto {
    pub id: i64,
    pub rule_id: String,
    pub namespace: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CreateRuleReq {
    pub yaml: String,
    pub version: Option<String>,
    pub maintainer: Option<String>,
}

fn rule_namespace(id: &str) -> Option<String> {
    id.rsplit_once('.').map(|(ns, _)| ns.to_string())
}

pub(crate) async fn create_rule(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateRuleReq>,
) -> ApiResult<Json<crate::IdResp>> {
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
    Ok(Json(crate::IdResp { id: rec.id }))
}

#[derive(Deserialize)]
pub(crate) struct RulesQuery {
    pub q: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

pub(crate) async fn list_rules(
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

pub(crate) async fn get_rule(
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

pub(crate) async fn update_rule(
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

pub(crate) async fn delete_rule(
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

#[derive(Deserialize)]
pub(crate) struct TryRuleReq {
    pub url: String,
    pub rule_id: Option<i64>,
    pub yaml: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct TryRuleEntry {
    pub title: Option<String>,
    pub url: Option<String>,
    pub content_len: usize,
}

#[derive(Serialize)]
pub(crate) struct TryRuleResp {
    pub used_smart: bool,
    pub list_url: String,
    pub item_count: usize,
    pub entries: Vec<TryRuleEntry>,
    pub ua: Option<String>,
    pub timeout_ms: Option<u64>,
    pub respect_robots: Option<bool>,
    pub delay_ms: Option<u64>,
    pub limit: Option<usize>,
    pub proxy_applied: bool,
    pub list_html_len: usize,
    pub fallback_used: bool,
    pub http_status: Option<u16>,
    pub duration_ms: u128,
    pub final_url: Option<String>,
    pub redirect_count: Option<u32>,
    pub list_item_matches: Option<usize>,
    pub content_selector_matches: Option<usize>,
}

#[axum::debug_handler]
pub(crate) async fn try_rule(
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
                fallback_used = true;
            }
        }
    }
    if list_html_len == 0 {
        use reqwest::Client;
        if let Ok(http) = Client::builder()
            .user_agent(
                spec.fetch
                    .user_agent
                    .clone()
                    .unwrap_or_else(|| "captura/0.1".into()),
            )
            .build()
        {
            use reqwest::header::HeaderMap;
            let header_map: Option<HeaderMap> = if let Some(hdrs) = spec.fetch.headers.as_ref() {
                let mut hm = HeaderMap::new();
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
                // fetch content html then count selector matches
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
                if let Ok(sel) = scraper::Selector::parse(sel_str) {
                    let doc = scraper::Html::parse_document(&content_html);
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
