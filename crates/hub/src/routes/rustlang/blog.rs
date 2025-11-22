use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate};
use regex::Regex;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://blog.rust-lang.org";

pub const META_RUSTLANG_BLOG: RouteMeta = RouteMeta {
    hub_id: "rustlang/blog",
    path: "/rustlang/blog",
    categories: &["programming"],
    example: "/rustlang/blog",
    params: &[ParamMeta {
        name: "limit",
        description: "最大文章数量，默认 30",
        default: Some("30"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["blog.rust-lang.org"],
        target: "/blog",
    }],
    name: "Rust 官方博客",
    maintainers: &["captura"],
    url: "https://blog.rust-lang.org/",
    description: "The Rust Programming Language 官方博客文章列表。",
    default_view: Some("articles"),
};

fn parse_date_from_link(link: &str) -> Option<DateTime<FixedOffset>> {
    // 典型链接形如 https://blog.rust-lang.org/2025/11/20/slug/
    let re = Regex::new(r"/(\d{4})/(\d{2})/(\d{2})/").ok()?;
    let caps = re.captures(link)?;
    let year = caps.get(1)?.as_str().parse::<i32>().ok()?;
    let month = caps.get(2)?.as_str().parse::<u32>().ok()?;
    let day = caps.get(3)?.as_str().parse::<u32>().ok()?;
    let naive = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(DateTime::<FixedOffset>::from_naive_utc_and_offset(
        naive, offset,
    ))
}

async fn fetch_index() -> Result<String> {
    util::get_html(ROOT_URL).await
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_row = Selector::parse("section#posts table.post-list tr")
        .map_err(|e| Error::Parse(format!("rustlang: invalid row selector: {e}")))?;
    let sel_link = Selector::parse("td a")
        .map_err(|e| Error::Parse(format!("rustlang: invalid link selector: {e}")))?;

    let mut items = Vec::new();
    for row in doc.select(&sel_row) {
        if items.len() >= limit {
            break;
        }

        let Some(a) = row.select(&sel_link).next() else {
            continue;
        };
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        // 跳过“See also”等非正文链接
        if !link.contains("/20") {
            continue;
        }

        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let pub_date = parse_date_from_link(&link);

        items.push(HubItem {
            title,
            description: None,
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["rust".to_string()],
        });
    }

    Ok(items)
}

async fn enrich_item(mut item: HubItem) -> Result<HubItem> {
    let Some(link) = &item.link else {
        return Ok(item);
    };

    let html = util::get_html(link).await?;
    let doc = Html::parse_document(&html);
    let sel_article = Selector::parse("section.white div.post")
        .map_err(|e| Error::Parse(format!("rustlang: invalid post selector: {e}")))?;

    if let Some(post) = doc.select(&sel_article).next() {
        let body = util::element_html(&post);
        if !body.trim().is_empty() {
            item.description = Some(body);
        }
    }

    Ok(item)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let html = fetch_index().await?;
    let items = extract_items(&html, limit)?;

    let mut enriched = Vec::new();
    for item in items {
        match enrich_item(item).await {
            Ok(it) => enriched.push(it),
            Err(_) => {}
        }
    }

    Ok(HubData {
        title: "The Rust Programming Language Blog".to_string(),
        description: Some("Rust 官方博客文章列表。".to_string()),
        link: Some(ROOT_URL.to_string()),
        image: Some("https://www.rust-lang.org/static/images/rust-social-wide.jpg".to_string()),
        language: Some("en".to_string()),
        items: enriched,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_RUSTLANG_BLOG: Route = Route {
    meta: &META_RUSTLANG_BLOG,
    handler: handler_fn,
};
