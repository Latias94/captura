use captura_common::{Error, Result};
use dom_smoothie::{Config as DsConfig, Readability as DsReadability};
use reqwest::Client;
use scraper::{Html, Selector};
use tracing::warn;

/// Extraction result: article HTML and optional new title.
#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub content_html: String,
    pub title: Option<String>,
}

/// DTO-style configuration for entry-level full-content extraction.
///
/// This type is deliberately decoupled from database models so it can be
/// reused by the pipeline crate, Hub handlers, CLI/TUI tools, etc.
#[derive(Debug, Clone, Default)]
pub struct EntryExtractConfig {
    pub page_url: String,
    pub scraper_rules: Option<String>,
    pub cookies: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub user_agent: Option<String>,
    pub request_timeout_ms: Option<u64>,
}

fn build_http_client(user_agent: Option<String>, timeout_ms: Option<u64>) -> Result<Client> {
    let mut builder = Client::builder();
    if let Some(ua) = user_agent {
        builder = builder.user_agent(ua);
    }
    if let Some(ms) = timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(ms));
    }
    builder.build().map_err(|e| Error::Network(e.to_string()))
}

/// Apply scraper_rules according to Miniflux semantics (one CSS selector per line).
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

/// Simplified Readability-like heuristic.
///
/// Currently tries a small set of common content selectors and returns raw HTML snippets.
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

/// Extract `<title>` text from the document.
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

/// Use dom_smoothie to extract readable content and metadata.
///
/// Returns a subset of `Article` (currently only title/content).
fn extract_with_dom_smoothie(html: &str, url: Option<&str>) -> Option<dom_smoothie::Article> {
    let cfg = DsConfig {
        ..Default::default()
    };
    let mut readability = DsReadability::new(html, url, Some(cfg)).ok()?;
    readability.parse().ok()
}

/// Fetch and extract entry content using an `EntryExtractConfig`.
pub async fn fetch_and_extract_entry_dto(cfg: &EntryExtractConfig) -> Result<ExtractResult> {
    let http = build_http_client(cfg.user_agent.clone(), cfg.request_timeout_ms)?;
    let mut req = http.get(&cfg.page_url);
    if let Some(ref c) = cfg.cookies {
        if !c.is_empty() {
            req = req.header(reqwest::header::COOKIE, c.clone());
        }
    }
    if let Some(ref u) = cfg.username {
        req = req.basic_auth(u, cfg.password.clone());
    }
    let html = req
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    extract_from_html(&html, Some(&cfg.page_url), cfg.scraper_rules.as_deref())
}

/// Convenience helper: fetch and extract entry content for a simple URL,
/// without advanced configuration.
pub async fn fetch_and_extract_entry(page_url: &str) -> Result<ExtractResult> {
    let cfg = EntryExtractConfig {
        page_url: page_url.to_string(),
        ..Default::default()
    };
    fetch_and_extract_entry_dto(&cfg).await
}

pub fn extract_from_html(
    html: &str,
    page_url: Option<&str>,
    scraper_rules: Option<&str>,
) -> Result<ExtractResult> {
    let doc = Html::parse_document(html);

    // 1) Prefer scraper_rules (compatible with Miniflux semantics).
    if let Some(rules) = scraper_rules {
        if let Some(content) = apply_scraper_rules(&doc, rules) {
            let title = extract_title(&doc);
            return Ok(ExtractResult {
                content_html: content,
                title,
            });
        }
    }

    // 2) Try dom_smoothie (Rust implementation based on mozilla/readability).
    if let Some(article) = extract_with_dom_smoothie(html, page_url) {
        let article_title = article.title.clone();
        let title = if article_title.trim().is_empty() {
            extract_title(&doc)
        } else {
            Some(article_title)
        };
        return Ok(ExtractResult {
            content_html: article.content.to_string(),
            title,
        });
    } else if let Some(url) = page_url {
        warn!(
            url = url,
            "dom_smoothie readability failed, falling back to simple heuristics"
        );
    }

    // 3) Fall back to simplified heuristics (readability_pick_raw), then to the full HTML.
    let mut content_html = readability_pick_raw(&doc);
    if content_html.is_none() {
        content_html = Some(html.to_string());
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
}
