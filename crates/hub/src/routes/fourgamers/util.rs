use crate::routes::types::HubItem;
use crate::routes::util;
use captura_common::Result;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use std::collections::HashMap;

pub const BASE_URL: &str = "https://www.4gamers.com.tw";

#[derive(Debug, Clone, Deserialize)]
struct ApiListResponse {
    data: ApiListData,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiListData {
    results: Vec<ApiArticleSummary>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiArticleSummary {
    id: i64,
    title: String,
    intro: String,
    #[serde(rename = "canonicalUrl")]
    canonical_url: String,
    #[serde(rename = "createPublishedAt")]
    create_published_at: i64,
    author: ApiAuthor,
    category: ApiCategory,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiAuthor {
    nickname: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiCategory {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiCategoryInfo {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiCategoryListResponse {
    data: Vec<ApiCategoryInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct FindSectionResponse {
    data: FindSectionData,
}

#[derive(Debug, Clone, Deserialize)]
struct FindSectionData {
    #[serde(rename = "contentSection")]
    content_section: ContentSection,
}

#[derive(Debug, Clone, Deserialize)]
struct ContentSection {
    sections: Vec<Section>,
}

#[derive(Debug, Clone, Deserialize)]
struct Section {
    #[serde(rename = "@type")]
    section_type: String,
    #[serde(default)]
    html: String,
    #[serde(default)]
    items: Vec<ImageItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct ImageItem {
    url: String,
    #[serde(default)]
    alt: String,
    #[serde(flatten)]
    _extra: HashMap<String, serde_json::Value>,
}

fn parse_timestamp_ms(ts: i64) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_ms_timestamp(ts, 8)
}

fn map_article_to_hub_item(a: ApiArticleSummary) -> HubItem {
    let mut categories = Vec::new();
    if !a.category.name.is_empty() {
        categories.push(a.category.name);
    }
    for t in a.tags {
        if !t.is_empty() && !categories.contains(&t) {
            categories.push(t);
        }
    }

    HubItem {
        title: a.title,
        description: if a.intro.is_empty() {
            None
        } else {
            Some(a.intro)
        },
        link: Some(a.canonical_url),
        author: Some(a.author.nickname),
        pub_date: parse_timestamp_ms(a.create_published_at),
        categories,
    }
}

pub async fn fetch_latest(limit: usize) -> Result<Vec<HubItem>> {
    let url = format!(
        "{}/site/api/news/latest?nextStart=0&pageSize={}",
        BASE_URL, limit
    );
    let resp: ApiListResponse = util::get_json(&url).await?;
    Ok(resp
        .data
        .results
        .into_iter()
        .map(map_article_to_hub_item)
        .collect())
}

pub async fn fetch_by_category(category: &str, limit: usize) -> Result<(String, Vec<HubItem>)> {
    let url = format!(
        "{}/site/api/news/by-category/{}?nextStart=0&pageSize={}",
        BASE_URL, category, limit
    );
    let resp: ApiListResponse = util::get_json(&url).await?;
    let items = resp
        .data
        .results
        .into_iter()
        .map(map_article_to_hub_item)
        .collect();

    // Best-effort fetch of category name.
    let cat_name = fetch_category_name(category)
        .await
        .unwrap_or_else(|| category.to_string());

    Ok((cat_name, items))
}

pub async fn fetch_by_tag(tag: &str, limit: usize) -> Result<Vec<HubItem>> {
    let url = format!(
        "{}/site/api/news/by-tag?tag={}&pageSize={}",
        BASE_URL, tag, limit
    );
    let resp: ApiListResponse = util::get_json(&url).await?;
    Ok(resp
        .data
        .results
        .into_iter()
        .map(map_article_to_hub_item)
        .collect())
}

pub async fn fetch_by_topic(topic: &str, limit: usize) -> Result<Vec<HubItem>> {
    let url = format!(
        "{}/site/api/news/option-cfg/{}?pageSize={}",
        BASE_URL, topic, limit
    );
    let resp: ApiListResponse = util::get_json(&url).await?;
    Ok(resp
        .data
        .results
        .into_iter()
        .map(map_article_to_hub_item)
        .collect())
}

async fn fetch_category_name(id_str: &str) -> Option<String> {
    let url = format!("{}/site/api/news/category", BASE_URL);
    let resp: ApiCategoryListResponse = util::get_json(&url).await.ok()?;
    let id = id_str.parse::<i64>().ok()?;
    for c in resp.data {
        if c.id == id {
            return Some(c.name);
        }
    }
    None
}

async fn parse_item(mut item: HubItem) -> Result<HubItem> {
    let link = match item.link.clone() {
        Some(l) => l,
        None => return Ok(item),
    };

    let article_id = match url::Url::parse(&link)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .map(|segments| segments.map(|s| s.to_string()).collect::<Vec<String>>())
        })
        .and_then(|segs| {
            if segs.len() >= 4 && segs[0] == "news" && segs[1] == "detail" {
                segs[2].parse::<i64>().ok()
            } else {
                None
            }
        }) {
        Some(id) => id,
        None => return Ok(item),
    };

    let url = format!("{}/site/api/news/find-section?sub={}", BASE_URL, article_id);
    let resp: FindSectionResponse = util::get_json(&url).await?;
    let sections = resp.data.content_section.sections;

    let mut content_html = String::new();

    for section in sections {
        match section.section_type.as_str() {
            "ContentAdsSection" | "ScrollerAdsSection" | "textScrollerAdsSection" => {}
            "RawHtmlSection" => {
                if !section.html.is_empty() {
                    content_html.push_str(&section.html);
                }
            }
            "ImageGroupSection" => {
                for img in section.items {
                    let alt = if img.alt.is_empty() {
                        &item.title
                    } else {
                        &img.alt
                    };
                    content_html.push_str("<p>");
                    content_html.push_str(&util::html_img(&img.url, alt));
                    content_html.push_str("</p>");
                }
            }
            other => {
                tracing::debug!("fourgamers: unhandled section type {}", other);
            }
        }
    }

    if !content_html.trim().is_empty() {
        let mut desc = String::new();
        if let Some(intro) = item.description.take() {
            if !intro.is_empty() {
                desc.push_str("<p>");
                desc.push_str(&intro);
                desc.push_str("</p>");
            }
        }
        desc.push_str(&content_html);
        item.description = Some(desc);
    }

    Ok(item)
}

pub async fn enrich_items(mut list: Vec<HubItem>) -> Vec<HubItem> {
    let mut out = Vec::new();
    for item in list.drain(..) {
        match parse_item(item).await {
            Ok(i) => out.push(i),
            Err(e) => {
                tracing::debug!("fourgamers: parse_item failed: {}", e);
            }
        }
    }
    out
}
