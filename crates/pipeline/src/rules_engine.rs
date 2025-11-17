use captura_common::{Error, NormalizedEntry, Result};
use captura_crawler::{self as crawler, CrawlOptions};
use captura_extract::{
    apply_description_template_v1 as extract_apply_description_template_v1,
    apply_rule_filters_v1 as extract_apply_rule_filters_v1, execute_json_v1_stateless,
    extract_html as extract_extract_html, json_get_path as extract_json_get_path,
    xpath_to_css_like, RuleExecCtx, RuleExecHttpCtx,
};
use captura_hub::v1::{
    merge_rule_params_v1, ContentMergeMode, ContentMode, RuleSpecV1, SourceType,
};
use captura_storage::entity::feed;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value as JsonValue;
use std::time::Duration;
use tracing::{debug, instrument};

use crate::{
    apply_entry_filters_with_cfg, extractor, render_with_params, sanitize_html,
    ContentTransformConfig,
};

/// Local fetch configuration used by helper functions to avoid depending on
/// legacy rule types.
#[derive(Debug, Clone)]
pub(crate) struct FetchCfg {
    pub user_agent: Option<String>,
    pub headers: Option<serde_json::Map<String, JsonValue>>,
    pub smart: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub respect_robots: Option<bool>,
    pub delay_ms: Option<u64>,
    pub limit: Option<usize>,
    pub proxy_url: Option<String>,
}

/// Fetch HTML for rules/Hub handlers using either the HTTP client or the
/// spider-based crawler, depending on `FetchCfg.smart`. This helper is shared
/// by rule executors and Hub utilities.
pub(crate) async fn fetch_html_strategy(
    client: &Client,
    url: &str,
    fetch: &FetchCfg,
    feed: Option<&feed::Model>,
) -> Result<String> {
    // Optional crawler path when `smart = true`.
    if fetch.smart.unwrap_or(false) {
        let mut opts = CrawlOptions {
            user_agent: fetch.user_agent.clone(),
            respect_robots: fetch.respect_robots.unwrap_or(true),
            ..CrawlOptions::default()
        };
        if let Some(d) = fetch.delay_ms {
            opts.delay_ms = d;
        }
        opts.limit = fetch.limit;
        opts.proxy_url = fetch.proxy_url.clone();

        if let Ok(html) = crawler::fetch_html(url, &opts).await {
            if !html.trim().is_empty() {
                return Ok(html);
            }
        }
    }

    // Fallback to plain HTTP using the provided client.
    let mut req = client.get(url);

    // Per-request timeout override if provided.
    if let Some(ms) = fetch.timeout_ms {
        req = req.timeout(Duration::from_millis(ms));
    }

    // Attach per-request headers from FetchCfg.
    if let Some(headers) = &fetch.headers {
        for (k, v) in headers {
            if let Some(s) = v.as_str() {
                if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                    if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                        req = req.header(name, val);
                    }
                }
            }
        }
    }

    // Attach cookies/basic auth from feed when available.
    if let Some(f) = feed {
        if let Some(ref c) = f.cookies {
            if !c.is_empty() {
                req = req.header(reqwest::header::COOKIE, c.clone());
            }
        }
        if let Some(ref u) = f.username {
            req = req.basic_auth(u, f.password.clone());
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    Ok(text)
}

/// Core entrypoint for executing a parsed v1 rule.
#[instrument(skip(feed, spec))]
pub async fn refresh_rule_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    // Execute pure DSL path for v1 rules.
    let mut entries = match spec.source.kind {
        SourceType::ListDetail => execute_list_detail_v1(feed, spec).await,
        SourceType::SinglePage => execute_single_page_v1(feed, spec).await,
        SourceType::Json => {
            let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
            // Rule-level proxies override feed-level proxy when provided so that
            // rules can steer traffic through dedicated proxy pools (e.g. for
            // specific regions), while still allowing feed-level defaults when
            // no rule preference is set.
            let effective_proxy = if let Some(pxs) = &spec.fetch.proxies {
                pxs.first().cloned()
            } else if feed.fetch_via_proxy {
                feed.proxy_url.clone()
            } else {
                None
            };
            let http_ctx = RuleExecHttpCtx {
                user_agent: spec
                    .fetch
                    .user_agent
                    .clone()
                    .or_else(|| feed.user_agent.clone()),
                headers: feed
                    .headers_json
                    .as_ref()
                    .and_then(|v| v.as_object())
                    .cloned(),
                cookies: feed.cookies.clone(),
                basic_auth: feed.username.clone().map(|u| (u, feed.password.clone())),
                proxy_url: effective_proxy,
                timeout_ms: spec
                    .fetch
                    .timeout_ms
                    .or(feed.request_timeout_ms.map(|v| v as u64)),
                smart: spec.fetch.smart,
                respect_robots: spec.fetch.respect_robots,
            };
            let ctx_exec = RuleExecCtx {
                http: http_ctx,
                params,
            };
            execute_json_v1_stateless(spec, &ctx_exec).await
        }
        SourceType::XPath => execute_xpath_v1(feed, spec).await,
    }?;

    extract_apply_rule_filters_v1(spec, &mut entries);
    apply_full_content_when_v1(feed, spec, &mut entries).await?;
    extract_apply_description_template_v1(spec, &mut entries);
    let cfg = ContentTransformConfig {
        url_rewrite_rules: feed.url_rewrite_rules.clone(),
        content_rewrite_rules: feed.rewrite_rules.clone(),
        keep_filter_rules: feed.keep_filter_entry_rules.clone(),
        block_filter_rules: feed.block_filter_entry_rules.clone(),
    };
    apply_entry_filters_with_cfg(&cfg, &mut entries);

    Ok(entries)
}

enum FullContentField {
    Title,
    Summary,
    ContentHtml,
}

struct FullContentMatcher {
    field: FullContentField,
    regex: regex::Regex,
}

async fn apply_full_content_when_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
    entries: &mut [NormalizedEntry],
) -> Result<()> {
    let Some(filters) = &spec.filters else {
        return Ok(());
    };
    let Some(conds) = &filters.fetch_full_content_when else {
        return Ok(());
    };
    if conds.is_empty() {
        return Ok(());
    }

    let mut matchers: Vec<FullContentMatcher> = Vec::new();
    for c in conds {
        let field = match c.field.as_str() {
            "title" => FullContentField::Title,
            "summary" => FullContentField::Summary,
            "content_html" => FullContentField::ContentHtml,
            _ => continue,
        };
        if let Ok(rx) = regex::Regex::new(&c.regex) {
            matchers.push(FullContentMatcher { field, regex: rx });
        }
    }
    if matchers.is_empty() {
        return Ok(());
    }

    let merge_mode = spec
        .transform
        .as_ref()
        .and_then(|t| t.content_merge.as_ref())
        .and_then(|m| m.mode.as_ref())
        .cloned()
        .unwrap_or(ContentMergeMode::Replace);

    for entry in entries.iter_mut() {
        let url = match &entry.url {
            Some(u) if !u.is_empty() => u.clone(),
            _ => continue,
        };

        let mut should_fetch = false;
        for m in &matchers {
            let val = match m.field {
                FullContentField::Title => entry.title.as_deref().unwrap_or(""),
                FullContentField::Summary => entry.summary.as_deref().unwrap_or(""),
                FullContentField::ContentHtml => entry.content_html.as_deref().unwrap_or(""),
            };
            if m.regex.is_match(val) {
                should_fetch = true;
                break;
            }
        }
        if !should_fetch {
            continue;
        }

        match extractor::fetch_and_extract_entry(&url, feed).await {
            Ok(extracted) => {
                let new_html = sanitize_html(&extracted.content_html);
                let merged = match merge_mode {
                    ContentMergeMode::Replace => new_html,
                    ContentMergeMode::Prepend => {
                        let mut buf = new_html;
                        if let Some(old) = &entry.content_html {
                            buf.push_str(old);
                        }
                        buf
                    }
                    ContentMergeMode::Append => {
                        let mut buf = entry.content_html.clone().unwrap_or_default();
                        buf.push_str(&new_html);
                        buf
                    }
                };
                entry.content_html = Some(merged);
                if entry.title.is_none() && extracted.title.is_some() {
                    entry.title = extracted.title;
                }
            }
            Err(e) => {
                debug!(
                    %url,
                    error = %e,
                    "fetch_full_content_when: failed to fetch or extract full content"
                );
                continue;
            }
        }
    }

    Ok(())
}

async fn execute_list_detail_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    let list = spec
        .source
        .list
        .as_ref()
        .ok_or_else(|| Error::Config("source.list is required for list_detail".into()))?;
    let content = spec
        .source
        .content
        .as_ref()
        .ok_or_else(|| Error::Config("source.content is required for list_detail".into()))?;

    let ua = spec
        .fetch
        .user_agent
        .clone()
        .or_else(|| feed.user_agent.clone())
        .unwrap_or_else(|| "captura/0.1".to_string());

    // Rule-level proxies override feed-level proxy when provided.
    let effective_proxy = if let Some(pxs) = &spec.fetch.proxies {
        pxs.first().cloned()
    } else if feed.fetch_via_proxy {
        feed.proxy_url.clone()
    } else {
        None
    };

    let client = crate::http_client::client_for_feed(feed, Some(ua), spec.fetch.timeout_ms)?;

    let fetch_cfg = FetchCfg {
        user_agent: spec
            .fetch
            .user_agent
            .clone()
            .or_else(|| feed.user_agent.clone()),
        headers: None,
        smart: spec.fetch.smart,
        timeout_ms: spec.fetch.timeout_ms,
        respect_robots: spec.fetch.respect_robots,
        delay_ms: None,
        limit: None,
        proxy_url: effective_proxy,
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let final_list_url = render_with_params(&list.request.url, params.as_ref());

    let html = fetch_html_strategy(&client, &final_list_url, &fetch_cfg, Some(feed)).await?;

    #[derive(Debug)]
    struct ListItem {
        url: Option<String>,
        title: Option<String>,
        summary: Option<String>,
        extra_params: serde_json::Map<String, JsonValue>,
    }

    let mut items: Vec<ListItem> = Vec::new();
    {
        let doc = Html::parse_document(&html);
        let item_sel = Selector::parse(&list.item)
            .map_err(|e| Error::Parse(format!("invalid item selector: {e}")))?;
        for el in doc.select(&item_sel) {
            let link = list
                .link
                .as_ref()
                .and_then(|s| crate::extract_attr(&el, s))
                .or_else(|| el.value().attr("href").map(|s| s.to_string()));
            let title = list
                .title
                .as_ref()
                .and_then(|s| crate::extract_text(&el, s));
            let summary = list
                .summary
                .as_ref()
                .and_then(|s| crate::extract_text(&el, s));

            // Per-item extra params for detail_extra (if configured).
            let mut extra_params = serde_json::Map::new();
            if let Some(extra) = &spec.source.detail_extra {
                for (k, expr) in extra.params_from.iter() {
                    let value = if expr.contains('@') {
                        crate::extract_attr(&el, expr)
                    } else {
                        crate::extract_text(&el, expr)
                    };
                    if let Some(v) = value {
                        extra_params.insert(k.clone(), JsonValue::String(v));
                    }
                }
            }

            items.push(ListItem {
                url: link,
                title,
                summary,
                extra_params,
            });
        }
    }
    debug!(
        feed_id = feed.id,
        list_url = %final_list_url,
        items = items.len(),
        "refresh_rule_v1: collected list items"
    );

    let mut entries = Vec::new();
    // Pre-compute global params from rule + feed for reuse in detail_extra.
    let base_params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());

    for item in items {
        let url = item
            .url
            .as_deref()
            .map(|u| crate::absolutize(&final_list_url, u));

        let content_html: Option<String> = match content.mode {
            ContentMode::Readability => {
                crate::readability_like_strategy_async(
                    &client,
                    url.as_deref(),
                    &fetch_cfg,
                    Some(feed),
                )
                .await
            }
            ContentMode::Css | ContentMode::JsonFragment => {
                if let Some(sel) = &content.selector {
                    if let Some(u) = &url {
                        Some(
                            crate::fetch_and_select_strategy(
                                &client,
                                u,
                                sel,
                                &fetch_cfg,
                                Some(feed),
                            )
                            .await?,
                        )
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };
        let mut entry = NormalizedEntry {
            guid: url.clone(),
            url,
            title: item.title,
            summary: item.summary,
            content_html,
            author: None,
            published_at: None,
            enclosures: vec![],
            extras: serde_json::json!({}),
        };

        // Optional per-item extra JSON fetch and merge into extras.
        if let Some(extra) = &spec.source.detail_extra {
            // Build params = global params + item-level params.
            let mut params_map = match &base_params {
                Some(JsonValue::Object(map)) => map.clone(),
                _ => serde_json::Map::new(),
            };
            for (k, v) in item.extra_params.iter() {
                params_map.insert(k.clone(), v.clone());
            }
            let params_val = JsonValue::Object(params_map);
            let extra_url = render_with_params(&extra.request.url, Some(&params_val));

            let mut req = match extra
                .request
                .method
                .as_deref()
                .unwrap_or("GET")
                .to_ascii_uppercase()
                .as_str()
            {
                "POST" => client.post(&extra_url),
                "PUT" => client.put(&extra_url),
                "DELETE" => client.delete(&extra_url),
                _ => client.get(&extra_url),
            };

            // Apply headers from extra.request if present.
            if let Some(headers) = &extra.request.headers {
                for (k, v) in headers {
                    if let Some(s) = v.as_str() {
                        if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                            if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                                req = req.header(name, val);
                            }
                        }
                    }
                }
            }

            // Apply timeout override if provided.
            if let Some(ms) = extra.request.timeout_ms.or(spec.fetch.timeout_ms) {
                req = req.timeout(Duration::from_millis(ms));
            }

            // Basic fetch; expecting JSON.
            match req.send().await {
                Ok(resp) => match resp.text().await {
                    Ok(text) => match serde_json::from_str::<JsonValue>(&text) {
                        Ok(json_val) => {
                            let value = if let Some(root) = &extra.root {
                                extract_json_get_path(&json_val, root)
                                    .cloned()
                                    .unwrap_or(json_val)
                            } else {
                                json_val
                            };
                            entry.extras = value;
                        }
                        Err(e) => {
                            debug!(
                                url = %extra_url,
                                error = %e,
                                "detail_extra: invalid json response"
                            );
                        }
                    },
                    Err(e) => {
                        debug!(
                            url = %extra_url,
                            error = %e,
                            "detail_extra: failed to read response body"
                        );
                    }
                },
                Err(e) => {
                    debug!(
                        url = %extra_url,
                        error = %e,
                        "detail_extra: http request failed"
                    );
                }
            }
        }

        entries.push(entry);
    }

    Ok(entries)
}

async fn execute_single_page_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    let req = spec
        .source
        .request
        .as_ref()
        .ok_or_else(|| Error::Config("source.request is required for single_page".into()))?;
    let content = spec
        .source
        .content
        .as_ref()
        .ok_or_else(|| Error::Config("source.content is required for single_page".into()))?;

    let ua = spec
        .fetch
        .user_agent
        .clone()
        .or_else(|| feed.user_agent.clone())
        .unwrap_or_else(|| "captura/0.1".to_string());

    let effective_proxy = if let Some(pxs) = &spec.fetch.proxies {
        pxs.first().cloned()
    } else if feed.fetch_via_proxy {
        feed.proxy_url.clone()
    } else {
        None
    };

    let client = crate::http_client::client_for_feed(feed, Some(ua), spec.fetch.timeout_ms)?;

    let fetch_cfg = FetchCfg {
        user_agent: spec
            .fetch
            .user_agent
            .clone()
            .or_else(|| feed.user_agent.clone()),
        headers: None,
        smart: spec.fetch.smart,
        timeout_ms: spec.fetch.timeout_ms.or(req.timeout_ms),
        respect_robots: spec.fetch.respect_robots,
        delay_ms: None,
        limit: None,
        proxy_url: effective_proxy,
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let final_url = render_with_params(&req.url, params.as_ref());

    let html = fetch_html_strategy(&client, &final_url, &fetch_cfg, Some(feed)).await?;

    let mut title: Option<String> = None;
    {
        let doc = Html::parse_document(&html);
        if let Ok(sel) = Selector::parse("title") {
            if let Some(el) = doc.select(&sel).next() {
                let t = el.text().collect::<Vec<_>>().join("").trim().to_string();
                if !t.is_empty() {
                    title = Some(t);
                }
            }
        }
    }

    let content_html: Option<String> = match content.mode {
        ContentMode::Readability => {
            crate::readability_like_strategy_async(
                &client,
                Some(&final_url),
                &fetch_cfg,
                Some(feed),
            )
            .await
        }
        ContentMode::Css | ContentMode::JsonFragment => {
            if let Some(sel) = &content.selector {
                Some(
                    crate::fetch_and_select_strategy(
                        &client,
                        &final_url,
                        sel,
                        &fetch_cfg,
                        Some(feed),
                    )
                    .await?,
                )
            } else {
                None
            }
        }
    };

    let entry = NormalizedEntry {
        guid: Some(final_url.clone()),
        url: Some(final_url),
        title,
        summary: None,
        content_html,
        author: None,
        published_at: None,
        enclosures: vec![],
        extras: serde_json::json!({}),
    };

    Ok(vec![entry])
}

/// Execute a v1 rule with `source.type = xpath` using a simple XPath→CSS adapter.
/// This only supports common patterns (see `xpath_to_css_like`) and reuses existing CSS helpers.
async fn execute_xpath_v1(feed: &feed::Model, spec: &RuleSpecV1) -> Result<Vec<NormalizedEntry>> {
    let req = spec
        .source
        .request
        .as_ref()
        .ok_or_else(|| Error::Config("source.request is required for xpath".into()))?;
    let xpath = spec
        .source
        .xpath
        .as_ref()
        .ok_or_else(|| Error::Config("source.xpath is required for xpath".into()))?;

    let ua = spec
        .fetch
        .user_agent
        .clone()
        .or_else(|| feed.user_agent.clone())
        .unwrap_or_else(|| "captura/0.1".to_string());

    let effective_proxy = if let Some(pxs) = &spec.fetch.proxies {
        pxs.first().cloned()
    } else if feed.fetch_via_proxy {
        feed.proxy_url.clone()
    } else {
        None
    };

    let client = crate::http_client::client_for_feed(feed, Some(ua), spec.fetch.timeout_ms)?;

    let fetch_cfg = FetchCfg {
        user_agent: spec
            .fetch
            .user_agent
            .clone()
            .or_else(|| feed.user_agent.clone()),
        headers: None,
        smart: spec.fetch.smart,
        timeout_ms: spec.fetch.timeout_ms.or(req.timeout_ms),
        respect_robots: spec.fetch.respect_robots,
        delay_ms: None,
        limit: None,
        proxy_url: effective_proxy,
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let final_url = crate::render_with_params(&req.url, params.as_ref());

    let html = fetch_html_strategy(&client, &final_url, &fetch_cfg, Some(feed)).await?;

    let doc = Html::parse_document(&html);
    let item_css = xpath_to_css_like(&xpath.item);
    let item_sel = Selector::parse(&item_css)
        .map_err(|e| Error::Parse(format!("invalid xpath item selector: {e}")))?;

    let mut entries = Vec::new();

    for el in doc.select(&item_sel) {
        let title = xpath.title.as_ref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            crate::extract_text(&el, &css)
        });

        let url = xpath.url.as_ref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            let raw = if css.contains('@') {
                crate::extract_attr(&el, &css)
            } else {
                crate::extract_text(&el, &css)
            };
            raw.map(|u| crate::absolutize(&final_url, &u))
        });

        let content_html = xpath.content_html.as_ref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            extract_extract_html(&el, &css)
        });

        let entry = NormalizedEntry {
            guid: url.clone(),
            url,
            title,
            summary: None,
            content_html,
            author: None,
            published_at: None,
            enclosures: vec![],
            extras: serde_json::json!({}),
        };

        entries.push(entry);
    }

    Ok(entries)
}
