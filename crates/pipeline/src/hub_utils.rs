use captura_common::{Error, Result};
use captura_storage::entity::feed;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value as JsonValue;
use url::Url;

use crate::rules_engine::{fetch_html_strategy, FetchCfg};
use crate::sanitize_html;

/// Hub 级 HTTP 选项封装，便于在 handler 中复用。
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

/// 使用统一的 FetchCfg + fetch_html_strategy 抓取 HTML。
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

/// 遍历匹配 selector 的所有元素，使用提供的回调处理。
/// 适合在 handler 中快速提取文本/属性并构造业务结构。
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

/// 便捷函数：对元素 HTML 做 sanitize。
pub(crate) fn element_html_sanitized(el: &ElementRef<'_>) -> String {
    sanitize_html(&el.html())
}

/// 基于 base URL 和 href 计算绝对地址。
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
