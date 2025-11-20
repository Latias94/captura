use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;

const ROOT_URL: &str = "https://www.36kr.com";

pub const META_36KR: RouteMeta = RouteMeta {
    hub_id: "36kr",
    path: "/36kr/:category/:subCategory?/:keyword?",
    categories: &["new-media"],
    example: "/36kr/newsflashes",
    params: &[
        ParamMeta {
            name: "category",
            description: "Category, e.g. news, newsflashes, recommend, search/articles, ...",
            default: Some("news"),
            options: &[
                ("news", "Latest news (mapped to information/web_news)"),
                ("newsflashes", "News flashes (快讯)"),
                ("recommend", "Recommended news"),
                ("life", "Life"),
                ("estate", "Real estate"),
                ("workplace", "Workplace"),
                ("search", "Search (used with subCategory 'articles' or 'newsflashes')"),
            ],
        },
        ParamMeta {
            name: "subCategory",
            description:
                "Optional sub-category, e.g. 'articles' when category=search, kept for compatibility",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "keyword",
            description: "Optional keyword when doing search/articles or search/newsflashes",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.36kr.com", "36kr.com"],
        target: "/:category",
    }],
    name: "36kr News / Flashes",
    maintainers: &["captura"],
    url: "https://www.36kr.com/",
    description:
        "36kr news, flashes and search results (simplified adaptation of RSSHub 36kr route).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("news");
    let sub = ctx.param_str("subCategory").unwrap_or("");
    let keyword = ctx.param_str("keyword").unwrap_or("");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let path = build_path(category, sub, keyword);
    let current_path = apply_shortcuts(&path);
    let current_url = format!("{}{}", ROOT_URL, current_path);

    let html = util::get_html(&current_url).await?;
    let raw_items = extract_item_list(&html)?;

    let mut items = Vec::new();
    for raw in raw_items.into_iter() {
        if items.len() >= limit {
            break;
        }
        if raw.itemType == 0 {
            continue;
        }

        let eff = raw
            .templateMaterial
            .as_ref()
            .map(|m| InnerItem {
                itemId: m.itemId,
                widgetTitle: m.widgetTitle.clone(),
                widgetContent: m.widgetContent.clone(),
                publishTime: m.publishTime,
            })
            .unwrap_or_else(|| InnerItem {
                itemId: raw.itemId,
                widgetTitle: raw.widgetTitle.clone(),
                widgetContent: raw.widgetContent.clone(),
                publishTime: raw.publishTime,
            });

        let item_id = eff.itemId;
        let title = eff.widgetTitle.as_deref().unwrap_or("").trim().to_string();
        if title.is_empty() {
            continue;
        }
        let desc = eff
            .widgetContent
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();

        let is_flash = path.contains("newsflashes");
        let link = if is_flash {
            format!("{}/newsflashes/{}", ROOT_URL, item_id)
        } else {
            format!("{}/p/{}", ROOT_URL, item_id)
        };

        let mut description = Some(desc);

        // For article-like paths, try to enhance with full content from
        // articleDetailContent, but ignore failures and keep list content.
        if !path.starts_with("/search") && !is_flash {
            if let Ok(detail_html) = util::get_html(&link).await {
                if let Some(full) = extract_article_detail(&detail_html) {
                    description = Some(full);
                }
            }
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("36kr - {}", path.trim_start_matches('/')),
        description: Some(format!("36kr path={}", path)),
        link: Some(current_url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn build_path(category: &str, sub: &str, keyword: &str) -> String {
    let mut segs = Vec::new();
    if !category.is_empty() {
        segs.push(category.to_string());
    }
    if !sub.is_empty() {
        segs.push(sub.to_string());
    }
    if !keyword.is_empty() {
        segs.push(keyword.to_string());
    }
    let mut path = String::from("/");
    path.push_str(&segs.join("/"));

    // Mirror RSSHub's normalization:
    // - /news -> /information (but keep /newsflashes)
    if path.starts_with("/news") && !path.starts_with("/newsflashes") {
        path = path.replacen("/news", "/information", 1);
    }
    // - /search/article -> /search/articles
    if path.starts_with("/search/article") {
        path = path.replacen("/search/article", "/search/articles", 1);
    }
    path
}

fn apply_shortcuts(path: &str) -> String {
    match path {
        "/information" => "/information/web_news".to_string(),
        "/information/latest" => "/information/web_news".to_string(),
        "/information/recommend" => "/information/web_recommend".to_string(),
        "/information/life" => "/information/happy_life".to_string(),
        "/information/estate" => "/information/real_estate".to_string(),
        "/information/workplace" => "/information/web_zhichang".to_string(),
        _ => path.to_string(),
    }
}

fn default_item_type() -> i32 {
    1
}

#[derive(Debug, Deserialize)]
struct RawItem {
    itemId: i64,
    #[serde(default = "default_item_type")]
    itemType: i32,
    #[serde(default)]
    templateMaterial: Option<InnerItem>,
    #[serde(default)]
    widgetTitle: Option<String>,
    #[serde(default, alias = "content")]
    widgetContent: Option<String>,
    #[serde(default)]
    publishTime: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct InnerItem {
    itemId: i64,
    #[serde(default)]
    widgetTitle: Option<String>,
    #[serde(default)]
    widgetContent: Option<String>,
    #[serde(default)]
    publishTime: Option<i64>,
}

static ITEMLIST_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""itemList":(\[.*?\])"#).expect("invalid itemList regex"));

fn extract_item_list(html: &str) -> Result<Vec<RawItem>> {
    let caps = ITEMLIST_RE
        .captures(html)
        .ok_or_else(|| Error::Parse("36kr: itemList not found".into()))?;
    let json = caps
        .get(1)
        .ok_or_else(|| Error::Parse("36kr: itemList capture missing".into()))?
        .as_str();
    serde_json::from_str::<Vec<RawItem>>(json)
        .map_err(|e| Error::Parse(format!("36kr: failed to parse itemList JSON: {}", e)))
}

fn extract_article_detail(html: &str) -> Option<String> {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("div.articleDetailContent").ok()?;
    let el = doc.select(&sel).next()?;
    let body = util::element_html(&el);
    if body.trim().is_empty() {
        None
    } else {
        Some(body)
    }
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_36KR: Route = Route {
    meta: &META_36KR,
    handler: handler_fn,
};
