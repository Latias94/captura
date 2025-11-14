//! Pipeline orchestrates fetcher / crawler / rules execution
//! into normalized entries ready for persistence.

use captura_common::{Error, NormalizedEntry, Result};
use captura_crawler::{self as crawler, CrawlOptions};
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_rules::v1::{ContentMergeMode, ContentMode, RuleSpecV1, SourceType};
use captura_storage::entity::feed;
use regex::Regex;
use reqwest::header::HeaderMap;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value as JsonValue;
use tracing::{debug, instrument};
use url::Url;

mod handlers;
mod hub_bridge;
mod hub_utils;
mod rules_engine;

use rules_engine::{fetch_html_strategy, FetchCfg};

pub use rules_engine::{refresh_rule_v1, refresh_rule_v1_with_yaml, refresh_rule_with_yaml};
pub use hub_bridge::execute_hub_route;

#[derive(Debug, Clone)]
pub struct RefreshMeta {
    pub last_status: Option<u16>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub mod extractor;

#[instrument(skip(feed))]
pub async fn refresh_feed(feed: &feed::Model) -> Result<Vec<NormalizedEntry>> {
    let (entries, _meta) = refresh_feed_with_meta(feed).await?;
    Ok(entries)
}

#[instrument(skip(feed))]
pub async fn refresh_feed_with_meta(
    feed: &feed::Model,
) -> Result<(Vec<NormalizedEntry>, Option<RefreshMeta>)> {
    match feed.r#type {
        feed::FeedType::Rss | feed::FeedType::Atom | feed::FeedType::Json => {
            refresh_standard_feed_with_meta(feed).await
        }
        feed::FeedType::Rule => Ok((vec![], None)),
    }
}

#[instrument(skip(feed))]
async fn refresh_standard_feed_with_meta(
    feed: &feed::Model,
) -> Result<(Vec<NormalizedEntry>, Option<RefreshMeta>)> {
    let mut headers = HeaderMap::new();
    // Merge custom headers from DB
    if let Some(ref json) = feed.headers_json {
        if let Some(map) = json.as_object() {
            for (k, v) in map {
                if let Some(s) = v.as_str() {
                    if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                        if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                            headers.insert(name, val);
                        }
                    }
                }
            }
        }
    }
    // Cookies
    if let Some(ref c) = feed.cookies {
        if let Ok(val) = reqwest::header::HeaderValue::from_str(c) {
            headers.insert(reqwest::header::COOKIE, val);
        }
    }

    let opts = FetchOptions {
        user_agent: feed.user_agent.clone(),
        etag: feed.etag.clone(),
        last_modified: feed.last_modified.clone(),
        headers,
        timeout: feed
            .request_timeout_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64)),
        allow_invalid_certs: feed.allow_invalid_certs,
        disable_http2: feed.disable_http2,
        proxy_url: if feed.fetch_via_proxy {
            feed.proxy_url.clone()
        } else {
            None
        },
        basic_auth: match (feed.username.clone(), feed.password.clone()) {
            (Some(u), Some(p)) if !u.is_empty() => Some((u, p)),
            (Some(u), None) if !u.is_empty() => Some((u, String::new())),
            _ => None,
        },
    };
    let client = HttpFetcher::new(opts)?;
    let out = client.fetch_feed_with_meta(&feed.feed_url).await?;
    let meta = Some(RefreshMeta {
        last_status: Some(out.meta.status.as_u16()),
        etag: out.meta.etag.clone(),
        last_modified: out.meta.last_modified.clone(),
    });
    if let Some(parsed) = out.feed {
        let mut entries: Vec<NormalizedEntry> = parsed
            .entries
            .into_iter()
            .map(|e| {
                let summary_text = e.summary.as_ref().map(|s| s.content.clone());
                let mut url = e.links.first().map(|l| clean_url(&l.href));
                // URL rewrite rules
                if let Some(ref rules) = feed.url_rewrite_rules {
                    if let Some(u) = &url {
                        url = Some(apply_rewrite_rules(u, rules));
                    }
                }
                let mut content_html = e
                    .content
                    .and_then(|c| c.body)
                    .or(summary_text.clone())
                    .map(|html| sanitize_html(&html));
                // Content rewrite rules
                if let Some(ref rules) = feed.rewrite_rules {
                    if let Some(c) = &content_html {
                        content_html = Some(apply_rewrite_rules(c, rules));
                    }
                }
                // 提取 enclosure（依据 link rel="enclosure"）
                let mut enclosures = Vec::new();
                for l in e.links.iter() {
                    // feed-rs: Link { href, rel, media_type, length, .. }
                    let rel = l.rel.as_deref().unwrap_or("");
                    if rel.eq_ignore_ascii_case("enclosure") {
                        let url = clean_url(&l.href);
                        let typ = l.media_type.as_deref().map(|s| s.to_string());
                        let len = l.length.map(|n| n as i64);
                        enclosures.push(captura_common::Enclosure {
                            url,
                            r#type: typ,
                            length: len,
                            kind: None,
                        });
                    }
                }

                NormalizedEntry {
                    guid: Some(e.id),
                    url,
                    title: e.title.map(|t| t.content),
                    summary: summary_text.clone(),
                    content_html,
                    author: e.authors.first().map(|a| a.name.clone()),
                    published_at: e.published.or(e.updated),
                    enclosures,
                    extras: serde_json::json!({}),
                }
            })
            .collect();
        apply_entry_filters(feed, &mut entries);
        Ok((entries, meta))
    } else {
        Ok((vec![], meta))
    }
}

pub(crate) fn sanitize_html(input: &str) -> String {
    let mut builder = ammonia::Builder::default();
    // 允许常用媒体/链接标签
    builder.add_tags([
        "a",
        "p",
        "div",
        "span",
        "img",
        "strong",
        "em",
        "ul",
        "ol",
        "li",
        "code",
        "pre",
        "blockquote",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "br",
        "hr",
        "table",
        "thead",
        "tbody",
        "th",
        "tr",
        "td",
    ]);
    builder.clean(input).to_string()
}

fn clean_url(u: &str) -> String {
    if let Ok(mut url) = Url::parse(u) {
        // 过滤常见跟踪参数
        let mut pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let trackers = [
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_term",
            "utm_content",
            "gclid",
            "fbclid",
            "mc_cid",
            "mc_eid",
            "ref",
            "ref_src",
        ];
        pairs.retain(|(k, _)| !trackers.contains(&k.as_str()));
        if pairs.is_empty() {
            url.set_query(None);
        } else {
            let new_query = pairs
                .into_iter()
                .map(|(k, v)| format!("{}={}", k, urlencoding::encode(&v)))
                .collect::<Vec<_>>()
                .join("&");
            url.set_query(Some(&new_query));
        }
        url.to_string()
    } else {
        u.to_string()
    }
}

fn apply_entry_filters(feed: &feed::Model, entries: &mut Vec<NormalizedEntry>) {
    let mut keep_regexes: Vec<Regex> = Vec::new();
    let mut block_regexes: Vec<Regex> = Vec::new();
    if let Some(ref s) = feed.keep_filter_entry_rules {
        for line in s.lines() {
            if let Ok(rx) = Regex::new(line.trim()) {
                keep_regexes.push(rx);
            }
        }
    }
    if let Some(ref s) = feed.block_filter_entry_rules {
        for line in s.lines() {
            if let Ok(rx) = Regex::new(line.trim()) {
                block_regexes.push(rx);
            }
        }
    }
    if keep_regexes.is_empty() && block_regexes.is_empty() {
        return;
    }

    entries.retain(|e| {
        let mut hay = String::new();
        if let Some(t) = &e.title {
            hay.push_str(t);
            hay.push('\n');
        }
        if let Some(s) = &e.summary {
            hay.push_str(s);
            hay.push('\n');
        }
        if let Some(c) = &e.content_html {
            hay.push_str(c);
        }
        // apply keep first: if any keep rules and none match, drop
        if !keep_regexes.is_empty() && !keep_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }
        // apply block: if any block matches, drop
        if block_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }
        true
    });
}

fn apply_rewrite_rules(input: &str, rules: &str) -> String {
    let mut out = input.to_string();
    for line in rules.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        // support sed-like: s/pattern/repl/
        if s.starts_with('s') && s.len() > 2 {
            let delim = s.chars().nth(1).unwrap();
            let parts: Vec<&str> = s[2..].split(delim).collect();
            if parts.len() >= 2 {
                let pat = parts.first().copied().unwrap_or("");
                let rep = parts.get(1).copied().unwrap_or("");
                if let Ok(rx) = Regex::new(pat) {
                    out = rx.replace_all(&out, rep).to_string();
                    continue;
                }
            }
        }
        // fallback: regex => replacement (=> delimiter)
        if let Some((pat, rep)) = s.split_once("=>") {
            if let Ok(rx) = Regex::new(pat.trim()) {
                out = rx.replace_all(&out, rep.trim()).to_string();
            }
        }
    }
    out
}

#[instrument(skip(feed, yaml))]
pub async fn refresh_rule_with_yaml_legacy(
    feed: &feed::Model,
    yaml: &str,
) -> Result<Vec<NormalizedEntry>> {
    let spec: RuleSpecV1 =
        captura_rules::v1::parse_rule_v1(yaml).map_err(|e| Error::Parse(e.to_string()))?;
    refresh_rule_v1(feed, &spec).await
}

/// Execute a v1 DSL rule defined as YAML for the given feed.
///
/// Currently supports `source.type = list_detail` with CSS/readability content
/// extraction. Other source types are rejected until implemented.
#[instrument(skip(feed, yaml))]
pub async fn refresh_rule_v1_with_yaml_legacy(
    feed: &feed::Model,
    yaml: &str,
) -> Result<Vec<NormalizedEntry>> {
    let spec: RuleSpecV1 =
        captura_rules::v1::parse_rule_v1(yaml).map_err(|e| Error::Parse(e.to_string()))?;
    refresh_rule_v1(feed, &spec).await
}

#[instrument(skip(feed, spec))]
pub async fn refresh_rule_v1_legacy(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    // 1) 优先尝试 Rust handler（对标 RSSHub 路由的代码级抓取能力）。
    if let Some(res) = handlers::execute_rust_handler_if_any(feed, spec).await {
        let mut entries = res?;
        // handler 负责构造完整 entries，这里仅统一应用 feed 级过滤规则。
        apply_entry_filters(feed, &mut entries);
        return Ok(entries);
    }

    // 2) 回退到 DSL v1 执行路径。
    let mut entries = match spec.source.kind {
        SourceType::ListDetail => execute_list_detail_v1(feed, spec).await,
        SourceType::SinglePage => execute_single_page_v1(feed, spec).await,
        SourceType::Json => execute_json_v1(feed, spec).await,
        SourceType::XPath => execute_xpath_v1(feed, spec).await,
    }?;

    // 先应用 DSL v1 规则级过滤（entry_include / entry_exclude）。
    apply_rule_filters_v1(spec, &mut entries);
    // 根据 DSL v1 filters.fetch_full_content_when + transform.content_merge
    // 条件性地抓取全文并合并。
    apply_full_content_when_v1(feed, spec, &mut entries).await?;
    // 最后应用 feed 级过滤（兼容 Miniflux keep/block 语义）。
    apply_entry_filters(feed, &mut entries);

    Ok(entries)
}

/// Merge rule param defaults with feed-level params (rule_params_json).
///
/// - Defaults come from `spec.params.defaults`.
/// - Feed params override defaults when keys collide.
pub(crate) fn merge_rule_params_v1(
    spec: &RuleSpecV1,
    feed_params: Option<&JsonValue>,
) -> Option<JsonValue> {
    let mut out = serde_json::Map::new();
    if let Some(p) = &spec.params {
        for (k, v) in p.defaults.iter() {
            out.insert(k.clone(), v.clone());
        }
    }
    if let Some(JsonValue::Object(map)) = feed_params {
        for (k, v) in map.iter() {
            out.insert(k.clone(), v.clone());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(JsonValue::Object(out))
    }
}

/// Execute a v1 rule with `source.type = list_detail`.
///
/// This is the most common pattern (news/blog listing + detail page).
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

    // Merge UA preference: rule-level fetch defaults > feed-level UA > fallback.
    let ua = spec
        .fetch
        .user_agent
        .clone()
        .or_else(|| feed.user_agent.clone())
        .unwrap_or_else(|| "captura/0.1".to_string());

    let client = Client::builder()
        .user_agent(ua)
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;

    // Map v1 fetch defaults into local fetch config for reuse of existing helpers.
    let fetch_cfg = FetchCfg {
        user_agent: Some(
            spec.fetch
                .user_agent
                .clone()
                .or_else(|| feed.user_agent.clone())
                .unwrap_or_else(|| "captura/0.1".to_string()),
        ),
        headers: None,
        smart: spec.fetch.smart,
        timeout_ms: spec.fetch.timeout_ms,
        respect_robots: spec.fetch.respect_robots,
        delay_ms: None,
        limit: None,
        proxy_url: None,
    };

    // Render list URL with params (rule params defaults + feed.rule_params_json).
    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let final_list_url = render_with_params(&list.request.url, params.as_ref());

    // Fetch list HTML (HTTP + optional spider smart).
    let html = fetch_html_strategy(&client, &final_list_url, &fetch_cfg, Some(feed)).await?;

    // 1) Collect links, titles, optional summaries from list page.
    let mut items: Vec<(Option<String>, Option<String>, Option<String>)> = Vec::new();
    {
        let doc = Html::parse_document(&html);
        let item_sel = Selector::parse(&list.item)
            .map_err(|e| Error::Parse(format!("invalid item selector: {e}")))?;
        for el in doc.select(&item_sel) {
            let link = list
                .link
                .as_ref()
                .and_then(|s| extract_attr(&el, s))
                .or_else(|| el.value().attr("href").map(|s| s.to_string()));
            let title = list.title.as_ref().and_then(|s| extract_text(&el, s));
            let summary = list.summary.as_ref().and_then(|s| extract_text(&el, s));
            items.push((link, title, summary));
        }
    }
    debug!(
        feed_id = feed.id,
        list_url = %final_list_url,
        items = items.len(),
        "refresh_rule_v1: collected list items"
    );

    // 2) Request detail pages and build entries.
    let mut entries = Vec::new();
    for (link, title, summary) in items {
        let url = link.as_deref().map(|u| absolutize(&final_list_url, u));

        // Content extraction strategy.
        let content_html: Option<String> = match content.mode {
            ContentMode::Readability => {
                readability_like_strategy_async(&client, url.as_deref(), &fetch_cfg, Some(feed))
                    .await
            }
            ContentMode::Css | ContentMode::JsonFragment => {
                if let Some(sel) = &content.selector {
                    if let Some(u) = &url {
                        Some(
                            fetch_and_select_strategy(&client, u, sel, &fetch_cfg, Some(feed))
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

        // v1: content.fallback/content_merge are not applied yet; summary is
        // preserved separately. Future versions may merge full content according
        // to transform.content_merge.

        entries.push(NormalizedEntry {
            guid: url.clone(),
            url,
            title,
            summary,
            content_html,
            author: None,
            published_at: None,
            enclosures: vec![],
            extras: serde_json::json!({}),
        });
    }

    Ok(entries)
}

/// Execute a v1 rule with `source.type = single_page`.
///
/// This treats the configured URL as a single logical entry.
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

    let client = Client::builder()
        .user_agent(ua)
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;

    let fetch_cfg = FetchCfg {
        user_agent: Some(
            spec.fetch
                .user_agent
                .clone()
                .or_else(|| feed.user_agent.clone())
                .unwrap_or_else(|| "captura/0.1".to_string()),
        ),
        headers: None,
        smart: spec.fetch.smart,
        timeout_ms: spec.fetch.timeout_ms.or(req.timeout_ms),
        respect_robots: spec.fetch.respect_robots,
        delay_ms: None,
        limit: None,
        proxy_url: None,
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
            readability_like_strategy_async(&client, Some(&final_url), &fetch_cfg, Some(feed)).await
        }
        ContentMode::Css | ContentMode::JsonFragment => {
            if let Some(sel) = &content.selector {
                Some(
                    fetch_and_select_strategy(&client, &final_url, sel, &fetch_cfg, Some(feed))
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

// Legacy JSON executor kept only for compatibility; it delegates to the
// consolidated implementation in `rules_engine`.
async fn execute_json_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    rules_engine::execute_json_v1(feed, spec).await
}


/// Execute a v1 rule with `source.type = xpath`.
///
/// 当前实现并未提供完整 XPath 解析器，而是针对常见模式（如
/// `//ul/li`, `.//h2/text()`, `.//a/@href`, `.//div[@class='entry']`）
/// 做一个轻量级的 XPath → CSS 近似转换，然后复用现有 CSS 解析逻辑。
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

    let client = Client::builder()
        .user_agent(ua)
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;

    let fetch_cfg = FetchCfg {
        user_agent: Some(
            spec.fetch
                .user_agent
                .clone()
                .or_else(|| feed.user_agent.clone())
                .unwrap_or_else(|| "captura/0.1".to_string()),
        ),
        headers: req.headers.clone(),
        smart: spec.fetch.smart.or(req.smart),
        timeout_ms: spec.fetch.timeout_ms.or(req.timeout_ms),
        respect_robots: spec.fetch.respect_robots.or(req.respect_robots),
        delay_ms: None,
        limit: None,
        proxy_url: None,
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let final_url = render_with_params(&req.url, params.as_ref());

    let html = fetch_html_strategy(&client, &final_url, &fetch_cfg, Some(feed)).await?;
    let doc = Html::parse_document(&html);

    // 选择 item 集合。
    let item_sel_str = xpath_to_css_like(&xpath.item);
    let item_sel = Selector::parse(&item_sel_str)
        .map_err(|e| Error::Parse(format!("invalid xpath.item selector '{item_sel_str}': {e}")))?;

    let mut entries = Vec::new();
    for el in doc.select(&item_sel) {
        // 标题
        let title = xpath.title.as_deref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            if css.contains('@') {
                extract_attr(&el, &css)
            } else {
                extract_text(&el, &css)
            }
        });

        // 链接 URL
        let raw_url = xpath.url.as_deref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            if css.contains('@') {
                extract_attr(&el, &css)
            } else {
                extract_text(&el, &css)
            }
        });
        let url = raw_url.as_deref().map(|u| absolutize(&final_url, u));

        // 正文 HTML 片段
        let content_html = xpath
            .content_html
            .as_deref()
            .and_then(|expr| {
                let css = xpath_to_css_like(expr);
                extract_html(&el, &css)
            })
            .map(|html| sanitize_html(&html));

        entries.push(NormalizedEntry {
            guid: url.clone(),
            url,
            title,
            summary: None,
            content_html,
            author: None,
            // TODO: 支持 xpath.published_at.expr / format
            published_at: None,
            enclosures: vec![],
            extras: serde_json::json!({}),
        });
    }

    Ok(entries)
}

pub(crate) fn extract_attr(parent: &scraper::ElementRef, expr: &str) -> Option<String> {
    if let Some((sel, attr)) = expr.split_once('@') {
        if let Ok(s) = Selector::parse(sel) {
            if let Some(el) = parent.select(&s).next() {
                return el.value().attr(attr).map(|v| v.to_string());
            }
        }
    }
    None
}

pub(crate) fn extract_text(parent: &scraper::ElementRef, sel: &str) -> Option<String> {
    if let Ok(s) = Selector::parse(sel) {
        if let Some(el) = parent.select(&s).next() {
            return Some(el.text().collect::<Vec<_>>().join("").trim().to_string());
        }
    }
    None
}

async fn fetch_and_select_strategy(
    client: &Client,
    url: &str,
    sel: &str,
    fetch: &rules_engine::FetchCfg,
    feed: Option<&feed::Model>,
) -> Result<String> {
    let html = rules_engine::fetch_html_strategy(client, url, fetch, feed).await?;
    let doc = Html::parse_document(&html);
    let s = Selector::parse(sel).map_err(|e| anyhow::anyhow!("invalid selector: {e}"))?;
    let mut out = String::new();
    for el in doc.select(&s) {
        out.push_str(&el.html());
    }
    Ok(sanitize_html(&out))
}

async fn readability_like_strategy_async(
    client: &Client,
    url: Option<&str>,
    fetch: &rules_engine::FetchCfg,
    feed: Option<&feed::Model>,
) -> Option<String> {
    let url = match url {
        Some(u) => u,
        None => return None,
    };
    let html = match rules_engine::fetch_html_strategy(client, url, fetch, feed).await {
        Ok(h) => h,
        Err(_) => return None,
    };
    // 先尝试 dom_smoothie，可读性失败时记录日志并回退到简单 heuristics。
    if let Some(article) = crate::extractor::extract_with_dom_smoothie(&html, Some(url)) {
        return Some(sanitize_html(&article.content));
    } else {
        debug!(
            url = url,
            "dom_smoothie readability failed in rule pipeline, falling back to simple heuristics"
        );
    }

    let doc = Html::parse_document(&html);
    crate::extractor::readability_pick_raw(&doc)
        .map(|raw| sanitize_html(&raw))
        .or_else(|| Some(sanitize_html(&html)))
}

fn extract_html(parent: &scraper::ElementRef, sel: &str) -> Option<String> {
    if let Ok(s) = Selector::parse(sel) {
        let mut out = String::new();
        for el in parent.select(&s) {
            out.push_str(&el.html());
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    } else {
        None
    }
}

/// 轻量级 XPath → CSS 近似转换，只覆盖 DSL 文档中典型示例。
fn xpath_to_css_like(expr: &str) -> String {
    let mut s = expr.trim();

    // 去掉前缀 //、.//、./
    if let Some(rest) = s.strip_prefix("//") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix(".//") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }

    // attr 访问，例如 "a/@href"
    if let Some(idx) = s.rfind("/@") {
        let (node_path, attr) = s.split_at(idx);
        let attr = &attr[2..];
        let tag = node_path
            .rsplit('/')
            .find(|seg| !seg.is_empty())
            .unwrap_or(node_path)
            .trim();
        if tag.is_empty() {
            return format!("@{}", attr);
        }
        return format!("{}@{}", simple_xpath_node_to_css(tag), attr);
    }

    // text() 访问，例如 "h2/text()"
    if let Some(idx) = s.rfind("/text()") {
        let node_path = &s[..idx];
        let tag = node_path
            .rsplit('/')
            .find(|seg| !seg.is_empty())
            .unwrap_or(node_path)
            .trim();
        if tag.is_empty() {
            return "*".to_string();
        }
        return simple_xpath_node_to_css(tag);
    } else if s == "text()" {
        return "*".to_string();
    }

    // 节点过滤，例如 "div[@class='entry-content']"
    if let Some(start) = s.find('[') {
        if let Some(end) = s.rfind(']') {
            let base = s[..start].trim();
            let cond = &s[start + 1..end];
            if let Some(rest) = cond.trim().strip_prefix('@') {
                if let Some((attr, val_raw)) = rest.split_once('=') {
                    let attr = attr.trim();
                    let val = val_raw.trim().trim_matches('\'').trim_matches('"');
                    if attr.eq_ignore_ascii_case("class") {
                        let mut css = base.to_string();
                        for cls in val.split_whitespace() {
                            if !cls.is_empty() {
                                css.push('.');
                                css.push_str(cls);
                            }
                        }
                        return css;
                    } else if attr.eq_ignore_ascii_case("id") {
                        let mut css = base.to_string();
                        css.push('#');
                        css.push_str(val);
                        return css;
                    } else {
                        return format!(r#"{}[{}="{}"]"#, base, attr, val);
                    }
                }
            }
        }
    }

    // 路径分段，例如 "ul/li" → "ul li"
    if s.contains('/') {
        let parts: Vec<&str> = s.split('/').filter(|seg| !seg.is_empty()).collect();
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }

    simple_xpath_node_to_css(s)
}

fn simple_xpath_node_to_css(node: &str) -> String {
    node.trim().to_string()
}

/// 规则级 DSL v1 过滤：entry_include / entry_exclude。
fn apply_rule_filters_v1(spec: &RuleSpecV1, entries: &mut Vec<NormalizedEntry>) {
    let Some(filters) = &spec.filters else {
        return;
    };
    let mut keep_regexes: Vec<Regex> = Vec::new();
    let mut block_regexes: Vec<Regex> = Vec::new();

    if let Some(list) = &filters.entry_include {
        for line in list {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rx) = Regex::new(line) {
                keep_regexes.push(rx);
            }
        }
    }
    if let Some(list) = &filters.entry_exclude {
        for line in list {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rx) = Regex::new(line) {
                block_regexes.push(rx);
            }
        }
    }

    if keep_regexes.is_empty() && block_regexes.is_empty() {
        return;
    }

    entries.retain(|e| {
        let mut hay = String::new();
        if let Some(t) = &e.title {
            hay.push_str(t);
            hay.push('\n');
        }
        if let Some(s) = &e.summary {
            hay.push_str(s);
            hay.push('\n');
        }
        if let Some(c) = &e.content_html {
            hay.push_str(c);
        }

        if !keep_regexes.is_empty() && !keep_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }

        if block_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }

        true
    });
}

enum FullContentField {
    Title,
    Summary,
    ContentHtml,
}

struct FullContentMatcher {
    field: FullContentField,
    regex: Regex,
}

/// 根据 DSL v1 filters.fetch_full_content_when + transform.content_merge
/// 条件性地对条目进行全文抓取和内容合并。
async fn apply_full_content_when_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
    entries: &mut Vec<NormalizedEntry>,
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
        if let Ok(rx) = Regex::new(&c.regex) {
            matchers.push(FullContentMatcher { field, regex: rx });
        }
    }
    if matchers.is_empty() {
        return Ok(());
    }

    // content_merge.mode 缺省为 replace。
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

        match crate::extractor::fetch_and_extract_entry(&url, feed).await {
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

#[cfg(test)]
mod live_tests {
    use super::*;
    use captura_storage::entity::feed;
    use chrono::{FixedOffset, Utc};

    fn should_run_live() -> bool {
        std::env::var("CAPTURA_TEST_LIVE")
            .ok()
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
    }

    fn make_feed(feed_url: &str, ftype: feed::FeedType) -> feed::Model {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        feed::Model {
            id: 0,
            user_id: 1,
            category_id: None,
            r#type: ftype,
            title: Some("live".into()),
            site_url: None,
            feed_url: feed_url.into(),
            favicon_id: None,
            rule_id: None,
            rule_params_json: None,
            user_agent: Some("captura-tests/0.1".into()),
            username: None,
            password: None,
            headers_json: None,
            cookies: None,
            proxy_url: None,
            fetch_via_proxy: false,
            disable_http2: false,
            allow_invalid_certs: false,
            request_timeout_ms: Some(15000),
            checked_at: None,
            next_run_at: None,
            etag: None,
            last_modified: None,
            last_status: None,
            last_error_message: None,
            error_count: 0,
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
        }
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_rust_blog_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed("https://blog.rust-lang.org/feed.xml", feed::FeedType::Atom);
        let entries = refresh_feed(&f).await.expect("fetch rust blog feed");
        assert!(!entries.is_empty(), "should fetch at least one entry");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_xkcd_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed("https://xkcd.com/atom.xml", feed::FeedType::Atom);
        let entries = refresh_feed(&f).await.expect("fetch xkcd feed");
        assert!(!entries.is_empty(), "should fetch at least one entry");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_jsonfeed_org_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed("https://jsonfeed.org/feed.json", feed::FeedType::Json);
        let entries = refresh_feed(&f).await.expect("fetch jsonfeed.org feed");
        assert!(!entries.is_empty(), "json feed should return entries");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_daring_fireball_json_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed(
            "https://daringfireball.net/feeds/json",
            feed::FeedType::Json,
        );
        let entries = refresh_feed(&f)
            .await
            .expect("fetch daring fireball json feed");
        assert!(!entries.is_empty(), "daring fireball should return entries");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_bbc_news_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed("http://feeds.bbci.co.uk/news/rss.xml", feed::FeedType::Rss);
        let entries = refresh_feed(&f).await.expect("fetch bbc news feed");
        assert!(!entries.is_empty(), "bbc should return entries");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_nhk_japanese_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed(
            "https://www3.nhk.or.jp/rss/news/cat0.xml",
            feed::FeedType::Rss,
        );
        let entries = refresh_feed(&f).await.expect("fetch nhk feed");
        assert!(!entries.is_empty(), "nhk should return entries");
        let has_non_ascii = entries.iter().any(|e| {
            e.title
                .as_deref()
                .map(|t| t.chars().any(|c| c as u32 > 127))
                .unwrap_or(false)
        });
        assert!(
            has_non_ascii,
            "NHK titles often include non-ascii characters"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_solidot_cn_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed("https://www.solidot.org/index.rss", feed::FeedType::Rss);
        let entries = refresh_feed(&f).await.expect("fetch solidot feed");
        assert!(!entries.is_empty(), "solidot should return entries");
        // 非 ASCII 标题/摘要覆盖（不强制断言具体值，仅断言存在）
        let has_non_ascii = entries.iter().any(|e| {
            e.title
                .as_deref()
                .map(|t| t.chars().any(|c| c as u32 > 127))
                .unwrap_or(false)
        });
        assert!(has_non_ascii, "should contain non-ascii titles");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_nasa_multimedia_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed(
            "https://www.nasa.gov/rss/dyn/breaking_news.rss",
            feed::FeedType::Rss,
        );
        let entries = refresh_feed(&f).await.expect("fetch nasa feed");
        assert!(!entries.is_empty(), "nasa should return entries");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_arstechnica_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed(
            "https://feeds.arstechnica.com/arstechnica/index",
            feed::FeedType::Rss,
        );
        let entries = refresh_feed(&f).await.expect("fetch arstechnica feed");
        assert!(!entries.is_empty(), "arstechnica should return entries");
    }

    #[tokio::test]
    #[ignore]
    async fn refresh_theverge_live() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let f = make_feed(
            "https://www.theverge.com/rss/index.xml",
            feed::FeedType::Rss,
        );
        let entries = refresh_feed(&f).await.expect("fetch theverge feed");
        assert!(!entries.is_empty(), "theverge should return entries");
    }
}
pub(crate) fn render_with_params(input: &str, params: Option<&serde_json::Value>) -> String {
    let mut s = input.to_string();
    let Some(serde_json::Value::Object(map)) = params else {
        return s;
    };
    for (k, v) in map.iter() {
        if let Some(val) = v.as_str() {
            let needle1 = format!(":{}", k);
            let needle2 = format!("{{{}}}", k);
            s = s.replace(&needle1, val);
            s = s.replace(&needle2, val);
        } else {
            let needle2 = format!("{{{}}}", k);
            s = s.replace(&needle2, &v.to_string());
        }
    }
    s
}

fn absolutize(base: &str, href: &str) -> String {
    if Url::parse(href).is_ok() {
        return href.to_string();
    }
    if let Ok(b) = Url::parse(base) {
        if let Ok(j) = b.join(href) {
            return j.to_string();
        }
    }
    href.to_string()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_render_with_params() {
        let url = "https://example.com/list/{cat}/:page";
        let params = serde_json::json!({"cat":"news","page":"2"});
        let out = render_with_params(url, Some(&params));
        assert!(out.contains("/news/2"));
    }

    #[test]
    fn test_absolutize() {
        let base = "https://news.ycombinator.com/";
        let href = "item?id=123";
        let out = absolutize(base, href);
        assert_eq!(out, "https://news.ycombinator.com/item?id=123");
    }
}
