//! Spider-based crawler adapter.
//! This crate exposes a constrained surface tailored for rule executor.

use captura_common::{Error, Result};
use spider::website::Website;
use tracing::instrument;

#[derive(Clone, Debug, Default)]
pub struct CrawlOptions {
    pub user_agent: Option<String>,
    pub respect_robots: bool,
    pub smart: bool,
    pub delay_ms: u64,
    pub limit: Option<usize>,
    pub proxy_url: Option<String>,
}

#[instrument]
pub async fn fetch_html(url: &str, opts: &CrawlOptions) -> Result<String> {
    // Use spider for enhanced fetching only (single page for now):
    let mut site = Website::new(url);
    site.with_respect_robots_txt(opts.respect_robots)
        .with_delay(opts.delay_ms)
        .with_limit(opts.limit.unwrap_or(1) as u32);
    if let Some(ref ua) = opts.user_agent {
        site.with_user_agent(Some(ua.as_str()));
    }
    // Note: smart/headless features are configured by feature flags at compile-time.
    site.crawl().await;

    // Grab the root page html from memory store (best-effort):
    if let Some(pages) = site.get_pages() {
        if let Some(page) = pages.first() {
            return Ok(page.get_html());
        }
    }
    Err(Error::Network("empty page".into()))
}
