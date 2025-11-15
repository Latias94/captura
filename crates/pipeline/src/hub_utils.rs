use captura_common::{Error, Result};
use captura_storage::entity::feed;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value as JsonValue;
use url::Url;

use crate::rules_engine::{fetch_html_strategy, FetchCfg};
use crate::sanitize_html;

/// HTTP options wrapper used by Hub handlers.
#[derive(Debug, Clone)]
pub(crate) struct HubHttpOpts {
    pub user_agent: Option<String>,
    pub timeout_ms: Option<u64>,
    pub smart: bool,
    /// Optional extra headers to attach for HTTP fetches.
    pub headers: Option<Vec<(String, String)>>,
}

impl Default for HubHttpOpts {
    fn default() -> Self {
        Self {
            user_agent: Some("captura/0.1".to_string()),
            timeout_ms: Some(15_000),
            smart: false,
            headers: None,
        }
    }
}

/// Fetch HTML using the shared FetchCfg + fetch_html_strategy pipeline.
pub(crate) async fn get_html(
    url: &str,
    opts: &HubHttpOpts,
    feed: Option<&feed::Model>,
) -> Result<String> {
    let ua = opts
        .user_agent
        .clone()
        .unwrap_or_else(|| "captura/0.1".to_string());

    let client = crate::http_client::client_basic(Some(ua.clone()), opts.timeout_ms)?;

    let headers_map = opts.headers.as_ref().map(|pairs| {
        let mut m = serde_json::Map::new();
        for (k, v) in pairs {
            m.insert(k.clone(), JsonValue::String(v.clone()));
        }
        m
    });

    let fetch_cfg = FetchCfg {
        user_agent: Some(ua),
        headers: headers_map,
        smart: Some(opts.smart),
        timeout_ms: opts.timeout_ms,
        respect_robots: Some(true),
        delay_ms: Some(250),
        limit: Some(1),
        proxy_url: None,
    };

    fetch_html_strategy(&client, url, &fetch_cfg, feed).await
}

/// Iterate over all elements matching the selector and apply the provided callback.
/// Useful in handlers for quickly extracting text/attributes to build business structures.
pub(crate) fn for_each_element<F>(html: &str, selector: &str, mut f: F) -> Result<()>
where
    F: FnMut(ElementRef<'_>),
{
    let doc = Html::parse_document(html);
    let sel =
        Selector::parse(selector).map_err(|e| Error::Parse(format!("invalid selector: {e}")))?;
    for el in doc.select(&sel) {
        f(el);
    }
    Ok(())
}

/// Convenience helper: sanitize the HTML of a single element.
pub(crate) fn element_html_sanitized(el: &ElementRef<'_>) -> String {
    sanitize_html(&el.html())
}

/// Compute an absolute URL from a base URL and href.
pub(crate) fn absolutize(base: &str, href: &str) -> String {
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
