//! Pipeline orchestrates fetcher / crawler / rules execution
//! into normalized entries ready for persistence.

use captura_common::{NormalizedEntry, Result};
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_storage::entity::feed;
use regex::Regex;
use reqwest::header::HeaderMap;
use reqwest::Client;
use scraper::{Html, Selector};
use tracing::{debug, instrument};
use url::Url;

mod handlers;
mod hub_bridge;
mod hub_utils;
mod http_client;
mod rules_engine;

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
