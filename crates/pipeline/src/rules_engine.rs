use captura_common::{Error, NormalizedEntry, Result};
use captura_crawler::{self as crawler, CrawlOptions};
use captura_rules::v1::{ContentMergeMode, ContentMode, RuleSpecV1, SourceType};
use captura_storage::entity::feed;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value as JsonValue;
use std::time::Duration;
use tracing::{debug, instrument};

use crate::{apply_entry_filters, extractor, render_with_params, sanitize_html};

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
        let mut opts = CrawlOptions::default();
        opts.user_agent = fetch.user_agent.clone();
        opts.respect_robots = fetch.respect_robots.unwrap_or(true);
        if let Some(d) = fetch.delay_ms {
            opts.delay_ms = d;
        }
        opts.limit = fetch.limit;
        if let Some(proxy) = &fetch.proxy_url {
            if !proxy.is_empty() {
                opts.proxy_url = Some(proxy.clone());
            }
        }

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
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("http status {}", status)));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    Ok(text)
}

/// Parse YAML into `RuleSpecV1` and execute it for the given feed.
#[instrument(skip(feed, yaml))]
pub async fn refresh_rule_with_yaml(
    feed: &feed::Model,
    yaml: &str,
) -> Result<Vec<NormalizedEntry>> {
    let spec: RuleSpecV1 =
        captura_rules::v1::parse_rule_v1(yaml).map_err(|e| Error::Parse(e.to_string()))?;
    refresh_rule_v1(feed, &spec).await
}

/// Execute a v1 DSL rule defined as YAML for the given feed.
#[instrument(skip(feed, yaml))]
pub async fn refresh_rule_v1_with_yaml(
    feed: &feed::Model,
    yaml: &str,
) -> Result<Vec<NormalizedEntry>> {
    let spec: RuleSpecV1 =
        captura_rules::v1::parse_rule_v1(yaml).map_err(|e| Error::Parse(e.to_string()))?;
    refresh_rule_v1(feed, &spec).await
}

/// Core entrypoint for executing a parsed v1 rule.
#[instrument(skip(feed, spec))]
pub async fn refresh_rule_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    // Prefer Rust handlers (Hub routes) when available.
    if let Some(res) = crate::handlers::execute_rust_handler_if_any(feed, spec).await {
        let mut entries = res?;
        apply_entry_filters(feed, &mut entries);
        return Ok(entries);
    }

    // Fallback to pure DSL execution.
    let mut entries = match spec.source.kind {
        SourceType::ListDetail => execute_list_detail_v1(feed, spec).await,
        SourceType::SinglePage => execute_single_page_v1(feed, spec).await,
        SourceType::Json => execute_json_v1(feed, spec).await,
        SourceType::XPath => execute_xpath_v1(feed, spec).await,
    }?;

    apply_rule_filters_v1(spec, &mut entries);
    apply_full_content_when_v1(feed, spec, &mut entries).await?;
    apply_entry_filters(feed, &mut entries);

    Ok(entries)
}

/// Navigate a JSON value using simple dot-notation (e.g. "items", "data.items").
fn json_get_path<'a>(v: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(v);
    }
    let mut cur = v;
    for part in path.split('.') {
        match cur {
            JsonValue::Object(map) => {
                cur = map.get(part)?;
            }
            _ => return None,
        }
    }
    Some(cur)
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

fn xpath_to_css_like(expr: &str) -> String {
    let mut s = expr.trim();

    if let Some(rest) = s.strip_prefix("//") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix(".//") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }

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

/// Merge rule param defaults with feed-level params (rule_params_json).
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

    let client =
        crate::http_client::client_for_feed(feed, Some(ua), spec.fetch.timeout_ms)?;

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
        proxy_url: None,
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let final_list_url = render_with_params(&list.request.url, params.as_ref());

    let html = fetch_html_strategy(&client, &final_list_url, &fetch_cfg, Some(feed)).await?;

    let mut items: Vec<(Option<String>, Option<String>, Option<String>)> = Vec::new();
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
            items.push((link, title, summary));
        }
    }
    debug!(
        feed_id = feed.id,
        list_url = %final_list_url,
        items = items.len(),
        "refresh_rule_v1: collected list items"
    );

    let mut entries = Vec::new();
    for (link, title, summary) in items {
        let url = link
            .as_deref()
            .map(|u| crate::absolutize(&final_list_url, u));

        let content_html: Option<String> = match content.mode {
            ContentMode::Readability => {
                crate::readability_like_strategy_async(&client, url.as_deref(), &fetch_cfg, Some(feed))
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
            crate::readability_like_strategy_async(&client, Some(&final_url), &fetch_cfg, Some(feed))
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

/// Execute a v1 rule with `source.type = json` (basic support).
///
/// This implementation supports:
/// - direct JSON responses via `source.request`, or
/// - JSON embedded in HTML via `source.from_html`.
///
/// Advanced timestamp and enclosure mapping can be extended later.
pub(crate) async fn execute_json_v1(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Result<Vec<NormalizedEntry>> {
    let root_path = spec
        .source
        .root
        .as_ref()
        .ok_or_else(|| Error::Config("source.root is required for json".into()))?;
    let mapping = spec
        .source
        .mapping
        .as_ref()
        .ok_or_else(|| Error::Config("source.mapping is required for json".into()))?;

    let ua = spec
        .fetch
        .user_agent
        .clone()
        .or_else(|| feed.user_agent.clone())
        .unwrap_or_else(|| "captura/0.1".to_string());

    let client = crate::http_client::client_for_feed(feed, Some(ua), spec.fetch.timeout_ms)?;

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());

    let json_root: JsonValue = if let Some(from_html) = &spec.source.from_html {
        let html_req = if let Some(req) = from_html.request.as_ref() {
            req
        } else if let Some(req) = spec.source.request.as_ref() {
            req
        } else {
            return Err(Error::Config(
                "either source.request or from_html.request is required when using from_html"
                    .into(),
            ));
        };

        let fetch_cfg = FetchCfg {
            user_agent: spec
                .fetch
                .user_agent
                .clone()
                .or_else(|| feed.user_agent.clone()),
            headers: None,
            smart: spec.fetch.smart,
            timeout_ms: spec.fetch.timeout_ms.or(html_req.timeout_ms),
            respect_robots: spec.fetch.respect_robots,
            delay_ms: None,
            limit: None,
            proxy_url: None,
        };
        let final_html_url = render_with_params(&html_req.url, params.as_ref());
        let html = fetch_html_strategy(&client, &final_html_url, &fetch_cfg, Some(feed)).await?;

        let doc = Html::parse_document(&html);
        let sel = Selector::parse(&from_html.selector)
            .map_err(|e| Error::Parse(format!("invalid from_html selector: {e}")))?;
        let mut fragments: Vec<JsonValue> = Vec::new();
        for el in doc.select(&sel) {
            let text = el.text().collect::<Vec<_>>().join("").trim().to_string();
            if text.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<JsonValue>(&text) {
                fragments.push(v);
                if !from_html.multiple.unwrap_or(false) {
                    break;
                }
            }
        }
        if fragments.is_empty() {
            return Err(Error::Parse("no JSON fragments extracted from HTML".into()));
        }
        if from_html.multiple.unwrap_or(false) {
            JsonValue::Array(fragments)
        } else {
            fragments.into_iter().next().unwrap()
        }
    } else {
        let req = spec
            .source
            .request
            .as_ref()
            .ok_or_else(|| Error::Config("source.request is required for json".into()))?;
        let final_url = render_with_params(&req.url, params.as_ref());
        let resp = client
            .get(&final_url)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let text = resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        serde_json::from_str(&text).map_err(|e| Error::Parse(format!("invalid json: {e}")))?
    };

    let root = json_get_path(&json_root, root_path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("json root is not an array".into()))?;

    let mut entries = Vec::new();
    for item in root {
        let title = mapping
            .title
            .as_deref()
            .and_then(|p| json_get_path(item, p))
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let url = mapping
            .url
            .as_deref()
            .and_then(|p| json_get_path(item, p))
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let summary = mapping
            .summary
            .as_deref()
            .and_then(|p| json_get_path(item, p))
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let content_html = mapping
            .content_html
            .as_deref()
            .and_then(|p| json_get_path(item, p))
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let author = mapping
            .author
            .as_deref()
            .and_then(|p| json_get_path(item, p))
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let mut enclosures = Vec::new();
        if let Some(enc_map) = &mapping.enclosure {
            let enc_url = enc_map
                .url
                .as_deref()
                .and_then(|p| json_get_path(item, p))
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            if let Some(enc_url) = enc_url {
                let enc_type = enc_map
                    .r#type
                    .as_deref()
                    .and_then(|p| json_get_path(item, p))
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                let enc_len = enc_map
                    .length
                    .as_deref()
                    .and_then(|p| json_get_path(item, p))
                    .and_then(|v| v.as_i64());
                enclosures.push(captura_common::Enclosure {
                    url: enc_url,
                    r#type: enc_type,
                    length: enc_len,
                    kind: None,
                });
            }
        }

        let entry_url = url.clone();

        entries.push(NormalizedEntry {
            guid: entry_url.clone(),
            url: entry_url,
            title,
            summary,
            content_html,
            author,
            published_at: None,
            enclosures,
            extras: serde_json::json!({}),
        });
    }

    Ok(entries)
}

/// Execute a v1 rule with `source.type = xpath`.
///
/// This implementation supports a pragmatic subset of XPath by converting
/// XPath-like expressions into CSS selectors used by `scraper`.
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

    let client = crate::http_client::client_for_feed(feed, Some(ua), spec.fetch.timeout_ms)?;

    let fetch_cfg = FetchCfg {
        user_agent: spec
            .fetch
            .user_agent
            .clone()
            .or_else(|| feed.user_agent.clone()),
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

    let item_sel_str = xpath_to_css_like(&xpath.item);
    let item_sel = Selector::parse(&item_sel_str)
        .map_err(|e| Error::Parse(format!("invalid xpath.item selector '{item_sel_str}': {e}")))?;

    let mut entries = Vec::new();
    for el in doc.select(&item_sel) {
        let title = xpath.title.as_deref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            if css.contains('@') {
                crate::extract_attr(&el, &css)
            } else {
                crate::extract_text(&el, &css)
            }
        });

        let raw_url = xpath.url.as_deref().and_then(|expr| {
            let css = xpath_to_css_like(expr);
            if css.contains('@') {
                crate::extract_attr(&el, &css)
            } else {
                crate::extract_text(&el, &css)
            }
        });
        let url = raw_url.as_deref().map(|u| crate::absolutize(&final_url, u));

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
            // TODO: support xpath.published_at.expr / format
            published_at: None,
            enclosures: vec![],
            extras: serde_json::json!({}),
        });
    }

    Ok(entries)
}
