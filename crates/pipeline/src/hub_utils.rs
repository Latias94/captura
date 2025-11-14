use captura_common::{Error, Result};
use captura_storage::entity::feed;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::{fetch_html_strategy, FetchCfg, sanitize_html, extract_attr, extract_text};

/// Hub 级 HTTP 选项封装，便于在 handler 中复用。
#[derive(Debug, Clone)]
pub(crate) struct HubHttpOpts {
    pub user_agent: Option<String>,
    pub timeout_ms: Option<u64>,
    pub smart: bool,
}

impl Default for HubHttpOpts {
    fn default() -> Self {
        Self {
            user_agent: Some("captura/0.1".to_string()),
            timeout_ms: Some(15_000),
            smart: false,
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

    let client = Client::builder()
        .user_agent(ua.clone())
        .build()
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;

    let fetch_cfg = FetchCfg {
        user_agent: Some(ua),
        headers: None,
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
    let sel = Selector::parse(selector)
        .map_err(|e| Error::Parse(format!("invalid selector: {e}")))?;
    for el in doc.select(&sel) {
        f(el);
    }
    Ok(())
}

/// 便捷函数：从 html 中按 selector 提取首个元素的文本内容。
pub(crate) fn first_text(html: &str, selector: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(selector).ok()?;
    doc.select(&sel)
        .next()
        .and_then(|el| extract_text(&el, selector))
}

/// 便捷函数：从 html 中按 selector@attr 语法提取首个属性值。
pub(crate) fn first_attr_expr(html: &str, expr: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let (sel, attr) = expr.split_once('@')?;
    let sel_parsed = Selector::parse(sel).ok()?;
    doc.select(&sel_parsed)
        .next()
        .and_then(|el| extract_attr(&el, &format!("{}@{}", sel, attr)))
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

