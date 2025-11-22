use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use serde_json::Value;

const BASE_URL: &str = "https://www.apiseven.com";
const BLOG_URL: &str = "https://www.apiseven.com/blog";

#[derive(Debug, Deserialize)]
struct ApisevenListItem {
    title: String,
    slug: String,
    published_at: String,
    #[serde(default)]
    tags: Vec<String>,
}

pub const META_APISEVEN_BLOG: RouteMeta = RouteMeta {
    hub_id: "apiseven/blog",
    path: "/apiseven/blog",
    categories: &["blog"],
    example: "/apiseven/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.apiseven.com/blog"],
        target: "/blog",
    }],
    name: "API7 Blog",
    maintainers: &["captura"],
    url: BLOG_URL,
    description: "API7 / Apache APISIX blog posts, based on the public Next.js data API.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn extract_list(json: &Value, limit: usize) -> Result<Vec<ApisevenListItem>, Error> {
    let props = json
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .ok_or_else(|| Error::Parse("apiseven: missing props.pageProps".to_string()))?;
    let list = props
        .get("list")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("apiseven: missing pageProps.list".to_string()))?;

    let mut out = Vec::new();
    for item in list.iter().take(limit) {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let slug = item
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let published_at = item
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() || slug.is_empty() {
            continue;
        }
        let tags = item
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.push(ApisevenListItem {
            title,
            slug,
            published_at,
            tags,
        });
    }
    Ok(out)
}

async fn fetch_detail(url: &str) -> Result<(String, String, Option<String>), Error> {
    let html = util::get_html(url).await?;
    let json = util::extract_next_data(&html)?;
    let props = json
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .ok_or_else(|| Error::Parse("apiseven: missing props.pageProps in detail".to_string()))?;
    let post = props
        .get("post")
        .ok_or_else(|| Error::Parse("apiseven: missing pageProps.post".to_string()))?;

    let title = post
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let content = post
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let author = post
        .get("author_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());

    // Lightweight Markdown-ish to HTML: replace newlines with `<br>`.
    let content_html = if content.is_empty() {
        String::new()
    } else {
        content.replace('\n', "<br>\n")
    };

    Ok((title, content_html, author))
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    // Limit to a reasonable number of posts to avoid excessive detail fetches.
    let limit: usize = 20;

    let html = util::get_html(BLOG_URL).await?;
    let json = util::extract_next_data(&html)?;
    let list = extract_list(&json, limit)?;

    let mut items = Vec::new();
    for item in list {
        let link = format!("{BASE_URL}{}", item.slug);
        let pub_date = parse_pub_date(&item.published_at);

        let (detail_title, detail_html, author) = match fetch_detail(&link).await {
            Ok(t) => t,
            Err(_) => {
                // Fallback to list metadata when detail JSON is unavailable.
                (item.title.clone(), String::new(), None)
            }
        };

        let title = if detail_title.is_empty() {
            item.title.clone()
        } else {
            detail_title
        };

        let description = if detail_html.is_empty() {
            None
        } else {
            Some(detail_html)
        };

        // Use tags as categories when available.
        let mut categories = Vec::new();
        if item.tags.is_empty() {
            categories.push("apiseven".to_string());
        } else {
            categories.extend(item.tags.clone());
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link.clone()),
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: "API7 Blog".to_string(),
        description: Some("API7 / Apache APISIX blog posts".to_string()),
        link: Some(BLOG_URL.to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_APISEVEN_BLOG: Route = Route {
    meta: &META_APISEVEN_BLOG,
    handler: handler_fn,
};
