//! HTML fetching and content extraction helpers.
//! This module centralizes entry-level fetching + extraction so
//! both the API compatibility layers and the internal pipeline can reuse the same logic.

use captura_common::{Error, Result};
use captura_storage::entity::feed;
use scraper::{Html, Selector};
use tracing::warn;

use dom_smoothie::{Config as DsConfig, Readability as DsReadability};

/// Extraction result: article HTML and optional new title.
#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub content_html: String,
    pub title: Option<String>,
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
/// Can be replaced with a full Readability implementation or binding in the future.
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

/// Fetch the page for a given entry according to feed configuration and try to extract content and title.
///
/// - Prefer feed.scraper_rules (CSS selectors, one per line).
/// - Otherwise, use the simplified Readability heuristic.
/// - If everything fails, fall back to the full HTML page.
pub async fn fetch_and_extract_entry(page_url: &str, f: &feed::Model) -> Result<ExtractResult> {
    let http = crate::http_client::client_for_feed(f, None, None)?;
    let mut req = http.get(page_url);
    if let Some(ref c) = f.cookies {
        if !c.is_empty() {
            req = req.header(reqwest::header::COOKIE, c.clone());
        }
    }
    if let Some(ref u) = f.username {
        // Password may be an empty string
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

    // 1) Prefer user-configured scraper_rules (compatible with Miniflux).
    if let Some(ref rules) = f.scraper_rules {
        if let Some(content) = apply_scraper_rules(&doc, rules) {
            let title = extract_title(&doc);
            return Ok(ExtractResult {
                content_html: content,
                title,
            });
        }
    }

    // 2) Try dom_smoothie (Rust implementation based on mozilla/readability).
    if let Some(article) = extract_with_dom_smoothie(&html, Some(page_url)) {
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
    } else {
        warn!(
            url = page_url,
            "dom_smoothie readability failed, falling back to simple heuristics"
        );
    }

    // 3) Fall back to simplified heuristics (readability_pick_raw), then to the full HTML.
    let mut content_html = readability_pick_raw(&doc);
    if content_html.is_none() {
        content_html = Some(html.clone());
    }

    let title = extract_title(&doc);

    Ok(ExtractResult {
        content_html: content_html.unwrap_or_default(),
        title,
    })
}

/// Use dom_smoothie to extract readable content and metadata.
///
/// - Returns a subset of `Article` (currently only title/content).
/// - Returns None on failure; callers decide their own fallback strategy.
pub(crate) fn extract_with_dom_smoothie(
    html: &str,
    url: Option<&str>,
) -> Option<dom_smoothie::Article> {
    let cfg = DsConfig {
        // Use default config for now; can be tuned later as needed.
        ..Default::default()
    };
    let mut readability = DsReadability::new(html, url, Some(cfg)).ok()?;
    readability.parse().ok()
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
