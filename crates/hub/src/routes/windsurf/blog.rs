use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://windsurf.com";

pub const META_WINDSURF_BLOG: RouteMeta = RouteMeta {
    hub_id: "windsurf/blog",
    path: "/windsurf/blog",
    categories: &["programming"],
    example: "/windsurf/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["windsurf.com/blog"],
        target: "/blog",
    }],
    name: "Windsurf Blog",
    maintainers: &["captura"],
    url: "https://windsurf.com/blog",
    description: "Official Windsurf blog, aligned with RSSHub /windsurf/blog.",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

async fn fetch_blog_meta() -> Result<(
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    let url = format!("{}/blog", BASE_URL);
    let html = util::get_html(&url).await?;
    let doc = Html::parse_document(&html);
    let sel_title = Selector::parse("title").unwrap();
    let sel_meta_desc = Selector::parse("meta[property=\"og:description\"]").unwrap();
    let sel_meta_img = Selector::parse("meta[property=\"og:image\"]").unwrap();
    let sel_meta_url = Selector::parse("meta[property=\"og:url\"]").unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string());
    let description = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&sel_meta_img)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let id = doc
        .select(&sel_meta_url)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok((title, description, image, id))
}

async fn fetch_blog_posts(limit: usize) -> Result<Vec<HubItem>> {
    let api_url = format!("{}/api/blog?paginate={}&cursor=0", BASE_URL, limit);
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            api_url, status
        )));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;
    let posts = json
        .get("posts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();
    for p in posts.into_iter().take(limit) {
        let title = p
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if title.is_empty() {
            continue;
        }
        let slug = p.get("slug").and_then(|v| v.as_str()).unwrap_or_default();
        let link = if slug.is_empty() {
            None
        } else {
            Some(format!("{}/blog/{}", BASE_URL, slug))
        };
        let summary = p
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let image = p
            .get("images")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.get(0))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let date_raw = p.get("date").and_then(|v| {
            if v.is_string() {
                v.as_str().map(|s| s.to_string())
            } else if v.is_number() {
                Some(v.to_string())
            } else {
                None
            }
        });
        let pub_date = date_raw.as_deref().and_then(parse_pub_date);

        let mut html_desc = String::new();
        if let Some(img) = &image {
            html_desc.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = img,
                alt = title
            ));
        }
        if !summary.is_empty() {
            if !html_desc.is_empty() {
                html_desc.push_str("<p></p>");
            }
            html_desc.push_str(&format!("<p>{}</p>", summary));
        }

        items.push(HubItem {
            title,
            description: if html_desc.is_empty() {
                None
            } else {
                Some(html_desc)
            },
            link,
            author: None,
            pub_date,
            categories: p
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;
    let (title, description, image, id) = fetch_blog_meta().await?;
    let items = fetch_blog_posts(limit).await?;
    let link = format!("{}/blog", BASE_URL);

    Ok(HubData {
        title: title.unwrap_or_else(|| "Windsurf Blog".to_string()),
        description: description
            .or_else(|| Some("Official posts from the Windsurf blog.".to_string())),
        link: Some(link),
        image,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_WINDSURF_BLOG: Route = Route {
    meta: &META_WINDSURF_BLOG,
    handler: handler_fn,
};
