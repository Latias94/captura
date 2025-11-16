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

use captura_rules::v1::{parse_rule_v1, RuleSpecV1, SourceType};
use captura_storage::entity::{feed, rule};

use crate::auth::AuthUser;
use crate::error::{bad_request, forbidden, internal, not_found, ApiResult};
use crate::util::validate_limit_offset;
use crate::AppState;
use captura_types::{IdResp, Paging};
use regex::Regex;

#[derive(Serialize)]
pub(crate) struct RuleDto {
    pub id: i64,
    pub rule_id: String,
    pub kind: String,
    pub namespace: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CreateRuleReq {
    /// Optional YAML form of the rule spec (DSL v1).
    pub yaml: Option<String>,
    /// Optional JSON form of the rule spec (DSL v1).
    pub spec_json: Option<serde_json::Value>,
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
) -> ApiResult<Json<IdResp>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let spec: RuleSpecV1 = if let Some(spec_json) = body.spec_json.clone() {
        serde_json::from_value(spec_json)
            .map_err(|e| bad_request(format!("invalid rule spec_json: {e}")))?
    } else if let Some(yaml) = body.yaml.as_ref() {
        parse_rule_v1(yaml).map_err(|e| bad_request(format!("invalid rule yaml: {e}")))?
    } else {
        return Err(bad_request("either yaml or spec_json is required"));
    };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let examples = serde_json::to_value(&spec.examples).map_err(internal)?;
    let spec_json = serde_json::to_value(&spec).map_err(internal)?;
    let am = rule::ActiveModel {
        rule_id: Set(spec.id.clone()),
        kind: Set("dsl".to_string()),
        version: Set(body.version.clone()),
        namespace: Set(rule_namespace(&spec.id)),
        description: Set(spec.description.clone()),
        spec_json: Set(Some(spec_json)),
        handler_target: Set(None),
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
pub(crate) struct RulesQuery {
    pub q: Option<String>,
    #[serde(flatten)]
    pub paging: Paging,
}

pub(crate) async fn list_rules(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<RulesQuery>,
) -> ApiResult<Json<Vec<RuleDto>>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.paging.limit, q.paging.offset)?;
    let mut sel = rule::Entity::find();
    if let Some(ref s) = q.q {
        let like = format!("%{}%", s);
        sel = sel.filter(
            Condition::any()
                .add(rule::Column::RuleId.like(like.as_str()))
                .add(rule::Column::Description.like(like.as_str())),
        );
    }
    if let Some(l) = q.paging.limit {
        sel = sel.limit(l);
    }
    if let Some(o) = q.paging.offset {
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
                kind: r.kind,
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
    let Some(r) = rule::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("rule not found"));
    };
    Ok(Json(RuleDto {
        id: r.id,
        rule_id: r.rule_id,
        kind: r.kind,
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
    let Some(r) = rule::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("rule not found"));
    };
    let spec: RuleSpecV1 = if let Some(spec_json) = body.spec_json.clone() {
        serde_json::from_value(spec_json)
            .map_err(|e| bad_request(format!("invalid rule spec_json: {e}")))?
    } else if let Some(yaml) = body.yaml.as_ref() {
        parse_rule_v1(yaml).map_err(|e| bad_request(format!("invalid rule yaml: {e}")))?
    } else {
        return Err(bad_request("either yaml or spec_json is required"));
    };
    let examples = serde_json::to_value(&spec.examples).map_err(internal)?;
    let spec_json = serde_json::to_value(&spec).map_err(internal)?;
    let mut am: rule::ActiveModel = r.into();
    am.rule_id = Set(spec.id.clone());
    am.kind = Set("dsl".to_string());
    am.version = Set(body.version.clone());
    am.namespace = Set(rule_namespace(&spec.id));
    am.description = Set(spec.description.clone());
    am.spec_json = Set(Some(spec_json));
    am.handler_target = Set(None);
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
    let used = feed::Entity::find()
        .filter(feed::Column::RuleId.eq(id))
        .count(&st.db)
        .await
        .map_err(internal)?;
    if used > 0 {
        return Err(forbidden("rule is in use by feeds"));
    }
    let Some(r) = rule::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("rule not found"));
    };
    let am: rule::ActiveModel = r.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

// ---------------- Templates (rule presets) ----------------

#[derive(Serialize)]
pub(crate) struct RuleTemplateDto {
    pub id: i64,
    pub rule_id: String,
    pub namespace: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub params: Vec<String>,
}

fn extract_params_from_rule(rec: &rule::Model) -> Vec<String> {
    if let Some(spec_json) = &rec.spec_json {
        if let Ok(spec_v1) = serde_json::from_value::<RuleSpecV1>(spec_json.clone()) {
            if matches!(spec_v1.source.kind, SourceType::ListDetail) {
                if let Some(list) = spec_v1.source.list {
                    return extract_params_from_url(&list.request.url);
                }
            }
        }
    }
    Vec::new()
}

fn extract_params_from_url(url: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let re1 = Regex::new(r":([a-zA-Z0-9_]+)").unwrap();
    let re2 = Regex::new(r"\{([a-zA-Z0-9_]+)\}").unwrap();
    for caps in re1.captures_iter(url) {
        if let Some(m) = caps.get(1) {
            names.push(m.as_str().to_string());
        }
    }
    for caps in re2.captures_iter(url) {
        if let Some(m) = caps.get(1) {
            names.push(m.as_str().to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

#[derive(Deserialize)]
pub(crate) struct TemplatesQuery {
    pub ns: Option<String>,
    pub q: Option<String>,
    #[serde(flatten)]
    pub paging: Paging,
}

pub(crate) async fn list_templates(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<TemplatesQuery>,
) -> ApiResult<Json<Vec<RuleTemplateDto>>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.paging.limit, q.paging.offset)?;
    // Simple strategy: filter by namespace or fuzzy match on rule_id/description
    let mut sel = rule::Entity::find();
    if let Some(ref ns) = q.ns {
        sel = sel.filter(rule::Column::Namespace.eq(ns.to_string()));
    }
    if let Some(ref s) = q.q {
        let like = format!("%{}%", s);
        sel = sel.filter(
            Condition::any()
                .add(rule::Column::RuleId.like(like.as_str()))
                .add(rule::Column::Description.like(like.as_str())),
        );
    }
    let sel = if let Some(l) = q.paging.limit {
        sel.limit(l)
    } else {
        sel
    };
    let sel = if let Some(o) = q.paging.offset {
        sel.offset(o)
    } else {
        sel
    };
    let rows = sel
        .order_by_desc(rule::Column::UpdatedAt)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let list = rows
        .into_iter()
        .map(|r| {
            let params = extract_params_from_rule(&r);
            RuleTemplateDto {
                id: r.id,
                rule_id: r.rule_id,
                namespace: r.namespace,
                description: r.description,
                version: r.version,
                params,
            }
        })
        .collect();
    Ok(Json(list))
}

pub(crate) async fn get_template(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RuleTemplateDto>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(r) = rule::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("rule template"));
    };
    let params = extract_params_from_rule(&r);
    Ok(Json(RuleTemplateDto {
        id: r.id,
        rule_id: r.rule_id,
        namespace: r.namespace,
        description: r.description,
        version: r.version,
        params,
    }))
}

#[derive(Deserialize)]
pub(crate) struct CreateFeedFromTemplateReq {
    pub template_id: i64,
    pub params: serde_json::Value,
    pub title: Option<String>,
    pub category_id: Option<i64>,
}

pub(crate) async fn create_feed_from_template(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(req): Json<CreateFeedFromTemplateReq>,
) -> ApiResult<Json<IdResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if !req.params.is_object() {
        return Err(bad_request("params must be object"));
    }
    if let Some(cid) = req.category_id {
        crate::util::assert_category_ownership(&st.db, user.user_id, cid).await?;
    }
    let Some(r) = rule::Entity::find_by_id(req.template_id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("rule template"));
    };
    let spec_json = r
        .spec_json
        .ok_or_else(|| internal("rule missing spec_json"))?;
    let spec: RuleSpecV1 = serde_json::from_value(spec_json).map_err(internal)?;
    // Render feed_url for easier debugging (even if rule mode does not depend on feed_url); only for list_detail use list.request.url.
    let feed_url_rendered = if matches!(spec.source.kind, SourceType::ListDetail) {
        if let Some(list) = spec.source.list {
            let mut s = list.request.url;
            if let Some(map) = req.params.as_object() {
                for (k, v) in map.iter() {
                    let needle1 = format!(":{}", k);
                    let needle2 = format!("{{{}}}", k);
                    let repl_owned = v
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| v.to_string());
                    s = s.replace(&needle1, &repl_owned);
                    s = s.replace(&needle2, &repl_owned);
                }
            }
            s
        } else {
            r.rule_id.clone()
        }
    } else {
        r.rule_id.clone()
    };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = feed::ActiveModel {
        user_id: Set(user.user_id),
        category_id: Set(req.category_id),
        r#type: Set(feed::FeedType::Rule),
        title: Set(req.title.clone()),
        site_url: Set(None),
        feed_url: Set(feed_url_rendered),
        rule_id: Set(Some(r.id)),
        rule_params_json: Set(Some(req.params.clone())),
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
        integrations_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let res = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(crate::IdResp { id: res.id }))
}

#[derive(Deserialize)]
pub(crate) struct TryRuleReq {
    pub url: String,
    pub rule_id: Option<i64>,
    pub yaml: Option<String>,
    pub spec_json: Option<serde_json::Value>,
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
    let mut spec = if let Some(spec_json) = req.spec_json {
        serde_json::from_value::<RuleSpecV1>(spec_json)
            .map_err(|e| bad_request(format!("invalid rule spec_json: {e}")))?
    } else if let Some(y) = req.yaml {
        parse_rule_v1(&y).map_err(internal)?
    } else {
        let rid = req
            .rule_id
            .ok_or_else(|| bad_request("rule_id or yaml required"))?;
        let r = rule::Entity::find_by_id(rid)
            .one(&st.db)
            .await
            .map_err(internal)?
            .ok_or_else(|| not_found("rule not found"))?;
        if r.kind != "dsl" {
            return Err(bad_request("try_rule currently only supports dsl rules"));
        }
        let spec_json = r
            .spec_json
            .ok_or_else(|| internal("rule missing spec_json"))?;
        serde_json::from_value::<RuleSpecV1>(spec_json).map_err(internal)?
    };
    if !matches!(spec.source.kind, SourceType::ListDetail) {
        return Err(bad_request(
            "try_rule currently only supports list_detail source type",
        ));
    }
    let list = spec
        .source
        .list
        .as_mut()
        .ok_or_else(|| bad_request("rule has no list section"))?;
    list.request.url = req.url.clone();

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
        rule_params_json: None,
        user_agent: spec.fetch.user_agent.clone(),
        username: None,
        password: None,
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
        last_error_message: None,
        disabled: false,
        scraper_rules: None,
        rewrite_rules: None,
        blocklist_rules: None,
        keeplist_rules: None,
        url_rewrite_rules: None,
        block_filter_entry_rules: None,
        keep_filter_entry_rules: None,
        integrations_json: None,
        created_at: now,
        updated_at: now,
    };

    let started = std::time::Instant::now();
    let entries = captura_pipeline::refresh_rule_v1(&feed_model, &spec)
        .await
        .map_err(internal)?;
    let duration_ms = started.elapsed().as_millis();
    let used_smart = spec.fetch.smart.unwrap_or(false);

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
        delay_ms: None,
        limit: None,
        proxy_applied: false,
        list_html_len: 0,
        fallback_used: false,
        http_status: None,
        duration_ms,
        final_url: None,
        redirect_count: None,
        list_item_matches: None,
        content_selector_matches: None,
    }))
}
