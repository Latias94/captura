use crate::routes::types::HubItem;
use crate::routes::util;
use captura_common::Result;
use scraper::{Html, Selector};

pub const BASE_URL: &str = "https://www.yystv.cn";

async fn parse_item(mut item: HubItem) -> Result<HubItem> {
    let Some(ref link) = item.link else {
        return Ok(item);
    };

    let html = util::get_html(link).await?;
    let doc = Html::parse_document(&html);

    // Prefer content under `#main section.article-section .doc-content > div`,
    // fallback to `.doc-content` if structure changes.
    let sel_doc = Selector::parse("#main section.article-section .doc-content").unwrap();
    let sel_div = Selector::parse("div").unwrap();

    let mut description = String::new();
    if let Some(container) = doc.select(&sel_doc).next() {
        if let Some(first_div) = container.select(&sel_div).next() {
            description = first_div.html();
        } else {
            description = container.html();
        }
    } else {
        let sel_fallback = Selector::parse(".doc-content").unwrap();
        if let Some(el) = doc.select(&sel_fallback).next() {
            description = el.html();
        }
    }

    if !description.trim().is_empty() {
        item.description = Some(description);
    }

    Ok(item)
}

pub async fn enrich_items(mut list: Vec<HubItem>) -> Vec<HubItem> {
    let mut out = Vec::new();
    for item in list.drain(..) {
        match parse_item(item).await {
            Ok(i) => out.push(i),
            Err(e) => {
                tracing::debug!("yystv: enrich item failed: {}", e);
            }
        }
    }
    out
}
