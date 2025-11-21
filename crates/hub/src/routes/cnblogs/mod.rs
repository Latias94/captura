//! cnblogs.com related routes.
//!
//! Currently implemented:
//! - /cnblogs/aggsite/:kind  AggSite rankings (top diggs / views / headline)
//! - /cnblogs/cate/:type     Category listing
//! - /cnblogs/pick           Editor picks

pub mod aggsite;
pub mod cate;
pub mod pick;

use crate::routes::types::{HubData, HubItem};
use crate::routes::util;
use captura_common::{Error, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.cnblogs.com";

fn parse_cnblogs_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    let raw = s.trim();
    if raw.is_empty() {
        return None;
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        let offset = FixedOffset::east_opt(8 * 3600)?;
        return offset.from_local_datetime(&naive).single();
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M") {
        let offset = FixedOffset::east_opt(8 * 3600)?;
        return offset.from_local_datetime(&naive).single();
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        if let Some(naive) = date.and_hms_opt(0, 0, 0) {
            let offset = FixedOffset::east_opt(8 * 3600)?;
            return offset.from_local_datetime(&naive).single();
        }
    }
    None
}

/// Fetch a cnblogs list page and convert it into HubData.
///
/// `sub_path` is the path relative to ROOT_URL, such as:
/// - `/aggsite/topdiggs`
/// - `/aggsite/topviews`
/// - `/aggsite/headline`
/// - `/cate/go`
/// - `/pick`
pub async fn fetch_cnblogs_list(sub_path: &str, limit: usize) -> Result<HubData> {
    let url = format!("{ROOT_URL}{sub_path}");
    let html = util::get_html(&url).await?;
    let doc = Html::parse_document(&html);

    let sel_title = Selector::parse("title")
        .map_err(|e| Error::Parse(format!("cnblogs: title selector error: {e}")))?;
    let sel_desc = Selector::parse(r#"meta[name="description"]"#)
        .map_err(|e| Error::Parse(format!("cnblogs: desc selector error: {e}")))?;
    let sel_article = Selector::parse("#post_list article")
        .map_err(|e| Error::Parse(format!("cnblogs: article selector error: {e}")))?;
    let sel_item_title = Selector::parse(".post-item-title")
        .map_err(|e| Error::Parse(format!("cnblogs: item title selector error: {e}")))?;
    let sel_item_summary = Selector::parse(".post-item-summary")
        .map_err(|e| Error::Parse(format!("cnblogs: item summary selector error: {e}")))?;
    let sel_item_foot_span = Selector::parse(".post-item-foot .post-meta-item span")
        .map_err(|e| Error::Parse(format!("cnblogs: item foot selector error: {e}")))?;
    let sel_editor_meta = Selector::parse(".editorpick-item-meta")
        .map_err(|e| Error::Parse(format!("cnblogs: editor meta selector error: {e}")))?;
    let sel_author = Selector::parse(".post-item-author span")
        .map_err(|e| Error::Parse(format!("cnblogs: author selector error: {e}")))?;

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "博客园".to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    let mut items = Vec::new();

    for article in doc.select(&sel_article).take(limit) {
        let title_el = article.select(&sel_item_title).next();
        let mut title_text = title_el
            .as_ref()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title_text.is_empty() {
            continue;
        }

        let link = title_el
            .and_then(|el| el.value().attr("href"))
            .map(|href| util::absolutize(ROOT_URL, href));

        let summary_text = article
            .select(&sel_item_summary)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let description_item = if summary_text.is_empty() {
            None
        } else {
            Some(summary_text)
        };

        let mut time_text = article
            .select(&sel_item_foot_span)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if time_text.is_empty() {
            time_text = article
                .select(&sel_editor_meta)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
        }
        let pub_date = if time_text.is_empty() {
            None
        } else {
            parse_cnblogs_datetime(&time_text)
        };

        let author = article
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        items.push(HubItem {
            title: title_text,
            description: description_item,
            link,
            author,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title,
        description,
        link: Some(url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}
