use captura_common::{Error, Result};
use scraper::{ElementRef, Html, Selector};
use url::Url;

/// Fetch HTML with a basic reqwest client and default UA.
pub async fn get_html(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("captura/0.1")
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    Ok(text)
}

/// Iterate over all elements matching the selector and apply the callback.
pub fn for_each_element<F>(html: &str, selector: &str, mut f: F) -> Result<()>
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

/// Sanitize is kept minimal here; hub handlers typically return small HTML fragments.
pub fn element_html(el: &ElementRef<'_>) -> String {
    el.html()
}

/// Compute an absolute URL from a base URL and href.
pub fn absolutize(base: &str, href: &str) -> String {
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

/// Extract attribute using "selector@attr" syntax.
pub fn extract_attr(parent: &ElementRef<'_>, expr: &str) -> Option<String> {
    if let Some((sel, attr)) = expr.split_once('@') {
        if let Ok(s) = Selector::parse(sel) {
            if let Some(el) = parent.select(&s).next() {
                return el.value().attr(attr).map(|v| v.to_string());
            }
        }
    }
    None
}

/// Extract text content for the first element matching the selector.
pub fn extract_text(parent: &ElementRef<'_>, sel: &str) -> Option<String> {
    if let Ok(s) = Selector::parse(sel) {
        if let Some(el) = parent.select(&s).next() {
            return Some(el.text().collect::<Vec<_>>().join("").trim().to_string());
        }
    }
    None
}
