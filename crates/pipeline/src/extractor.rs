//! HTML fetching and content extraction helpers.
//! This module centralizes entry-level fetching + extraction so
//! both the API compatibility layers and the internal pipeline can reuse the same logic.

use captura_common::Result;
use captura_extract::{
    EntryExtractConfig, ExtractResult, extract_from_html as core_extract_from_html,
};
use captura_storage::entity::feed;

/// DTO-style configuration for entry-level full-content extraction.
/// This is decoupled from database models so it can be reused by
/// clients (TUI/CLI) and other tools.
#[derive(Debug, Clone, Default)]
pub struct EntryExtractConfigDto {
    pub page_url: String,
    pub scraper_rules: Option<String>,
    pub cookies: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub user_agent: Option<String>,
    pub request_timeout_ms: Option<u64>,
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
        .map_err(|e| captura_common::Error::Network(e.to_string()))?
        .text()
        .await
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;

    core_extract_from_html(&html, Some(page_url), f.scraper_rules.as_deref())
}

/// Fetch and extract entry content using a DTO configuration
/// rather than a database feed model. This is intended for
/// use by clients and tools that do not depend on SeaORM.
pub async fn fetch_and_extract_entry_dto(cfg: &EntryExtractConfigDto) -> Result<ExtractResult> {
    let core_cfg = EntryExtractConfig {
        page_url: cfg.page_url.clone(),
        scraper_rules: cfg.scraper_rules.clone(),
        cookies: cfg.cookies.clone(),
        username: cfg.username.clone(),
        password: cfg.password.clone(),
        user_agent: cfg.user_agent.clone(),
        request_timeout_ms: cfg.request_timeout_ms,
    };
    captura_extract::fetch_and_extract_entry_dto(&core_cfg).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dto_config_default() {
        let cfg = EntryExtractConfigDto::default();
        assert!(cfg.page_url.is_empty());
    }
}
