use crate::routes::types::HubItem;
use crate::routes::util;
use captura_common::Result;
use scraper::{Html, Selector};

pub const BASE_URL: &str = "https://indienova.com";

pub fn parse_list(doc: &Html) -> Vec<HubItem> {
    let sel = Selector::parse(".article-list article, .article-list .article").unwrap();
    let title_sel = Selector::parse("h2 a, h3 a").unwrap();
    let meta_sel = Selector::parse(".meta, .article-meta").unwrap();

    let mut out = Vec::new();
    for el in doc.select(&sel) {
        let link_el = match el.select(&title_sel).next() {
            Some(a) => a,
            None => continue,
        };
        let href = match link_el.value().attr("href") {
            Some(h) => h,
            None => continue,
        };
        let link = util::absolutize(BASE_URL, href);
        let title = util::element_text(&link_el);
        if title.is_empty() {
            continue;
        }
        let description = None;
        let pub_date = el.select(&meta_sel).next().and_then(|m| {
            let s = util::element_text(&m);
            util::parse_date(&s)
        });

        out.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["indienova".to_string()],
        });
    }

    out
}

async fn parse_item(mut item: HubItem) -> Result<HubItem> {
    let Some(ref link) = item.link else {
        return Ok(item);
    };
    let html = util::get_html(link).await?;
    let doc = Html::parse_document(&html);
    let sel = Selector::parse(".main article, .post, .article").unwrap();
    if let Some(content) = doc.select(&sel).next() {
        let html = content.html();
        if !html.trim().is_empty() {
            item.description = Some(html);
        }
    }
    Ok(item)
}

pub async fn enrich_items(mut list: Vec<HubItem>) -> Vec<HubItem> {
    let mut out = Vec::new();
    for item in list.drain(..) {
        match parse_item(item).await {
            Ok(i) => out.push(i),
            Err(e) => {
                tracing::debug!("indienova: parse_item failed: {}", e);
            }
        }
    }
    out
}
