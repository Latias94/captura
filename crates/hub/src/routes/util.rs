use captura_common::{Error, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use scraper::{ElementRef, Html, Selector};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

/// Fetch HTML with a basic reqwest client and default UA.
pub async fn get_html(url: &str) -> Result<String> {
    let client = captura_net::client_basic(None, None)?;
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

/// Fetch HTML using an advanced crawler backend when available (e.g. spider),
/// falling back to plain HTTP when crawling fails or returns empty content.
///
/// This helper is intended for sites with heavier anti-bot or JavaScript
/// challenges. The concrete crawler implementation is kept behind the
/// `captura-crawler` crate so that the Hub does not depend on any specific
/// engine such as spider directly.
pub async fn get_html_smart(url: &str) -> Result<String> {
    // First try the crawler backend in "smart" mode.
    let mut opts = captura_crawler::CrawlOptions::default();
    opts.smart = true;
    // For Hub routes we default to respecting robots.txt; callers that need
    // different behaviour can still use `get_html` directly.
    opts.respect_robots = true;

    if let Ok(html) = captura_crawler::fetch_html(url, &opts).await {
        if !html.trim().is_empty() {
            return Ok(html);
        }
    }

    // Fallback to the basic HTTP client if crawler path fails.
    get_html(url).await
}

/// Fetch JSON from the given URL with a basic client and default UA.
pub async fn get_json<T>(url: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let client = captura_net::client_basic(None, None)?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    resp.json::<T>()
        .await
        .map_err(|e| Error::Parse(e.to_string()))
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

/// Extract plain text from an element (self text), trimmed.
pub fn element_text(el: &ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join("").trim().to_string()
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

pub use captura_net::html::{extract_attr, extract_text};

/// Best-effort parse of a date string into `DateTime<FixedOffset>`.
///
/// Supports common formats such as RFC3339, RFC2822, `YYYY-MM-DD HH:MM:SS`,
/// and `YYYY-MM-DD` (treated as midnight UTC).
pub fn parse_date(input: &str) -> Option<DateTime<FixedOffset>> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt);
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Some(dt);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        if let Some(offset) = FixedOffset::east_opt(0) {
            return Some(offset.from_utc_datetime(&naive));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(naive) = date.and_hms_opt(0, 0, 0) {
            if let Some(offset) = FixedOffset::east_opt(0) {
                return Some(offset.from_utc_datetime(&naive));
            }
        }
    }
    None
}

/// Parse a millisecond timestamp (`i64`) into `DateTime<FixedOffset>` with the given hour offset.
///
/// 常见于各类国内新闻站（毫秒级时间戳），offset_hours 例如：0 / 8 / 9。
pub fn parse_ms_timestamp(ts: i64, offset_hours: i32) -> Option<DateTime<FixedOffset>> {
    if ts <= 0 {
        return None;
    }
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000).max(0) as u32;
    let naive = NaiveDateTime::from_timestamp_opt(secs, nsecs)?;
    let offset = FixedOffset::east_opt(offset_hours * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

/// 从 HTML 中抽取 Next.js 的 `__NEXT_DATA__` JSON。
///
/// 许多站点（Pixiv、小宇宙、澎湃等）都使用该模式。
pub fn extract_next_data(html: &str) -> Result<Value> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"script#__NEXT_DATA__"#)
        .map_err(|e| Error::Parse(format!("next_data: selector error: {e}")))?;
    let script = doc
        .select(&sel)
        .next()
        .ok_or_else(|| Error::Parse("next_data: __NEXT_DATA__ not found".to_string()))?;
    let json_str = script.text().collect::<String>();
    serde_json::from_str(&json_str)
        .map_err(|e| Error::Parse(format!("next_data: invalid JSON: {e}")))
}

/// 解析仅包含日期的字符串（YYYY-MM-DD）为 `NaiveDate`。
pub fn parse_ymd_date(s: &str) -> Option<NaiveDate> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()
}

/// 解析日本日期字符串（例如 `2025年11月20日`）为 `NaiveDate`。
pub fn parse_jp_date_only(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let re = regex::Regex::new(r"(?P<y>\d{4})年(?P<m>\d{1,2})月(?P<d>\d{1,2})日").ok()?;
    let caps = re.captures(s)?;
    let y = caps.name("y")?.as_str().parse::<i32>().ok()?;
    let m = caps.name("m")?.as_str().parse::<u32>().ok()?;
    let d = caps.name("d")?.as_str().parse::<u32>().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

/// 解析日本日期时间字符串（例如 `2025年11月20日 00:00`）为 JST(+9) 时间。
pub fn parse_jp_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let re = regex::Regex::new(
        r"(?P<y>\d{4})年(?P<m>\d{1,2})月(?P<d>\d{1,2})日\s+(?P<h>\d{1,2}):(?P<min>\d{1,2})",
    )
    .ok()?;
    let caps = re.captures(s)?;
    let y = caps.name("y")?.as_str().parse::<i32>().ok()?;
    let m = caps.name("m")?.as_str().parse::<u32>().ok()?;
    let d = caps.name("d")?.as_str().parse::<u32>().ok()?;
    let h = caps.name("h")?.as_str().parse::<u32>().ok()?;
    let min = caps.name("min")?.as_str().parse::<u32>().ok()?;

    let naive = NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(h, min, 0)?;
    let offset = FixedOffset::east_opt(9 * 3600)?;
    offset.from_local_datetime(&naive).single()
}

/// Parse a Chinese datetime string like `2025年11月20日 00:00` as CST(+8).
pub fn parse_cn_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let re = regex::Regex::new(
        r"(?P<y>\d{4})年(?P<m>\d{1,2})月(?P<d>\d{1,2})日\s+(?P<h>\d{1,2}):(?P<min>\d{1,2})",
    )
    .ok()?;
    let caps = re.captures(s)?;
    let y = caps.name("y")?.as_str().parse::<i32>().ok()?;
    let m = caps.name("m")?.as_str().parse::<u32>().ok()?;
    let d = caps.name("d")?.as_str().parse::<u32>().ok()?;
    let h = caps.name("h")?.as_str().parse::<u32>().ok()?;
    let min = caps.name("min")?.as_str().parse::<u32>().ok()?;

    let naive = NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(h, min, 0)?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    offset.from_local_datetime(&naive).single()
}

/// 解析秒级 Unix 时间戳为 `DateTime<FixedOffset>`。
pub fn parse_unix_timestamp(ts: i64, offset_hours: i32) -> Option<DateTime<FixedOffset>> {
    if ts <= 0 {
        return None;
    }
    let naive = NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(offset_hours * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

/// 生成一个简单的 `<img>` 标签片段。
pub fn html_img(src: &str, alt: &str) -> String {
    format!(r#"<img src="{src}" alt="{alt}">"#, src = src, alt = alt)
}

/// 生成一个带 `controls` 的 `<audio>` 播放器片段。
pub fn html_audio(src: &str) -> String {
    format!(
        r#"<audio controls src="{src}">Your browser does not support the audio element.</audio>"#,
        src = src
    )
}
