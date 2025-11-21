//! Generic crawler adapter for advanced HTML fetching.
//!
//! The public API in this crate is intentionally kept small and engine-agnostic:
//! callers talk in terms of `CrawlOptions` and HTML strings, without depending
//! on any concrete crawler implementation. Today we use `spider` under the hood
//! to benefit from its anti-bot and (optionally) headless browser support, but
//! that choice is an internal detail and can be swapped out in the future
//! without touching Hub routes or rule executors.

use captura_common::{Error, Result};
use spider::website::Website;
use tracing::instrument;

/// Generic crawl options used by higher-level components (Hub routes, rule
/// engine) to steer advanced fetching behaviour.
///
/// The semantics are deliberately engine-neutral:
/// - `user_agent`: override UA for this crawl only;
/// - `respect_robots`: whether to respect robots.txt;
/// - `smart`: enable stronger anti-bot / JS handling when supported by the
///   underlying engine (for spider this maps to `crawl_smart`);
/// - `delay_ms`: politeness delay between requests;
/// - `limit`: maximum number of pages the crawl should explore;
/// - `proxy_url`: optional proxy endpoint.
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
    if let Some(ref proxy) = opts.proxy_url {
        if !proxy.is_empty() {
            site.with_proxies(Some(vec![proxy.clone()]));
        }
    }
    // Note: smart/headless features are configured by feature flags at compile-time.
    if opts.smart {
        site.crawl_smart().await;
    } else {
        site.crawl().await;
    }

    // Grab the root page html from memory store (best-effort):
    if let Some(pages) = site.get_pages() {
        if let Some(page) = pages.first() {
            return Ok(page.get_html());
        }
    }
    Err(Error::Network("empty page".into()))
}
