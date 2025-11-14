//! HTML fetching and content extraction helpers.
//! This module centralizes entry-level fetching + extraction so
//! API 兼容层和内部 pipeline 都可以复用同一套逻辑。

use captura_common::{Error, Result};
use captura_storage::entity::feed;
use reqwest::Client;
use scraper::{Html, Selector};

/// 抽取结果：正文 HTML 与可选的新标题。
#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub content_html: String,
    pub title: Option<String>,
}

/// 为单个条目按订阅配置构建 HTTP 客户端。
fn build_http_client_for_feed(f: &feed::Model) -> Result<Client> {
    let mut http = Client::builder();
    if let Some(ua) = f.user_agent.clone() {
        http = http.user_agent(ua);
    }
    if let Some(ms) = f.request_timeout_ms {
        http = http.timeout(std::time::Duration::from_millis(ms as u64));
    }
    if f.allow_invalid_certs {
        http = http.danger_accept_invalid_certs(true);
    }
    if f.disable_http2 {
        http = http.http1_only();
    }
    if f.fetch_via_proxy {
        if let Some(ref p) = f.proxy_url {
            if !p.is_empty() {
                if let Ok(proxy) = reqwest::Proxy::all(p) {
                    http = http.proxy(proxy);
                }
            }
        }
    }
    http.build().map_err(|e| Error::Network(e.to_string()))
}

/// 按 Miniflux 语义应用 Scraper Rules（每行一个 CSS 选择器）。
fn apply_scraper_rules(doc: &Html, rules: &str) -> Option<String> {
    let selector_lines: Vec<&str> = rules
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    if selector_lines.is_empty() {
        return None;
    }
    let mut buf = String::new();
    for sel_str in selector_lines {
        if let Ok(sel) = Selector::parse(sel_str) {
            for el in doc.select(&sel) {
                buf.push_str(&el.html());
            }
        }
    }
    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

/// 简化版的 Readability 择优逻辑。
///
/// 当前仅尝试一组常见正文选择器，返回原始 HTML 片段。
/// 后续可以在此位置替换为完整的 Readability 实现或绑定。
pub fn readability_pick_raw(doc: &Html) -> Option<String> {
    let candidates = [
        "article",
        "main",
        "#content",
        ".post",
        ".article",
        ".entry-content",
    ];
    for c in candidates.iter() {
        if let Ok(sel) = Selector::parse(c) {
            if let Some(el) = doc.select(&sel).next() {
                return Some(el.html());
            }
        }
    }
    None
}

/// 从文档中提取 `<title>` 文本。
fn extract_title(doc: &Html) -> Option<String> {
    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = doc.select(&sel).next() {
            let t = el.text().collect::<Vec<_>>().join("").trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

/// 根据订阅配置抓取某个条目的网页，并尝试抽取正文与标题。
///
/// - 优先使用 feed.scraper_rules（CSS 选择器，每行一条）。
/// - 否则使用简化版 Readability 逻辑。
/// - 若仍然失败，则退回整页 HTML。
pub async fn fetch_and_extract_entry(page_url: &str, f: &feed::Model) -> Result<ExtractResult> {
    let http = build_http_client_for_feed(f)?;
    let mut req = http.get(page_url);
    if let Some(ref c) = f.cookies {
        if !c.is_empty() {
            req = req.header(reqwest::header::COOKIE, c.clone());
        }
    }
    if let Some(ref u) = f.username {
        // 密码可为空字符串
        req = req.basic_auth(u, f.password.clone());
    }
    let html = req
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&html);
    let mut content_html: Option<String> = None;

    if let Some(ref rules) = f.scraper_rules {
        content_html = apply_scraper_rules(&doc, rules);
    }

    if content_html.is_none() {
        content_html = readability_pick_raw(&doc);
        if content_html.is_none() {
            content_html = Some(html.clone());
        }
    }

    let title = extract_title(&doc);

    Ok(ExtractResult {
        content_html: content_html.unwrap_or_default(),
        title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readability_picks_article() {
        let html = r#"<html><body><article><p>Hello</p></article><div class='content'><p>Other</p></div></body></html>"#;
        let doc = Html::parse_document(html);
        let out = readability_pick_raw(&doc).unwrap();
        assert!(out.contains("Hello"));
    }

    #[test]
    fn readability_picks_common_selector() {
        let html = r#"<html><body><div class='post'><p>PickMe</p></div><div class='article'><p>Alt</p></div></body></html>"#;
        let doc = Html::parse_document(html);
        let out = readability_pick_raw(&doc).unwrap();
        assert!(out.contains("PickMe") || out.contains("Alt"));
    }

    #[test]
    fn readability_fallback_none() {
        let html = r#"<html><body>Plain <b>text</b></body></html>"#;
        let doc = Html::parse_document(html);
        assert!(readability_pick_raw(&doc).is_none());
    }
}

