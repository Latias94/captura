use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate};
use scraper::{Html, Selector};
use serde::Deserialize;

const ROOT_URL: &str = "https://www.swift.org";

#[derive(Debug, Deserialize)]
struct SwiftPost {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    #[allow(dead_code)]
    #[serde(rename = "image-url")]
    image_url: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    #[serde(rename = "image-alt")]
    image_alt: Option<String>,
}

pub const META_SWIFT_BLOG: RouteMeta = RouteMeta {
    hub_id: "swift/blog",
    path: "/swift/blog",
    categories: &["programming"],
    example: "/swift/blog",
    params: &[ParamMeta {
        name: "limit",
        description: "最大文章数量，默认 30",
        default: Some("30"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["swift.org/blog", "www.swift.org/blog"],
        target: "/blog",
    }],
    name: "Swift 官方博客",
    maintainers: &["captura"],
    url: "https://www.swift.org/blog/",
    description: "Swift.org 官方博客文章列表。",
    default_view: Some("articles"),
};

fn parse_swift_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // 形如 "November 17, 2025"
    let naive = NaiveDate::parse_from_str(s, "%B %d, %Y")
        .or_else(|_| NaiveDate::parse_from_str(s, "%B %e, %Y"))
        .ok()?;
    let naive_dt = naive.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(DateTime::<FixedOffset>::from_naive_utc_and_offset(
        naive_dt, offset,
    ))
}

async fn fetch_index() -> captura_common::Result<String> {
    util::get_html(&format!("{}/blog/", ROOT_URL)).await
}

fn extract_post_data(html: &str) -> Result<Vec<SwiftPost>, Error> {
    let doc = Html::parse_document(html);
    let sel_script = Selector::parse(r#"script#post-data"#)
        .map_err(|e| Error::Parse(format!("swift/blog: selector error: {e}")))?;
    let script = doc
        .select(&sel_script)
        .next()
        .ok_or_else(|| Error::Parse("swift/blog: post-data script not found".to_string()))?;
    let json_str = script.text().collect::<String>();
    let posts: Vec<SwiftPost> = serde_json::from_str(&json_str)
        .map_err(|e| Error::Parse(format!("swift/blog: invalid post-data JSON: {e}")))?;
    Ok(posts)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let html = fetch_index().await?;
    let posts = extract_post_data(&html)?;

    let mut items = Vec::new();
    for post in posts.into_iter().take(limit) {
        if post.title.trim().is_empty() || post.url.trim().is_empty() {
            continue;
        }

        let link = util::absolutize(ROOT_URL, &post.url);
        let pub_date = parse_swift_date(&post.date);

        let mut categories = Vec::new();
        categories.push("swift".to_string());
        for c in &post.categories {
            if !c.trim().is_empty() {
                categories.push(c.trim().to_string());
            }
        }

        let mut desc = String::new();
        if !post.excerpt.trim().is_empty() {
            desc.push_str(&html_escape::encode_safe(&post.excerpt));
        }

        items.push(HubItem {
            title: post.title.trim().to_string(),
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: "Swift.org Blog".to_string(),
        description: Some("Swift.org 官方博客文章列表。".to_string()),
        link: Some(format!("{}/blog/", ROOT_URL)),
        image: Some(format!("{}/assets/images/icon-swift.svg", ROOT_URL)),
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SWIFT_BLOG: Route = Route {
    meta: &META_SWIFT_BLOG,
    handler: handler_fn,
};
